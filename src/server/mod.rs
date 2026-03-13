//! Web API server module

pub mod admin;
pub mod auth;
pub mod e2e_test;
pub mod gray_release;
pub mod quota;
pub mod rate_limit;
pub mod types;
pub mod user_id;

use std::sync::Arc;
use axum::{
    routing::{get, post, put},
    Router,
    extract::{State, Multipart},
    http::StatusCode,
    Json,
};
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
    limit::RequestBodyLimitLayer,
};
use tracing::{info, error, warn};
use utoipa::OpenApi;
use crate::{
    infrastructure::external::ApiClient,
    config::{Config, ConfigManager},
    telemetry::TelemetryRecorder,
    db::DbPool,
    server::{
        auth::{AuthState, api_key_auth},
        rate_limit::{RateLimitState, rate_limit_middleware},
        gray_release::GrayReleaseConfig,
        quota::QuotaState,
        admin::{get_current_user_quota, create_api_key},
        types::{
            ApiResponse, HealthResponse, AnalyzeResponse,
            ChatRequest, ChatResponse, StatsResponse,
            Validator, ErrorCode,
        },
    },
};

/// Server state
pub struct ServerState {
    pub auth_state: Arc<AuthState>,
    pub rate_limit: Arc<RateLimitState>,
    pub config: Arc<ConfigManager>,
    pub telemetry: TelemetryRecorder,
    pub api_client: ApiClient,
    pub gray_release: Arc<tokio::sync::RwLock<GrayReleaseConfig>>,
    pub quota_state: Arc<QuotaState>,
    pub db: Option<DbPool>,
}

impl ServerState {
    pub fn new(
        config: Config,
        telemetry: TelemetryRecorder,
        api_client: ApiClient,
        gray_release: Arc<GrayReleaseConfig>,
        quota_state: Arc<QuotaState>,
    ) -> Self {
        Self {
            auth_state: Arc::new(AuthState::new()),
            // Use rate limit configuration from config file
            rate_limit: Arc::new(RateLimitState::new(
                config.rate_limit_requests_per_second,
                config.rate_limit_burst_multiplier,
            )),
            config: Arc::new(ConfigManager::new(config)),
            telemetry,
            api_client,
            gray_release: Arc::new(tokio::sync::RwLock::new((*gray_release).clone())),
            quota_state,
            db: None,
        }
    }

    pub async fn initialize(&mut self) {
        // 初始化数据库（从配置或环境变量获取 URL）
        let config = self.config.get();
        let db_url = config.database_url.clone();
        drop(config);

        match crate::db::init_database(db_url.as_deref()).await {
            Ok(db) => {
                info!("Database initialized successfully (type: {})", db.db_type());

                // 更新 quota state 使用数据库
                let quota_state = QuotaState::new(self.quota_state.default_daily_limit)
                    .with_db(db.clone())
                    .with_fallback_policy(self.quota_state.fallback_policy);
                self.quota_state = Arc::new(quota_state);

                // 更新 auth state 使用数据库
                let auth_state = AuthState::new().with_db(db.clone());
                self.auth_state = Arc::new(auth_state);

                // 更新 telemetry recorder 使用数据库
                self.telemetry = TelemetryRecorder::with_database(
                    Some(self.telemetry.user_id().to_string()),
                    db.clone(),
                );

                self.db = Some(db);
            }
            Err(e) => {
                warn!("Database initialization failed: {}, will use in-memory storage", e);
                self.db = None;
            }
        }
    }
}

/// Create API router
pub fn create_router(state: Arc<ServerState>) -> Router {
    use crate::server::gray_release::check_gray_access;
    use crate::server::admin::{
        generate_api_key, revoke_api_key, list_api_keys, rotate_api_key,
        get_user_quota, update_user_quota,
        get_gray_release_config, update_gray_release_config,
        get_system_stats, export_metrics,
    };
    use crate::server::quota::quota_middleware;

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Admin routes (require authentication)
    let admin_routes = Router::new()
        // API Key management
        .route("/keys", get(list_api_keys))
        .route("/keys/generate", post(generate_api_key))
        .route("/keys/rotate", post(rotate_api_key))
        .route("/keys/revoke", post(revoke_api_key))
        // User quota management
        .route("/users/:user_id/quota", get(get_user_quota))
        .route("/users/:user_id/quota", put(update_user_quota))
        // Gray release config management
        .route("/gray-release", get(get_gray_release_config))
        .route("/gray-release", put(update_gray_release_config))
        // System stats (admin only)
        .route("/admin/stats", get(get_system_stats))
        // Debug health check (requires authentication)
        .route("/debug/health", get(debug_health_check))
        .layer(axum::middleware::from_fn_with_state(
            state.auth_state.clone(),
            api_key_auth,
        ));

    // Prometheus metrics (public, no authentication required)
    let metrics_route = Router::new()
        .route("/metrics", get(export_metrics));

    // Protected API routes
    // Middleware execution order (outer to inner / first to last):
    // 1. Rate limiting (outermost) - prevent DDoS, applies to all requests
    // 2. Authentication - verify API Key
    // 3. Gray release check - check if user is in gray release list
    // 4. Quota check (innermost) - check user quota before processing
    let protected_routes = Router::new()
        .route("/stats", get(get_stats))
        .route("/analyze", post(analyze_drawing))
        .route("/chat", post(chat))
        .route("/quota", get(get_current_user_quota))
        .route("/api-keys", post(create_api_key))
        // Layer 4 (innermost): Quota check
        .layer(axum::middleware::from_fn_with_state(
            state.quota_state.clone(),
            quota_middleware,
        ))
        // Layer 3: Gray release check
        .layer(axum::middleware::from_fn_with_state(
            state.gray_release.clone(),
            check_gray_access,
        ))
        // Layer 2: Authentication
        .layer(axum::middleware::from_fn_with_state(
            state.auth_state.clone(),
            api_key_auth,
        ))
        // Layer 1 (outermost): Rate limiting
        .layer(axum::middleware::from_fn_with_state(
            state.rate_limit.clone(),
            rate_limit_middleware,
        ));

    // Public routes (health check doesn't require authentication)
    let public_routes = Router::new()
        .route("/health", get(health_check));

    // Merge all routes and set state
    Router::new()
        .nest("/api/v1", admin_routes.merge(protected_routes).merge(public_routes).merge(metrics_route))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .layer(RequestBodyLimitLayer::new(10 * 1024 * 1024)) // 10MB
        .with_state(state)
}

/// Health check handler - simple status only (public endpoint)
async fn health_check(
    State(state): State<Arc<ServerState>>,
) -> Json<ApiResponse<HealthResponse>> {
    let config = state.config.get();
    // 只返回基本状态，不暴露内部细节
    let status = if config.default_local_model.is_empty() && config.default_cloud_model.is_empty() {
        "degraded"
    } else {
        "healthy"
    };

    let response = HealthResponse {
        status: status.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        checks: None, // 公开端点不返回详细检查信息
    };
    Json(ApiResponse::success(response))
}

/// Detailed health check handler - deep check all dependencies (requires authentication)
async fn debug_health_check(
    State(state): State<Arc<ServerState>>,
) -> Json<ApiResponse<HealthResponse>> {
    let mut status = "healthy";
    let mut checks: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    // Check database connection (if configured)
    if let Some(ref db) = state.db {
        match db.is_healthy().await {
            true => {
                checks.insert("database".to_string(), "connected".to_string());
            }
            false => {
                status = "unhealthy";
                checks.insert("database".to_string(), "disconnected".to_string());
                tracing::error!("Health check - database connection failed");
            }
        }
    } else {
        checks.insert("database".to_string(), "not configured".to_string());
    }

    // Check API client (by checking config)
    let config = state.config.get();
    if config.default_local_model.is_empty() && config.default_cloud_model.is_empty() {
        status = "degraded";
        checks.insert("api_client".to_string(), "no model configured".to_string());
    } else {
        checks.insert("api_client".to_string(), "configured".to_string());
    }
    drop(config);

    // Check quota system
    if state.quota_state.is_db_failed().await {
        status = "degraded";
        checks.insert(
            "quota_system".to_string(),
            "degraded: using memory fallback".to_string()
        );
    } else if state.quota_state.db.is_some() {
        checks.insert("quota_system".to_string(), "enabled".to_string());
    } else {
        checks.insert("quota_system".to_string(), "memory mode".to_string());
    }

    let response = HealthResponse {
        status: status.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        checks: Some(checks),
    };
    Json(ApiResponse::success(response))
}

/// Stats handler
async fn get_stats(
    State(state): State<Arc<ServerState>>,
) -> Json<ApiResponse<StatsResponse>> {
    // Get stats from telemetry
    let stats = state.telemetry.get_stats().await;
    let response = StatsResponse {
        total_requests: stats.total_requests,
        successful_requests: stats.successful_requests,
        failed_requests: stats.failed_requests,
        avg_latency_ms: stats.avg_latency_ms,
    };
    Json(ApiResponse::success(response))
}

/// Drawing analysis handler (支持自动模板选择)
async fn analyze_drawing(
    State(state): State<Arc<ServerState>>,
    multipart: Multipart,
) -> Result<Json<ApiResponse<AnalyzeResponse>>, (StatusCode, Json<ApiResponse<AnalyzeResponse>>)> {
    use std::time::Instant;
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    use crate::security::{validate_image_content, AllowedImageType};

    let start = Instant::now();

    // Parse multipart form
    let mut image_base64: Option<String> = None;
    let config = state.config.get();
    let mut question = "Please analyze this drawing".to_string();
    drop(config);

    let mut form = multipart;
    while let Some(field) = form.next_field().await.map_err(|e| {
        warn!("Multipart parse error: {}", e);
        (StatusCode::BAD_REQUEST, Json(ApiResponse::invalid_request("Multipart form parse failed".to_string())))
    })? {
        let name = field.name().unwrap_or("");
        match name {
            "image" => {
                let bytes = field.bytes().await.map_err(|e| {
                    warn!("Failed to read image data: {}", e);
                    (StatusCode::BAD_REQUEST, Json(ApiResponse::invalid_request("Failed to read image data".to_string())))
                })?;

                // 安全校验：验证文件 MIME 类型，防止恶意文件上传
                let allowed_types = AllowedImageType::all();
                match validate_image_content(&bytes, allowed_types) {
                    Ok(image_type) => {
                        let mime = image_type.mime_type();
                        info!("Uploaded image MIME type: {}", mime);
                        image_base64 = Some(BASE64.encode(&bytes));
                    }
                    Err(e) => {
                        warn!("Image MIME type validation failed: {}", e);
                        return Err((
                            StatusCode::BAD_REQUEST,
                            Json(ApiResponse::invalid_request(format!("Invalid image file type: {}", e))),
                        ));
                    }
                }
            }
            "question" => {
                question = field.text().await.unwrap_or(question);
            }
            // drawing_type 参数已废弃，保留以兼容旧客户端（但不再使用）
            "drawing_type" => {
                info!("drawing_type parameter is deprecated and will be ignored (auto template selection is enabled)");
            }
            _ => {}
        }
    }

    // Validate image data
    let image_data = match image_base64 {
        Some(data) => {
            if let Err(e) = Validator::validate_image_base64(&data) {
                warn!("Image validation failed: {}", e);
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::invalid_request(e)),
                ));
            }
            data
        }
        None => {
            warn!("Missing image data");
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::invalid_request("Missing image data".to_string())),
            ));
        }
    };

    // Validate question
    if let Err(e) = Validator::validate_question(&question) {
        warn!("Question validation failed: {} - {}", question, e);
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::invalid_request(e)),
        ));
    }

    // 使用自动模板选择（如果配置启用）
    let use_auto_template = state.config.get().template_selection.enabled;
    let template_info = if use_auto_template {
        // 使用多模态模型自动选择模板类型
        "auto-selected".to_string()
    } else {
        // 使用默认类型（向后兼容）
        let config = state.config.get();
        let default_type = config.default_drawing_type.clone();
        drop(config);
        default_type
    };

    // Build prompt
    let prompt = format!("Drawing type: {}\n\n{}", template_info, question);
    let messages = vec![crate::infrastructure::external::Message::user_with_images(
        prompt,
        vec![image_data.clone()],
    )];

    // Call API
    match state.api_client.chat(&messages).await {
        Ok(content) => {
            let latency = start.elapsed().as_millis() as u64;
            let duration_secs = start.elapsed().as_secs_f64();

            // Record Prometheus metrics
            crate::metrics::GLOBAL_METRICS.record_request(duration_secs);

            // Record telemetry
            let config = state.config.get();
            let model = config.default_cloud_model.clone();
            drop(config);
            
            state.telemetry.log_request(
                "/api/v1/analyze",
                latency,
                true,
                Some(&model),
            ).await;

            let response = AnalyzeResponse {
                content,
                model,
                latency_ms: latency,
            };

            Ok(Json(ApiResponse::success(response)))
        }
        Err(e) => {
            let latency = start.elapsed().as_millis() as u64;

            // Record Prometheus error metrics
            crate::metrics::GLOBAL_METRICS.record_error();

            // Record error telemetry
            let config = state.config.get();
            let model = config.default_cloud_model.clone();
            drop(config);
            
            state.telemetry.log_request(
                "/api/v1/analyze",
                latency,
                false,
                Some(&model),
            ).await;

            error!("Analyze error: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(ErrorCode::ModelError, format!("AI service call failed: {}", e))),
            ))
        }
    }
}

/// Chat handler
async fn chat(
    State(state): State<Arc<ServerState>>,
    Json(payload): Json<ChatRequest>,
) -> Result<Json<ApiResponse<ChatResponse>>, (StatusCode, Json<ApiResponse<ChatResponse>>)> {
    use std::time::Instant;

    let start = Instant::now();

    // Validate message
    if let Err(e) = Validator::validate_message(&payload.message) {
        warn!("Message validation failed: {} - {}", payload.message, e);
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::invalid_request(e)),
        ));
    }

    // Validate session ID (if provided)
    if let Some(ref session_id) = payload.session_id {
        if let Err(e) = Validator::validate_session_id(session_id) {
            warn!("Session ID validation failed: {} - {}", session_id, e);
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::invalid_request(e)),
            ));
        }
    }

    // Validate images (if provided)
    if let Some(ref images) = payload.images {
        for (i, image) in images.iter().enumerate() {
            if let Err(e) = Validator::validate_image_base64(image) {
                warn!("Image #{} validation failed: {}", i, e);
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::invalid_request(format!("Image #{} validation failed: {}", i, e))),
                ));
            }
        }
    }

    // Build message
    let messages = if let Some(images) = payload.images {
        vec![crate::infrastructure::external::Message::user_with_images(payload.message, images)]
    } else {
        vec![crate::infrastructure::external::Message::user(payload.message)]
    };

    // Call API
    match state.api_client.chat(&messages).await {
        Ok(content) => {
            let latency = start.elapsed().as_millis() as u64;
            let duration_secs = start.elapsed().as_secs_f64();

            // Record Prometheus metrics
            crate::metrics::GLOBAL_METRICS.record_request(duration_secs);

            // Record telemetry
            let config = state.config.get();
            let model = config.default_cloud_model.clone();
            drop(config);
            
            state.telemetry.log_request(
                "/api/v1/chat",
                latency,
                true,
                Some(&model),
            ).await;

            let response = ChatResponse {
                content,
                session_id: payload.session_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                latency_ms: latency,
            };

            Ok(Json(ApiResponse::success(response)))
        }
        Err(e) => {
            let latency = start.elapsed().as_millis() as u64;

            // Record Prometheus error metrics
            crate::metrics::GLOBAL_METRICS.record_error();

            // Record error telemetry
            let config = state.config.get();
            let model = config.default_cloud_model.clone();
            drop(config);
            
            state.telemetry.log_request(
                "/api/v1/chat",
                latency,
                false,
                Some(&model),
            ).await;

            error!("Chat error: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(ErrorCode::ModelError, format!("AI service call failed: {}", e))),
            ))
        }
    }
}

/// Start server
pub async fn start_server(config: Config, port: u16) -> std::io::Result<()> {
    use tokio::net::TcpListener;

    // Load API Key (load once at startup)
    let api_key = std::env::var("OLLAMA_API_KEY")
        .map_err(|e| std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("OLLAMA_API_KEY environment variable must be set: {}", e)
        ))?;

    // Initialize telemetry
    let telemetry = TelemetryRecorder::new(None);

    // Create API client
    let api_client = ApiClient::cloud(
        &config.default_cloud_model,
        &api_key,
        3,
    );

    // Load gray release config
    let gray_release = Arc::new(load_gray_release_config(&config));

    // Create quota state
    // Safety first: use Reject policy by default, reject service instead of falling back to memory mode when database fails
    let fallback_policy = match config.quota_fallback_policy.as_str() {
        "memory" => crate::server::quota::QuotaFallbackPolicy::MemoryMode,
        _ => crate::server::quota::QuotaFallbackPolicy::Reject,  // Default Reject (safety first)
    };
    let quota_state = Arc::new(
        QuotaState::new(config.quota_daily_limit)
            .with_fallback_policy(fallback_policy)
    );

    // Create server state (database initialized in initialize)
    let mut state = Arc::new(ServerState::new(
        config,
        telemetry,
        api_client,
        gray_release,
        quota_state,
    ));

    // Initialize (load API Keys, database, etc.)
    // Safety: This is the only reference to state, so get_mut will succeed
    if let Some(state_mut) = Arc::get_mut(&mut state) {
        state_mut.initialize().await;
    } else {
        error!("Failed to get mutable reference to state for initialization");
        return Err(std::io::Error::other(
            "State initialization failed"
        ));
    }

    let state = state; // Transfer ownership

    // Create OpenAPI documentation
    #[derive(OpenApi)]
    #[openapi(
        info(
            title = "CAD OCR API",
            description = "CAD Drawing Recognition and Analysis API",
            version = "0.9.0"
        ),
        tags(
            (name = "health", description = "Health check endpoints"),
            (name = "analyze", description = "Drawing analysis endpoints"),
            (name = "chat", description = "Chat endpoints"),
            (name = "admin", description = "Admin endpoints"),
            (name = "metrics", description = "Prometheus metrics")
        )
    )]
    struct ApiDoc;

    // Output OpenAPI spec to file (for documentation purposes)
    let openapi = ApiDoc::openapi();
    let openapi_json = openapi.to_pretty_json().unwrap_or_default();
    if let Err(e) = std::fs::write("openapi.json", &openapi_json) {
        warn!("Failed to write OpenAPI spec: {}", e);
    } else {
        info!("OpenAPI specification written to openapi.json");
    }

    // Create router
    let app = create_router(state.clone());

    // Start server
    let addr = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&addr).await?;

    info!("Server started at: {}", addr);
    info!("Health check endpoint: http://{}/api/v1/health", addr);
    info!("Use Authorization: Bearer <API_KEY> to access API");

    let gray_config = state.gray_release.read().await;
    if gray_config.enabled {
        info!("Gray release enabled, whitelist user count: {}", gray_config.whitelist.len());
        info!("Gray release quota: {} times/day", gray_config.quota_per_user);
    }
    drop(gray_config);

    // 配置在启动时一次性加载，生产环境配置变更应通过重启完成
    info!("Configuration loaded at startup, restart required for changes");

    // 优雅关闭处理
    let server = axum::serve(listener, app);
    let server_handle = server.with_graceful_shutdown(shutdown_signal(state.clone()));

    server_handle.await
}

/// Shutdown signal handler - 处理优雅关闭
async fn shutdown_signal(state: Arc<ServerState>) {
    use tokio::signal;

    let ctrl_c = async {
        if let Err(e) = signal::ctrl_c().await {
            error!("failed to install Ctrl+C handler: {}", e);
        }
    };

    #[cfg(windows)]
    let terminate = async {
        match signal::windows::ctrl_c() {
            Ok(mut s) => { s.recv().await; }
            Err(e) => error!("failed to install signal handler: {}", e),
        }
    };

    #[cfg(not(windows))]
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut s) => { s.recv().await; }
            Err(e) => error!("failed to install signal handler: {}", e),
        }
    };

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C signal, shutting down gracefully...");
        }
        _ = terminate => {
            info!("Received terminate signal, shutting down gracefully...");
        }
    }

    // 1. 等待进行中的请求完成（这里简化处理，实际可以加等待逻辑）
    info!("Waiting for in-flight requests to complete...");
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    // 2. 刷盘遥测数据（如果有 WAL）
    info!("Flushing telemetry data...");
    state.telemetry.clear_wal().await;

    // 3. 数据库连接会在进程退出时自动关闭
    info!("Database connections will be closed automatically");

    info!("Graceful shutdown completed");
}

#[cfg(test)]
pub fn create_router_for_test(state: Arc<ServerState>) -> Router {
    create_router(state)
}

/// Load gray release config
fn load_gray_release_config(config: &crate::config::Config) -> GrayReleaseConfig {
    // Try to load from environment variables
    let enabled = std::env::var("GRAY_RELEASE_ENABLED")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);

    let whitelist_str = std::env::var("GRAY_RELEASE_WHITELIST")
        .unwrap_or_default();

    let whitelist = whitelist_str
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| s.trim().to_string())
        .collect();

    // Use config file value or environment variable or default
    let quota_per_user = std::env::var("GRAY_RELEASE_QUOTA_PER_USER")
        .and_then(|v| v.parse::<u32>().map_err(|_| std::env::VarError::NotPresent))
        .unwrap_or(config.gray_release_quota_per_user);

    GrayReleaseConfig {
        enabled,
        whitelist,
        quota_per_user,
    }
}
