//! User quota check middleware

use axum::{
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
    extract::State,
    Json,
    body::Body,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;
use chrono::{Utc, NaiveDate};
use tracing::{warn, info, error};
use crate::{
    server::types::ApiResponse,
    metrics,
    db::{Database, UserQuota},
};

/// 内存中的用户配额记录
#[derive(Debug, Clone)]
pub struct MemoryQuota {
    pub user_id: String,
    pub daily_limit: u32,
    pub used_today: u32,
    pub last_reset_date: NaiveDate,
}

impl MemoryQuota {
    pub fn new(user_id: String, daily_limit: u32) -> Self {
        Self {
            user_id,
            daily_limit,
            used_today: 0,
            last_reset_date: Utc::now().naive_utc().date(),
        }
    }

    pub fn is_exceeded(&self) -> bool {
        // Check if date needs reset
        let today = Utc::now().naive_utc().date();
        if self.last_reset_date < today {
            return false; // New day, quota reset
        }
        self.used_today >= self.daily_limit
    }

    pub fn remaining(&self) -> u32 {
        let today = Utc::now().naive_utc().date();
        if self.last_reset_date < today {
            return self.daily_limit;
        }
        self.daily_limit.saturating_sub(self.used_today)
    }
}

/// Quota fallback policy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaFallbackPolicy {
    /// Reject service when database fails (safety first)
    Reject,
    /// Use memory mode when database fails (availability first, but logs warning)
    MemoryMode,
}

/// Quota state
pub struct QuotaState {
    /// Default daily quota
    pub default_daily_limit: u32,
    /// Database connection (optional)
    pub db: Option<Arc<dyn Database>>,
    /// Memory quota storage (when database is unavailable)
    pub memory_quotas: Arc<RwLock<HashMap<String, MemoryQuota>>>,
    /// Fallback policy
    pub fallback_policy: QuotaFallbackPolicy,
    /// Whether database has failed (for health check)
    pub db_failed: Arc<RwLock<bool>>,
}

impl QuotaState {
    pub fn new(default_daily_limit: u32) -> Self {
        Self {
            default_daily_limit,
            db: None,
            memory_quotas: Arc::new(RwLock::new(HashMap::new())),
            fallback_policy: QuotaFallbackPolicy::MemoryMode,
            db_failed: Arc::new(RwLock::new(false)),
        }
    }

    pub fn with_db(mut self, db: Arc<dyn Database>) -> Self {
        self.db = Some(db);
        self
    }

    /// Set fallback policy
    pub fn with_fallback_policy(mut self, policy: QuotaFallbackPolicy) -> Self {
        self.fallback_policy = policy;
        self
    }

    /// Mark database as failed
    pub async fn mark_db_failed(&self, failed: bool) {
        let mut db_failed = self.db_failed.write().await;
        *db_failed = failed;
    }

    /// Check if database has failed
    pub async fn is_db_failed(&self) -> bool {
        *self.db_failed.read().await
    }

    /// Get memory quota for a user (for admin endpoints)
    pub async fn get_memory_quota(&self, user_id: &str) -> MemoryQuota {
        let mut quotas = self.memory_quotas.write().await;
        quotas
            .entry(user_id.to_string())
            .or_insert_with(|| MemoryQuota::new(user_id.to_string(), self.default_daily_limit))
            .clone()
    }
}

use crate::server::user_id::extract_user_id;

/// Quota check middleware
pub async fn quota_middleware(
    State(quota_state): State<Arc<QuotaState>>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, (StatusCode, Json<ApiResponse<()>>)> {
    // Extract user ID
    let user_id = match extract_user_id(&request) {
        Some(id) => id,
        None => {
            // No user ID, skip quota check
            return Ok(next.run(request).await);
        }
    };

    info!("Checking user quota: user_id={}", user_id);

    let mut quota_exceeded = false;
    let mut quota_info = String::new();
    let mut using_fallback = false;

    if let Some(ref db) = quota_state.db {
        // 原子性检查并扣减配额（使用 SQL UPDATE 避免竞态条件）
        // SQL: UPDATE ... SET used_today = used_today + 1 WHERE user_id = ? AND used_today < daily_limit
        let rows_affected = db
            .execute_sql(
                r#"
                UPDATE user_quotas
                SET used_today = used_today + 1, updated_at = CURRENT_TIMESTAMP
                WHERE user_id = ?1
                AND used_today < daily_limit
                "#,
                1,  // count
                &user_id,
            )
            .await;

        match rows_affected {
            Ok(affected) if affected > 0 => {
                // 配额扣减成功，记录 API 使用日志
                info!("User quota check passed (atomic SQL): user_id={}", user_id);
                if let Err(e) = db.log_api_usage(
                    &user_id,
                    "api_request",
                    None,
                    0,
                    true,
                ).await {
                    warn!("Failed to log API usage: {}", e);
                }
            }
            Ok(_) => {
                // 影响行数为 0，说明配额已用尽
                warn!("User quota exceeded: user_id={}", user_id);
                quota_exceeded = true;
                // 获取配额信息用于错误消息
                if let Ok(quota) = db.get_or_create_user_quota(&user_id, quota_state.default_daily_limit).await {
                    quota_info = format!("{} times/day", quota.daily_limit);
                }
            }
            Err(e) => {
                error!("Failed to atomically check and consume quota: {}, falling back to memory mode", e);
                // Mark database as failed
                quota_state.mark_db_failed(true).await;
                using_fallback = true;
                // Continue to fallback logic
            }
        }
    } else {
        // Database not configured
        using_fallback = true;
    }

    // Fallback logic: use memory mode when database is unavailable
    if using_fallback {
        match quota_state.fallback_policy {
            QuotaFallbackPolicy::Reject => {
                // Safety first: reject service when database fails
                error!("Database unavailable and configured to reject mode, user {} rejected", user_id);
                return Err((
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(ApiResponse::error(
                        crate::server::types::ErrorCode::ServiceUnavailable,
                        "Quota system unavailable, please try again later".to_string()
                    )),
                ));
            }
            QuotaFallbackPolicy::MemoryMode => {
                // Availability first: use memory mode, but log warning
                warn!("Quota system using memory fallback mode (data will be lost on restart): user_id={}", user_id);
                
                let mut quotas = quota_state.memory_quotas.write().await;

                let quota = quotas.entry(user_id.clone())
                    .or_insert_with(|| MemoryQuota::new(user_id.clone(), quota_state.default_daily_limit));

                // Check and reset daily quota
                let today = Utc::now().naive_utc().date();
                if quota.last_reset_date < today {
                    quota.used_today = 0;
                    quota.last_reset_date = today;
                }

                if quota.is_exceeded() {
                    warn!("User quota exceeded (memory mode): user_id={}, used={}/{}",
                          user_id, quota.used_today, quota.daily_limit);
                    quota_exceeded = true;
                    quota_info = format!("{} times/day", quota.daily_limit);
                } else {
                    // Increment usage count
                    quota.used_today += 1;
                    info!("User quota check passed (memory mode): user_id={}, remaining={}/{}",
                          user_id, quota.remaining(), quota.daily_limit);
                }
            }
        }
    }

    if quota_exceeded {
        // Record Prometheus metrics
        metrics::GLOBAL_METRICS.record_quota_exceeded();

        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(ApiResponse::quota_exceeded(format!(
                "Daily quota exhausted ({} times/day), please try again tomorrow or upgrade account",
                quota_info.replace(" times/day", "")
            ))),
        ));
    }

    Ok(next.run(request).await)
}

/// Helper function to check user quota
pub async fn check_user_quota(
    user_id: &str,
    default_limit: u32,
    db: Option<&Arc<dyn Database>>,
) -> Result<UserQuota, (StatusCode, Json<ApiResponse<()>>)> {
    if let Some(database) = db {
        match database.get_or_create_user_quota(user_id, default_limit).await {
            Ok(quota) => Ok(quota),
            Err(e) => {
                error!("Failed to get user quota from database: {}", e);
                Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::error(
                        crate::server::types::ErrorCode::InternalError,
                        format!("Failed to get quota: {}", e)
                    )),
                ))
            }
        }
    } else {
        // 简化实现：返回空配额
        Ok(UserQuota {
            user_id: user_id.to_string(),
            daily_limit: default_limit,
            used_today: 0,
            last_reset_date: chrono::Utc::now().format("%Y-%m-%d").to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::UserQuota;

    #[test]
    fn test_quota_state() {
        let state = QuotaState::new(100);
        assert_eq!(state.default_daily_limit, 100);
    }

    #[test]
    fn test_quota_check() {
        let quota = UserQuota {
            user_id: "test_user".to_string(),
            daily_limit: 100,
            used_today: 50,
            last_reset_date: chrono::Utc::now().format("%Y-%m-%d").to_string(),
        };

        assert!(!quota.is_exceeded());
        assert_eq!(quota.remaining(), 50);
    }

    #[test]
    fn test_quota_exceeded() {
        let quota = UserQuota {
            user_id: "test_user".to_string(),
            daily_limit: 100,
            used_today: 100,
            last_reset_date: chrono::Utc::now().format("%Y-%m-%d").to_string(),
        };

        assert!(quota.is_exceeded());
        assert_eq!(quota.remaining(), 0);
    }
}
