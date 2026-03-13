//! 管理接口 - API Key 管理、用户配额、灰度配置

use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use tracing::info;
use crate::{
    server::{
        ServerState,
        types::{ApiResponse, ErrorCode},
        gray_release::GrayReleaseConfig,
    },
    metrics,
};
use serde::{Deserialize, Serialize};

/// API Key 信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyInfo {
    pub key_prefix: String,
}

/// 生成 API Key 请求
#[derive(Debug, Deserialize)]
pub struct GenerateApiKeyRequest {
    pub name: Option<String>,
    pub expires_in_days: Option<u32>,
}

/// 轮换 API Key 请求
#[derive(Debug, Deserialize)]
pub struct RotateApiKeyRequest {
    pub old_key: String,
    pub name: Option<String>,
    pub expires_in_days: Option<u32>,
    pub revoke_old: bool, // 是否立即撤销旧 key
}

/// 轮换 API Key 响应
#[derive(Debug, Serialize)]
pub struct RotateApiKeyResponse {
    pub new_key: String,
    pub new_key_prefix: String,
    pub old_key_prefix: String,
    pub old_key_revoked: bool,
    pub message: String,
}

/// 生成 API Key 响应
#[derive(Debug, Serialize)]
pub struct GenerateApiKeyResponse {
    pub key: String,
    pub key_prefix: String,
    pub message: String,
}

/// 列出 API Key 响应
#[derive(Debug, Serialize)]
pub struct ListApiKeysResponse {
    pub keys: Vec<ApiKeyInfo>,
    pub total: usize,
}

/// 用户配额信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserQuotaInfo {
    pub user_id: String,
    pub daily_limit: u32,
    pub used_today: u32,
    pub remaining: u32,
    pub is_exceeded: bool,
}

/// 更新用户配额请求
#[derive(Debug, Deserialize)]
pub struct UpdateUserQuotaRequest {
    pub daily_limit: u32,
}

/// 灰度配置信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrayReleaseConfigInfo {
    pub enabled: bool,
    pub whitelist: Vec<String>,
    pub quota_per_user: u32,
}

/// 更新灰度配置请求
#[derive(Debug, Deserialize)]
pub struct UpdateGrayReleaseConfigRequest {
    pub enabled: Option<bool>,
    pub whitelist: Option<Vec<String>>,
    pub quota_per_user: Option<u32>,
}

/// 系统统计信息
#[derive(Debug, Serialize)]
pub struct SystemStats {
    pub total_users: usize,
    pub total_api_keys: usize,
    pub gray_release_enabled: bool,
    pub default_quota: u32,
}

/// 生成新的 API Key
pub async fn generate_api_key(
    State(state): State<Arc<ServerState>>,
    Json(payload): Json<GenerateApiKeyRequest>,
) -> Result<Json<ApiResponse<GenerateApiKeyResponse>>, StatusCode> {
    // 生成随机 API Key
    let key = generate_random_key();
    let key_prefix = format!("{}...", &key[..8]);

    // 添加到认证状态
    state.auth_state.add_api_key(key.clone()).await;

    let response = GenerateApiKeyResponse {
        key: key.clone(),
        key_prefix,
        message: format!("API Key 生成成功，请妥善保管。{}", 
            if let Some(name) = payload.name {
                format!("名称：{}", name)
            } else {
                String::new()
            }
        ),
    };

    Ok(Json(ApiResponse::success(response)))
}

/// 撤销 API Key
pub async fn revoke_api_key(
    State(state): State<Arc<ServerState>>,
    Json(payload): Json<RevokeApiKeyRequest>,
) -> Result<Json<ApiResponse<()>>, StatusCode> {
    let key = payload.key;

    // 检查是否存在
    if !state.auth_state.contains_key(&key).await {
        return Ok(Json(ApiResponse::error(
            ErrorCode::NotFound,
            "API Key 不存在".to_string()
        )));
    }

    // 撤销
    state.auth_state.remove_api_key(&key).await;

    Ok(Json(ApiResponse::success(())))
}

/// 撤销 API Key 请求
#[derive(Debug, Deserialize)]
pub struct RevokeApiKeyRequest {
    pub key: String,
}

/// 列出所有 API Key（仅显示前缀）
pub async fn list_api_keys(
    State(state): State<Arc<ServerState>>,
) -> Json<ApiResponse<ListApiKeysResponse>> {
    let key_prefixes = state.auth_state.get_api_key_prefixes().await;

    let key_infos: Vec<ApiKeyInfo> = key_prefixes.iter().map(|prefix| {
        ApiKeyInfo {
            key_prefix: prefix.clone(),
        }
    }).collect();

    let total = key_infos.len();

    Json(ApiResponse::success(ListApiKeysResponse {
        keys: key_infos,
        total,
    }))
}

/// 轮换 API Key
pub async fn rotate_api_key(
    State(state): State<Arc<ServerState>>,
    Json(payload): Json<RotateApiKeyRequest>,
) -> Result<Json<ApiResponse<RotateApiKeyResponse>>, StatusCode> {
    use crate::security::mask_api_key;
    
    let old_key = &payload.old_key;
    let old_key_prefix = format!("{}...", &old_key[..8.min(old_key.len())]);
    
    // 验证旧 key 是否存在
    if !state.auth_state.contains_key(old_key).await {
        return Ok(Json(ApiResponse::error(
            ErrorCode::NotFound,
            "旧 API Key 不存在".to_string()
        )));
    }

    // 生成新 API Key
    let new_key = generate_random_key();
    let new_key_prefix = format!("{}...", &new_key[..8]);

    // 添加新 key
    state.auth_state.add_api_key(new_key.clone()).await;

    // 如果指定撤销旧 key，立即撤销
    if payload.revoke_old {
        state.auth_state.remove_api_key(old_key).await;
        info!("API Key rotated: {} -> {} (old key revoked)", 
              mask_api_key(old_key), mask_api_key(&new_key));
    } else {
        info!("API Key rotated: {} -> {} (old key still active)", 
              mask_api_key(old_key), mask_api_key(&new_key));
    }

    let response = RotateApiKeyResponse {
        new_key,
        new_key_prefix,
        old_key_prefix,
        old_key_revoked: payload.revoke_old,
        message: if payload.revoke_old {
            "API Key 轮换成功，旧 Key 已撤销。请妥善保管新 Key。".to_string()
        } else {
            "API Key 轮换成功，旧 Key 仍可使用。建议尽快更新所有使用旧 Key 的地方。".to_string()
        },
    };

    Ok(Json(ApiResponse::success(response)))
}

/// 生成随机 API Key
fn generate_random_key() -> String {
    use uuid::Uuid;
    format!("sk_{}", Uuid::new_v4().to_string().replace('-', ""))
}

/// 获取用户配额信息
pub async fn get_user_quota(
    State(state): State<Arc<ServerState>>,
    axum::extract::Path(user_id): axum::extract::Path<String>,
) -> Result<Json<ApiResponse<UserQuotaInfo>>, StatusCode> {
    let db = match &state.db {
        Some(d) => d,
        None => return Ok(Json(ApiResponse::error(
            ErrorCode::InternalError,
            "数据库未初始化".to_string()
        ))),
    };

    match db.get_or_create_user_quota(&user_id, state.quota_state.default_daily_limit).await {
        Ok(quota) => {
            let info = UserQuotaInfo {
                user_id: quota.user_id.clone(),
                daily_limit: quota.daily_limit,
                used_today: quota.used_today,
                remaining: quota.remaining(),
                is_exceeded: quota.is_exceeded(),
            };
            Ok(Json(ApiResponse::success(info)))
        }
        Err(e) => Ok(Json(ApiResponse::error(
            ErrorCode::InternalError,
            format!("获取配额失败：{}", e)
        ))),
    }
}

/// 更新用户配额
pub async fn update_user_quota(
    State(state): State<Arc<ServerState>>,
    axum::extract::Path(user_id): axum::extract::Path<String>,
    Json(payload): Json<UpdateUserQuotaRequest>,
) -> Result<Json<ApiResponse<()>>, StatusCode> {
    let db = match &state.db {
        Some(d) => d,
        None => return Ok(Json(ApiResponse::error(
            ErrorCode::InternalError,
            "数据库未初始化".to_string()
        ))),
    };

    match db.set_user_quota(&user_id, payload.daily_limit).await {
        Ok(_) => Ok(Json(ApiResponse::success(()))),
        Err(e) => Ok(Json(ApiResponse::error(
            ErrorCode::InternalError,
            format!("更新配额失败：{}", e)
        ))),
    }
}

/// 获取灰度配置
pub async fn get_gray_release_config(
    State(state): State<Arc<ServerState>>,
) -> Json<ApiResponse<GrayReleaseConfigInfo>> {
    let config = state.gray_release.read().await;
    Json(ApiResponse::success(GrayReleaseConfigInfo {
        enabled: config.enabled,
        whitelist: config.whitelist.iter().cloned().collect(),
        quota_per_user: config.quota_per_user,
    }))
}

/// 更新灰度配置（仅内存，重启后恢复）
pub async fn update_gray_release_config(
    State(state): State<Arc<ServerState>>,
    Json(payload): Json<UpdateGrayReleaseConfigRequest>,
) -> Json<ApiResponse<GrayReleaseConfigInfo>> {
    // 获取当前配置
    let current = state.gray_release.read().await;
    
    // 创建新配置
    let new_config = GrayReleaseConfig {
        enabled: payload.enabled.unwrap_or(current.enabled),
        whitelist: payload.whitelist.unwrap_or_else(|| current.whitelist.iter().cloned().collect()).into_iter().collect(),
        quota_per_user: payload.quota_per_user.unwrap_or(current.quota_per_user),
    };
    drop(current); // 释放读锁
    
    // 更新配置（通过写锁）
    {
        let mut config = state.gray_release.write().await;
        *config = new_config;
    }
    
    let config = state.gray_release.read().await;
    info!("Gray release config updated: enabled={}, whitelist_size={}, quota={}",
        config.enabled,
        config.whitelist.len(),
        config.quota_per_user
    );
    
    Json(ApiResponse::success(GrayReleaseConfigInfo {
        enabled: config.enabled,
        whitelist: config.whitelist.iter().cloned().collect(),
        quota_per_user: config.quota_per_user,
    }))
}

/// 获取系统统计信息
pub async fn get_system_stats(
    State(state): State<Arc<ServerState>>,
) -> Json<ApiResponse<SystemStats>> {
    let total_api_keys = state.auth_state.get_api_key_count().await;

    let total_users = if let Some(db) = &state.db {
        // 从数据库获取用户数量（通过列出 API Key 间接获取）
        match db.list_api_keys().await {
            Ok(keys) => keys.len(),
            Err(_) => 0,
        }
    } else {
        0
    };

    let gray_config = state.gray_release.read().await;
    Json(ApiResponse::success(SystemStats {
        total_users,
        total_api_keys,
        gray_release_enabled: gray_config.enabled,
        default_quota: state.quota_state.default_daily_limit,
    }))
}

/// 导出 Prometheus 指标
pub async fn export_metrics() -> axum::response::Response {
    use axum::response::IntoResponse;
    use axum::http::header;

    let metrics = metrics::encode_metrics();
    (
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        metrics
    ).into_response()
}

/// 获取当前用户配额（从认证的 API Key 自动获取用户 ID）
pub async fn get_current_user_quota(
    State(state): State<Arc<ServerState>>,
    request: axum::http::Request<axum::body::Body>,
) -> Result<Json<ApiResponse<UserQuotaInfo>>, StatusCode> {
    use crate::server::auth::get_auth_user;

    let auth_user = get_auth_user(&request)
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let db = match &state.db {
        Some(d) => d,
        None => {
            // 如果没有数据库，返回内存中的配额信息
            let quota = state.quota_state.get_memory_quota(&auth_user.user_id).await;
            let info = UserQuotaInfo {
                user_id: quota.user_id.clone(),
                daily_limit: quota.daily_limit,
                used_today: quota.used_today,
                remaining: quota.remaining(),
                is_exceeded: quota.is_exceeded(),
            };
            return Ok(Json(ApiResponse::success(info)));
        }
    };

    match db.get_or_create_user_quota(&auth_user.user_id, state.quota_state.default_daily_limit).await {
        Ok(quota) => {
            let info = UserQuotaInfo {
                user_id: quota.user_id.clone(),
                daily_limit: quota.daily_limit,
                used_today: quota.used_today,
                remaining: quota.remaining(),
                is_exceeded: quota.is_exceeded(),
            };
            Ok(Json(ApiResponse::success(info)))
        }
        Err(e) => Ok(Json(ApiResponse::error(
            ErrorCode::InternalError,
            format!("获取配额失败：{}", e)
        ))),
    }
}

/// 创建 API Key（简化版，供前端使用）
pub async fn create_api_key(
    State(state): State<Arc<ServerState>>,
    Json(_payload): Json<CreateApiKeySimpleRequest>,
) -> Result<Json<ApiResponse<GenerateApiKeyResponse>>, StatusCode> {
    let key = generate_random_key();
    let _key_prefix = crate::db::get_key_prefix(&key);
    let _key_hash = crate::db::hash_api_key_with_salt(&key);

    // 保存到内存和数据库
    state.auth_state.add_api_key(key.clone()).await;

    info!("New API Key created: {} (prefix: {})", key, _key_prefix);

    Ok(Json(ApiResponse::success(GenerateApiKeyResponse {
        key,
        key_prefix: _key_prefix,
        message: "API Key created successfully. Please save it immediately - it won't be shown again.".to_string(),
    })))
}

/// 简化版创建 API Key 请求
#[derive(Debug, Deserialize)]
pub struct CreateApiKeySimpleRequest {
    pub name: Option<String>,
    pub daily_limit: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_random_key() {
        let key1 = generate_random_key();
        let key2 = generate_random_key();

        assert!(key1.starts_with("sk_"));
        assert!(key2.starts_with("sk_"));
        assert_ne!(key1, key2);
        assert_eq!(key1.len(), 35); // sk_ + 32 chars (UUID without hyphens)
    }
}
