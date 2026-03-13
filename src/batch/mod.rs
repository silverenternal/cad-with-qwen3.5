//! 批量处理模块
//!
//! 包含会话池、熔断器、死信队列等组件

pub mod session;
pub mod circuit_breaker;
pub mod dead_letter_queue;
pub mod progress;
pub mod planner;
pub mod merger;
pub mod stream_processor;
pub mod concurrency_controller;

pub use session::SessionPool;
pub use circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitState, SharedCircuitBreaker};
pub use dead_letter_queue::{DeadLetterQueue, FailedFile, SharedDeadLetterQueue};
pub use progress::{BatchProgress, BatchPlan, BatchStatus, ProgressGuard};
pub use planner::{ProcessingPlan, BatchConfig, create_processing_plan, PdfInfo, BatchPlan as PlannerBatchPlan};
pub use merger::{FinalResult, TempDirGuard, create_temp_dir};
pub use stream_processor::{StreamBatchProcessor, StreamBatchProcessorConfig};
pub use concurrency_controller::{ConcurrencyController, ConcurrencyStats};

use crate::infrastructure::external::ApiClient;
use crate::batch_result::{BatchResult, FileResult, FileStatus, OutputFormat};
use crate::error::{Result, Error};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tracing::{info, warn, error};

/// 批量处理错误类型
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum BatchError {
    /// 可重试错误
    Retryable(String),
    /// 不可重试错误（认证、模型等）
    Fatal(String),
    /// 图片损坏
    ImageCorrupted(String),
    /// 配额不足
    QuotaExceeded {
        required: usize,
        remaining: u32,
    },
}

impl BatchError {
    /// 判断是否可重试
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Retryable(_))
    }

    /// 从应用错误创建批量错误
    pub fn from_app_error(e: &crate::error::Error) -> Self {
        match e {
            // 验证错误 - 不可重试
            Error::Validation(msg) => {
                if msg.contains("配额") {
                    // 配额错误特殊处理
                    Self::QuotaExceeded {
                        required: 1,
                        remaining: 0,
                    }
                } else {
                    Self::Fatal(msg.clone())
                }
            }
            // 资源不存在 - 不可重试
            Error::NotFound(msg) => Self::Fatal(msg.clone()),
            // 认证/授权错误 - 不可重试
            Error::Unauthorized(msg) => Self::Fatal(msg.clone()),
            // 外部服务错误 - 可重试
            Error::External(msg) => {
                if msg.contains("请求过于频繁") || msg.contains("RATE_LIMIT") || msg.contains("429") {
                    Self::Retryable(msg.clone())
                } else {
                    Self::Retryable(msg.clone())
                }
            }
            // 内部错误 - 通常不可重试，但超时和网络错误除外
            Error::Internal(msg) => {
                if msg.contains("timeout") || msg.contains("timed out") || msg.contains("network") {
                    Self::Retryable(msg.clone())
                } else {
                    Self::Fatal(msg.clone())
                }
            }
        }
    }
}

impl std::fmt::Display for BatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BatchError::Retryable(msg) => write!(f, "Retryable error: {}", msg),
            BatchError::Fatal(msg) => write!(f, "Fatal error: {}", msg),
            BatchError::ImageCorrupted(msg) => write!(f, "Image corrupted: {}", msg),
            BatchError::QuotaExceeded { required, remaining } => {
                write!(f, "Quota exceeded: required {}, remaining {}", required, remaining)
            }
        }
    }
}

/// 支持的图片扩展名（包括 PDF）
const SUPPORTED_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "bmp", "pdf"];

/// 模板识别配置（BatchProcessor 专用）
#[derive(Debug, Clone)]
pub struct BatchTemplateSelectionConfig {
    /// 是否启用每张图片独立识别
    pub enabled: bool,
    /// 分类策略："hybrid" | "multimodal" | "rule_based"
    pub strategy: String,
    /// 分类模型（独立于主分析模型）
    pub model: String,
    /// 分类置信度阈值
    pub confidence_threshold: f32,
    /// 低置信度时的回退策略："default_type" | "manual_review"
    pub low_confidence_fallback: String,
    /// 默认类型（回退时使用）
    pub default_type: String,
    /// 是否启用分类缓存
    pub enable_cache: bool,
    /// 缓存大小（条目数）
    pub cache_max_entries: usize,
}

impl Default for BatchTemplateSelectionConfig {
    fn default() -> Self {
        Self {
            enabled: false, // 默认关闭，向后兼容
            strategy: "hybrid".to_string(),
            model: "llava:7b".to_string(),
            confidence_threshold: 0.6,
            low_confidence_fallback: "default_type".to_string(),
            default_type: "culvert_layout".to_string(),
            enable_cache: true,
            cache_max_entries: 1000,
        }
    }
}

impl BatchTemplateSelectionConfig {
    /// 从全局 TemplateSelectionConfig 转换
    pub fn from_global_config(global: &crate::config::TemplateSelectionConfig) -> Self {
        Self {
            enabled: global.enabled,
            strategy: global.strategy.clone(),
            model: global.model.clone(),
            confidence_threshold: global.confidence_threshold,
            low_confidence_fallback: "default_type".to_string(),
            default_type: global.default_type.clone(),
            enable_cache: global.enable_cache,
            cache_max_entries: global.cache_max_entries,
        }
    }
}

/// 批量处理器配置
#[derive(Debug, Clone)]
pub struct BatchProcessorConfig {
    /// 会话池大小（并发会话数）
    pub session_pool_size: usize,
    /// 图片编码并发数
    pub encoding_concurrency: usize,
    /// API 请求并发数
    pub api_concurrency: usize,
    /// 最大图片尺寸
    pub max_image_dimension: u32,
    /// 最大重试次数
    pub max_retries: u32,
    /// 基础延迟 (ms)
    pub base_delay_ms: u64,
    /// 图纸类型
    pub drawing_type: String,
    /// 问题
    pub question: String,
    /// 用户 ID（用于配额检查）
    pub user_id: Option<String>,
    /// 是否启用配额检查
    pub enable_quota_check: bool,
    /// 熔断器配置
    pub circuit_breaker: CircuitBreakerConfig,
    /// 死信队列持久化路径
    pub dead_letter_queue_path: Option<PathBuf>,
    /// 工作目录（用于路径安全校验）
    pub working_dir: Option<PathBuf>,
    /// 模板识别配置
    pub template_selection: BatchTemplateSelectionConfig,
}

impl Default for BatchProcessorConfig {
    fn default() -> Self {
        Self {
            session_pool_size: 4,
            encoding_concurrency: 2,
            api_concurrency: 3,
            max_image_dimension: 2048,
            max_retries: 3,
            base_delay_ms: 100,
            drawing_type: "CAD 图纸".to_string(),
            question: "请分析这张图片".to_string(),
            user_id: None,
            enable_quota_check: false,  // CLI 模式默认禁用
            circuit_breaker: CircuitBreakerConfig::default(),
            dead_letter_queue_path: None,
            working_dir: None,
            template_selection: BatchTemplateSelectionConfig::default(),
        }
    }
}

impl BatchProcessorConfig {
    /// 从全局配置构建批量处理配置
    pub fn from_global_config(global: &crate::config::Config) -> Self {
        let ts = &global.template_selection;
        let concurrency = global.get_concurrency_config();
        let validator_preset = global.get_validator_preset();

        Self {
            session_pool_size: concurrency.batch_concurrency,
            encoding_concurrency: concurrency.encoding_concurrency,
            api_concurrency: concurrency.api_concurrency,
            max_image_dimension: global.max_image_dimension,
            max_retries: validator_preset.max_retries,
            base_delay_ms: validator_preset.initial_delay_ms,
            drawing_type: "auto_detected".to_string(),
            question: global.default_batch_question.clone(),
            user_id: None,
            enable_quota_check: false,
            circuit_breaker: crate::batch::circuit_breaker::CircuitBreakerConfig::default(),
            dead_letter_queue_path: global.dead_letter_queue_path.as_ref().map(PathBuf::from),
            working_dir: None,
            template_selection: BatchTemplateSelectionConfig::from_global_config(ts),
        }
    }
}

/// 批量处理器 - 使用会话池和熔断器
pub struct BatchProcessor {
    /// 会话池
    session_pool: SessionPool,
    /// 熔断器
    circuit_breaker: SharedCircuitBreaker,
    /// 死信队列
    dead_letter_queue: SharedDeadLetterQueue,
    /// 配置
    config: BatchProcessorConfig,
    /// 图片编码信号量
    encoding_semaphore: Arc<Semaphore>,
    /// 配额检查器（可选）
    quota_checker: Option<Arc<dyn QuotaChecker>>,
    /// Prometheus 指标（可选）
    metrics: Option<Arc<crate::metrics::Metrics>>,
    /// 路径安全守卫
    path_guard: Option<crate::security::path_middleware::PathGuard>,
}

/// 配额检查器 trait（用于依赖注入）
#[async_trait::async_trait]
pub trait QuotaChecker: Send + Sync {
    /// 检查并扣减配额
    async fn check_and_consume(&self, user_id: &str, count: usize) -> std::result::Result<(), String>;
    /// 获取用户剩余配额
    async fn get_remaining(&self, user_id: &str) -> u32;
}

impl BatchProcessor {
    /// 创建新的批量处理器（不带配额检查）
    pub fn new(
        base_client: ApiClient,
        config: BatchProcessorConfig,
    ) -> Self {
        // 使用配置中的熔断器配置
        let circuit_breaker = Arc::new(CircuitBreaker::new(config.circuit_breaker.clone()));

        // 使用配置中的死信队列持久化路径
        let dead_letter_queue = if let Some(ref path) = config.dead_letter_queue_path {
            Arc::new(DeadLetterQueue::with_persistence(path))
        } else {
            Arc::new(DeadLetterQueue::new())
        };

        // 根据配置创建会话池（固定类型模式 or 每图识别模式）
        let session_pool = if config.template_selection.enabled {
            // 每图识别模式
            let classification_config = Self::build_classification_config(&config);
            SessionPool::with_per_image_classification(
                base_client,
                config.session_pool_size,
                config.question.clone(),
                config.max_image_dimension,
                config.max_retries,
                config.base_delay_ms,
                classification_config,
            )
        } else {
            // 固定类型模式（向后兼容）
            SessionPool::new(
                base_client,
                config.session_pool_size,
                config.drawing_type.clone(),
                config.question.clone(),
                config.max_image_dimension,
                config.max_retries,
                config.base_delay_ms,
            )
        };

        // 初始化路径安全守卫（使用工作目录作为根目录）
        let path_guard = config.working_dir.as_ref()
            .map(crate::security::path_middleware::PathGuard::new);

        Self {
            session_pool,
            circuit_breaker,
            dead_letter_queue,
            config: config.clone(),
            encoding_semaphore: Arc::new(Semaphore::new(config.encoding_concurrency)),
            quota_checker: None,
            metrics: None,
            path_guard,
        }
    }

    /// 构建分类器配置
    fn build_classification_config(config: &BatchProcessorConfig) -> crate::infrastructure::template_selection::HybridClassifierConfig {
        use crate::infrastructure::template_selection::{HybridClassifierConfig, ClassificationStrategy, TemplateCacheConfig};
        use crate::domain::model::drawing::CulvertType;
        use std::str::FromStr;

        let ts = &config.template_selection;

        // 解析分类策略
        let strategy = ClassificationStrategy::from_str(&ts.strategy)
            .unwrap_or(ClassificationStrategy::Hybrid);

        // 解析默认类型
        let default_type = CulvertType::from_internal_id(&ts.default_type)
            .unwrap_or(CulvertType::CulvertLayout);

        HybridClassifierConfig {
            strategy,
            rule_confidence_high: ts.confidence_threshold,
            multimodal_confidence_threshold: ts.confidence_threshold,
            default_type,
            mark_low_confidence_for_review: ts.low_confidence_fallback == "manual_review",
            enable_logging: true,
            cache_config: TemplateCacheConfig {
                max_entries: ts.cache_max_entries,
                ttl_seconds: 3600,
                enabled: ts.enable_cache,
            },
            classification_model: ts.model.clone(),
        }
    }

    /// 创建带配额检查的批量处理器
    pub fn with_quota_checker(
        base_client: ApiClient,
        config: BatchProcessorConfig,
        quota_checker: Arc<dyn QuotaChecker>,
    ) -> Self {
        let mut processor = Self::new(base_client, config);
        processor.quota_checker = Some(quota_checker);
        processor
    }

    /// 设置指标收集器
    pub fn with_metrics(mut self, metrics: Arc<crate::metrics::Metrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// 处理目录中的所有图片
    pub async fn process_directory(
        &self,
        dir: &Path,
        output_path: Option<&Path>,
        progress_file: Option<&Path>,
    ) -> Result<BatchResult> {
        info!("Scanning directory: {}", dir.display());

        // 使用路径守卫扫描目录（如果已配置）
        let files = if let Some(ref guard) = self.path_guard {
            guard.read_dir(dir.to_str().unwrap_or(""))?
        } else {
            // 未配置路径守卫时使用传统方法（向后兼容）
            self.scan_images(dir)?
        };
        info!("Found {} image files", files.len());

        if files.is_empty() {
            warn!("No image files found in directory");
            return Ok(BatchResult::new(BatchResult::generate_id(), chrono::Utc::now()));
        }

        // 检查是否有已存在的进度文件（断点续传）
        let result = if let Some(path) = progress_file.or(output_path) {
            if let Some(existing) = BatchResult::load_from_file(path) {
                info!("Found existing progress file, resuming: {}", path.display());
                let processed_files: std::collections::HashSet<_> = existing.results
                    .iter()
                    .map(|r| r.file.clone())
                    .collect();

                // 过滤掉已处理的文件
                let remaining_files: Vec<_> = files.into_iter()
                    .filter(|f| {
                        let file_name = f.file_name().and_then(|n| n.to_str()).unwrap_or("unknown");
                        !processed_files.contains(file_name)
                    })
                    .collect();

                info!("Skipped {} processed files, {} files remaining",
                    processed_files.len(), remaining_files.len());

                let mut r = existing;
                self.process_files_internal(remaining_files, &mut r).await?;
                r
            } else {
                self.process_files(files).await?
            }
        } else {
            self.process_files(files).await?
        };

        Ok(result)
    }

    /// 处理多个文件
    async fn process_files(&self, files: Vec<PathBuf>) -> Result<BatchResult> {
        let batch_id = BatchResult::generate_id();
        let started_at = chrono::Utc::now();
        let mut result = BatchResult::new(batch_id, started_at);

        // 配额检查（仅当启用且配置了用户 ID 时）
        if self.config.enable_quota_check {
            if let Some(ref user_id) = self.config.user_id {
                if let Err(e) = self.check_quota(user_id, files.len()).await {
                    // 将 BatchError 转换为 Error
                    return Err(match e {
                        BatchError::QuotaExceeded { required, remaining } => {
                            Error::Validation(format!("配额不足：需要 {}，剩余 {}", required, remaining))
                        }
                        _ => Error::Internal(format!("配额检查失败：{}", e))
                    });
                }
            }
        }

        self.process_files_internal(files, &mut result).await?;

        Ok(result)
    }

    /// 检查配额
    async fn check_quota(&self, user_id: &str, required: usize) -> std::result::Result<(), BatchError> {
        // 使用注入的配额检查器
        if let Some(ref checker) = self.quota_checker {
            match checker.check_and_consume(user_id, required).await {
                Ok(()) => {
                    info!("Quota check passed for user {}: {} files", user_id, required);
                    Ok(())
                }
                Err(e) => {
                    let remaining = checker.get_remaining(user_id).await;
                    warn!("Quota check failed for user {}: {}", user_id, e);
                    Err(BatchError::QuotaExceeded {
                        required,
                        remaining,
                    })
                }
            }
        } else {
            // 没有配额检查器，跳过检查
            info!("No quota checker configured, skipping quota check");
            Ok(())
        }
    }

    /// 处理多个文件（内部实现）
    async fn process_files_internal(
        &self,
        files: Vec<PathBuf>,
        result: &mut BatchResult,
    ) -> Result<()> {
        let total_files = files.len();

        if total_files == 0 {
            return Ok(());
        }

        let batch_start = Instant::now();

        info!(
            "Batch processing {} files (sessions: {}, encoding concurrency: {})",
            files.len(),
            self.config.session_pool_size,
            self.config.encoding_concurrency
        );

        // 更新会话活跃指标
        if let Some(ref metrics) = self.metrics {
            metrics.set_batch_session_active(self.config.session_pool_size as i64);
        }

        // 创建进度条
        let pb = ProgressBar::new(total_files as u64);
        let template = ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) - {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_bar());
        pb.set_style(
            template
                .progress_chars("=>-"),
        );

        // 使用信号量控制 API 并发
        let api_semaphore = Arc::new(Semaphore::new(self.config.api_concurrency));
        let mut join_set = JoinSet::new();

        // 使用 SafeBatchResult 确保线程安全
        let safe_result = SafeBatchResult::new(result.clone());
        let safe_result_for_tasks = safe_result.clone_inner();

        // 提交所有任务
        for file_path in files {
            let api_permit = api_semaphore.clone().acquire_owned().await
                .map_err(|e| Error::Internal(format!("Semaphore error: {}", e)))?;

            let encoding_semaphore = self.encoding_semaphore.clone();
            let session = self.session_pool.next_session(); // 返回克隆的会话
            let pb_clone = pb.clone();
            let result_handle = safe_result_for_tasks.clone();
            let circuit_breaker = self.circuit_breaker.clone();
            let dead_letter_queue = self.dead_letter_queue.clone();
            let file_path_clone = file_path.clone();
            let metrics = self.metrics.clone();

            join_set.spawn(async move {
                let _api_permit = api_permit;
                let file_name = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown").to_string();
                pb_clone.set_message(format!("Processing {}", file_name));

                // 检查熔断器
                if !circuit_breaker.allow_request().await {
                    warn!("Circuit breaker is open, skipping file: {}", file_name);
                    let file_result = FileResult::failed(
                        file_name.clone(),
                        session.drawing_type.clone(),
                        session.question.clone(),
                        "Circuit breaker open - too many failures".to_string(),
                    );
                    result_handle.lock().await.add_result(file_result);
                    pb_clone.inc(1);
                    return;
                }

                // 获取编码许可
                let encoding_permit = match encoding_semaphore.acquire_owned().await {
                    Ok(p) => p,
                    Err(e) => {
                        error!("Encoding semaphore error: {}", e);
                        pb_clone.inc(1);
                        return;
                    }
                };

                // 处理文件
                let start = Instant::now();
                let _encoding_permit = encoding_permit;

                match session.process_with_retry(&file_path, &file_name).await {
                    Ok(answer) => {
                        circuit_breaker.record_success();
                        let latency_ms = start.elapsed().as_millis() as u64;
                        info!("✓ File {} processed successfully in {}ms", file_name, latency_ms);
                        
                        // 上报指标
                        if let Some(ref metrics) = metrics {
                            metrics.record_batch_file(true);
                            metrics.record_batch_duration(latency_ms as f64 / 1000.0);
                        }
                        
                        let file_result = FileResult::success(
                            file_name.clone(),
                            session.drawing_type.clone(),
                            session.question.clone(),
                            answer,
                            latency_ms,
                        );
                        result_handle.lock().await.add_result(file_result);
                    }
                    Err(e) => {
                        let batch_error = BatchError::from_app_error(&e);
                        circuit_breaker.record_failure().await;

                        // 添加到死信队列
                        dead_letter_queue
                            .add(file_path_clone, batch_error.clone(), session.max_retries)
                            .await;

                        error!("✗ File {} failed: {}", file_name, batch_error);
                        
                        // 上报指标
                        if let Some(ref metrics) = metrics {
                            metrics.record_batch_file(false);
                        }
                        
                        let file_result = FileResult::failed(
                            file_name.clone(),
                            session.drawing_type.clone(),
                            session.question.clone(),
                            batch_error.to_string(),
                        );
                        result_handle.lock().await.add_result(file_result);
                    }
                }

                pb_clone.inc(1);
            });
        }

        // 等待所有任务完成
        while let Some(res) = join_set.join_next().await {
            if let Err(e) = res {
                error!("Task join error: {}", e);
            }
        }

        pb.finish_with_message("Batch processing completed");

        // 更新原始 result
        let final_result = safe_result.into_inner().await;
        *result = final_result;

        // 上报批量处理完成指标
        if let Some(ref metrics) = self.metrics {
            let batch_duration = batch_start.elapsed().as_secs_f64();
            metrics.record_batch_duration(batch_duration);
            
            // 更新熔断器状态
            let cb_state = self.circuit_breaker.state();
            let state_value = match cb_state {
                CircuitState::Closed => 0,
                CircuitState::Open => 1,
                CircuitState::HalfOpen => 2,
            };
            metrics.set_batch_circuit_breaker_state(state_value);
            
            // 重置会话活跃数为 0
            metrics.set_batch_session_active(0);
        }

        // 记录死信队列信息
        let dlq_len = self.dead_letter_queue.len().await;
        if dlq_len > 0 {
            warn!("{} files failed and added to dead letter queue", dlq_len);
        }

        info!(
            "Batch completed: {} total, {} success, {} failed",
            result.total, result.success, result.failed
        );

        Ok(())
    }

    /// 扫描目录中的所有图片
    fn scan_images(&self, dir: &Path) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        if !dir.exists() {
            return Err(Error::Internal(format!("目录不存在：{}", dir.display())));
        }

        if !dir.is_dir() {
            return Err(Error::Internal(format!("不是目录：{}", dir.display())));
        }

        self.scan_recursive(dir, &mut files)?;
        files.sort();

        Ok(files)
    }

    /// 递归扫描目录（带路径安全检查）
    fn scan_recursive(&self, dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
        for entry in std::fs::read_dir(dir).map_err(|e| Error::Internal(e.to_string()))? {
            let entry = entry.map_err(|e| Error::Internal(e.to_string()))?;
            let path = entry.path();

            // 关键安全检查：确保文件在配置的根目录内
            if let Some(ref guard) = self.path_guard {
                if !path.starts_with(guard.root()) {
                    return Err(Error::Unauthorized(format!("文件不在根目录内：{} (root: {})", path.display(), guard.root().display())));
                }
            }

            if path.is_dir() {
                self.scan_recursive(&path, files)?;
            } else if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if SUPPORTED_EXTENSIONS.iter().any(|&e| e.eq_ignore_ascii_case(ext)) {
                        files.push(path);
                    }
                }
            }
        }
        Ok(())
    }

    /// 获取熔断器状态
    pub fn circuit_breaker_state(&self) -> CircuitState {
        self.circuit_breaker.state()
    }

    /// 获取死信队列中的失败文件
    pub async fn get_failed_files(&self) -> Vec<FailedFile> {
        self.dead_letter_queue.get_all().await
    }
}

/// 线程安全的批量结果包装器
#[derive(Clone)]
struct SafeBatchResult {
    inner: Arc<tokio::sync::Mutex<BatchResult>>,
}

impl SafeBatchResult {
    fn new(result: BatchResult) -> Self {
        Self {
            inner: Arc::new(tokio::sync::Mutex::new(result)),
        }
    }

    fn clone_inner(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }

    async fn lock(&self) -> tokio::sync::MutexGuard<'_, BatchResult> {
        self.inner.lock().await
    }

    async fn into_inner(self) -> BatchResult {
        // 注意：在并发场景下，如果还有其他强引用，try_unwrap 会失败
        // 这种情况不应该发生，因为 into_inner 应该在所有任务完成后调用
        match Arc::try_unwrap(self.inner) {
            Ok(mutex) => mutex.into_inner(),
            Err(_) => {
                // 如果还有强引用，创建一个默认结果
                // 这种情况表明代码逻辑有问题，应该记录错误
                tracing::error!("SafeBatchResult::into_inner called while still referenced");
                // 使用当前时间和 ID 创建一个空结果
                BatchResult::new(
                    uuid::Uuid::new_v4().to_string(),
                    chrono::Utc::now(),
                )
            }
        }
    }
}

/// 保存结果到文件
pub fn save_result(result: &BatchResult, output_path: &Path, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Json => save_result_json(result, output_path),
        OutputFormat::Csv => save_result_csv(result, output_path),
    }
}

/// 保存为 JSON 格式
fn save_result_json(result: &BatchResult, path: &Path) -> Result<()> {
    let json = serde_json::to_string_pretty(result)
        .map_err(|e| Error::Internal(format!("JSON 序列化错误：{}", e)))?;
    std::fs::write(path, json)
        .map_err(|e| Error::Internal(e.to_string()))?;
    info!("Results saved to: {}", path.display());
    Ok(())
}

/// 保存为 CSV 格式
fn save_result_csv(result: &BatchResult, path: &Path) -> Result<()> {
    use std::io::Write;

    let mut file = std::fs::File::create(path)
        .map_err(|e| Error::Internal(e.to_string()))?;

    // 写入表头
    writeln!(file, "file,drawing_type,question,status,answer,error,latency_ms")
        .map_err(|e| Error::Internal(e.to_string()))?;

    // 写入数据行
    for r in &result.results {
        let (answer, error, latency_ms) = match &r.status {
            FileStatus::Success { answer, latency_ms } => {
                (escape_csv_field(answer), String::new(), latency_ms.to_string())
            }
            FileStatus::Failed { error } => {
                (String::new(), escape_csv_field(error), String::new())
            }
        };

        writeln!(
            file,
            "{},{},{},{},{},{},{}",
            escape_csv_field(&r.file),
            escape_csv_field(&r.drawing_type),
            escape_csv_field(&r.question),
            match &r.status {
                FileStatus::Success { .. } => "success",
                FileStatus::Failed { .. } => "failed",
            },
            answer,
            error,
            latency_ms,
        ).map_err(|e| Error::Internal(e.to_string()))?;
    }

    info!("Results saved to: {}", path.display());
    Ok(())
}

/// 转义 CSV 字段（处理逗号、引号、换行）
fn escape_csv_field(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') || field.contains('\r') {
        // 需要转义：用双引号包裹，内部引号加倍
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::external::ApiClient;

    #[test]
    fn test_batch_processor_config_default() {
        let config = BatchProcessorConfig::default();
        assert_eq!(config.session_pool_size, 4);
        assert_eq!(config.encoding_concurrency, 2);
        assert_eq!(config.api_concurrency, 3);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.base_delay_ms, 100);
        assert!(!config.enable_quota_check);
        assert!(config.user_id.is_none());
    }

    #[test]
    fn test_batch_error_is_retryable() {
        let retryable = BatchError::Retryable("test".to_string());
        assert!(retryable.is_retryable());
        
        let fatal = BatchError::Fatal("test".to_string());
        assert!(!fatal.is_retryable());
        
        let quota = BatchError::QuotaExceeded { required: 10, remaining: 5 };
        assert!(!quota.is_retryable());
    }

    #[test]
    fn test_batch_error_display() {
        let err = BatchError::Retryable("network error".to_string());
        assert!(err.to_string().contains("Retryable error"));
        
        let err = BatchError::Fatal("auth failed".to_string());
        assert!(err.to_string().contains("Fatal error"));
        
        let err = BatchError::QuotaExceeded { required: 100, remaining: 50 };
        let msg = err.to_string();
        assert!(msg.contains("Quota exceeded"));
        assert!(msg.contains("100"));
        assert!(msg.contains("50"));
    }

    #[test]
    fn test_csv_escape() {
        assert_eq!(escape_csv_field("hello"), "hello");
        assert_eq!(escape_csv_field("hello,world"), "\"hello,world\"");
        assert_eq!(escape_csv_field("hello\"world"), "\"hello\"\"world\"");
        assert_eq!(escape_csv_field("hello\nworld"), "\"hello\nworld\"");
    }

    #[test]
    fn test_supported_extensions() {
        // 验证支持的扩展名常量
        assert!(SUPPORTED_EXTENSIONS.contains(&"jpg"));
        assert!(SUPPORTED_EXTENSIONS.contains(&"png"));
        assert!(SUPPORTED_EXTENSIONS.contains(&"webp"));
    }

    #[tokio::test]
    async fn test_dead_letter_queue() {
        let dlq = DeadLetterQueue::new();
        
        assert_eq!(dlq.len().await, 0);
        
        dlq.add(
            PathBuf::from("/test/file1.jpg"),
            BatchError::Retryable("test error".to_string()),
            3,
        )
        .await;
        
        assert_eq!(dlq.len().await, 1);
        
        let failed = dlq.get_all().await;
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].path, PathBuf::from("/test/file1.jpg"));
        
        dlq.clear().await;
        assert_eq!(dlq.len().await, 0);
    }

    #[tokio::test]
    async fn test_circuit_breaker_basic() {
        use std::time::Duration;
        
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout: Duration::from_millis(100),
            max_probes_in_half_open: 3,
        };
        let cb = CircuitBreaker::new(config);

        // 初始状态应为闭合
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.allow_request().await);

        // 记录失败
        cb.record_failure().await;
        assert_eq!(cb.state(), CircuitState::Closed); // 还未达到阈值
    }

    #[test]
    fn test_session_pool_creation() {
        let client = ApiClient::local("test-model", 3);
        let pool = SessionPool::new(
            client,
            4,  // pool_size
            "CAD".to_string(),
            "Analyze".to_string(),
            2048,
            3,
            100,
        );
        
        assert_eq!(pool.size(), 4);
        
        // 获取会话应该返回克隆
        let session1 = pool.next_session();
        let session2 = pool.next_session();
        
        // 两个会话应该是独立的克隆
        assert_eq!(session1.drawing_type, "CAD");
        assert_eq!(session2.drawing_type, "CAD");
    }

    #[test]
    fn test_batch_processor_creation() {
        let client = ApiClient::local("test-model", 3);
        let config = BatchProcessorConfig::default();
        
        let processor = BatchProcessor::new(client, config.clone());
        
        // 验证处理器创建成功
        assert_eq!(processor.circuit_breaker_state(), CircuitState::Closed);
    }
}
