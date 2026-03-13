//! Gray release control module

use axum::{
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
    extract::State,
    Json,
    body::Body,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use tracing::{warn, info};
use crate::{
    db::Database,
    server::types::ApiResponse,
};

/// Gray release config (内部可变性，使用 RwLock 包裹)
#[derive(Debug, Serialize, Deserialize)]
pub struct GrayReleaseConfig {
    /// Whether gray release is enabled
    pub enabled: bool,
    /// Whitelist user ID list
    pub whitelist: HashSet<String>,
    /// Per user daily quota
    pub quota_per_user: u32,
}

impl Clone for GrayReleaseConfig {
    fn clone(&self) -> Self {
        Self {
            enabled: self.enabled,
            whitelist: self.whitelist.clone(),
            quota_per_user: self.quota_per_user,
        }
    }
}

impl Default for GrayReleaseConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            whitelist: HashSet::new(),
            // Default quota, can be overridden by config file
            quota_per_user: 100,
        }
    }
}

use crate::server::user_id::extract_user_id;

/// Gray access check middleware
pub async fn check_gray_access(
    State(config): State<Arc<tokio::sync::RwLock<GrayReleaseConfig>>>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, (StatusCode, Json<ApiResponse<()>>)> {
    // If gray release is not enabled, allow all
    let config = config.read().await;
    if !config.enabled {
        drop(config);
        return Ok(next.run(request).await);
    }

    // Extract user ID
    let user_id = match extract_user_id(&request) {
        Some(id) => id,
        None => {
            // No user ID, deny access
            warn!("Gray access denied: missing user identifier");
            return Err((
                StatusCode::FORBIDDEN,
                Json(ApiResponse::error(
                    crate::server::types::ErrorCode::Forbidden,
                    "User identifier required during gray release".to_string()
                )),
            ));
        }
    };

    // Check if in whitelist
    if config.whitelist.contains(&user_id) {
        info!("Gray user {} in whitelist, access allowed", user_id);
        drop(config);
        return Ok(next.run(request).await);
    }

    // Not in whitelist, deny access
    warn!("Gray user {} not in whitelist, access denied", user_id);
    drop(config);
    Err((
        StatusCode::FORBIDDEN,
        Json(ApiResponse::error(
            crate::server::types::ErrorCode::Forbidden,
            format!("User {} not in gray release whitelist", user_id)
        )),
    ))
}

/// Check if user is in gray release whitelist
pub fn is_in_gray_whitelist(user_id: &str, config: &GrayReleaseConfig) -> bool {
    if !config.enabled {
        return true; // Gray release not enabled, allow all users
    }
    config.whitelist.contains(user_id)
}

/// Check user quota
pub async fn check_user_quota(user_id: &str, daily_limit: u32, db: Option<&Arc<dyn Database>>) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    // 从数据库查询用户今日已用配额
    if let Some(database) = db {
        let today = Utc::now();
        match database.get_user_daily_usage(user_id, today).await {
            Ok(used) => {
                if used >= daily_limit {
                    return Ok(false); // 配额已用尽
                }
                return Ok(true); // 配额充足
            }
            Err(e) => {
                warn!("Failed to get user daily usage from database: {}", e);
                // 数据库查询失败，降级到内存模式或允许请求
            }
        }
    }
    
    // 没有数据库或查询失败，简化处理：允许请求
    // 实际生产中应该根据 fallback policy 决定
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gray_release_config_default() {
        let config = GrayReleaseConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.quota_per_user, 100);
        assert!(config.whitelist.is_empty());
    }

    #[test]
    fn test_is_in_gray_whitelist() {
        let mut config = GrayReleaseConfig::default();
        config.enabled = true;
        config.whitelist.insert("user_test123".to_string());

        assert!(is_in_gray_whitelist("user_test123", &config));
        assert!(!is_in_gray_whitelist("user_unknown", &config));
    }

    #[test]
    fn test_is_in_gray_whitelist_disabled() {
        let config = GrayReleaseConfig::default(); // enabled = false
        assert!(is_in_gray_whitelist("any_user", &config));
    }
}
