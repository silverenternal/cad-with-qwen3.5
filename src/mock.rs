//! Mock 实现模块 - 用于单元测试
//!
//! 本模块提供：
//! - Database trait 的 Mock 实现
//! - ApiClient 的 Mock 实现
//! - 其他外部依赖的 Mock

#[cfg(test)]
pub mod mocks {
    use crate::db::{Database, DbError, UserQuota, ApiKeyInfo, TelemetryStats};
    use crate::infrastructure::external::{ApiError, Message};
    use async_trait::async_trait;
    use chrono::Utc;
    use tokio::sync::RwLock;
    use std::collections::HashMap;

    /// Mock 数据库实现
    #[derive(Default)]
    pub struct MockDatabase {
        /// 用户配额数据
        pub quotas: RwLock<HashMap<String, UserQuota>>,
        /// API 使用日志
        pub logs: RwLock<Vec<(String, String, bool)>>,
        /// API Keys
        pub api_keys: RwLock<HashMap<String, bool>>, // key_hash -> active
        /// 是否应该失败
        pub should_fail: RwLock<bool>,
    }

    impl MockDatabase {
        pub fn new() -> Self {
            Self::default()
        }

        /// 设置数据库操作是否应该失败
        pub fn set_should_fail(&self, should_fail: bool) {
            *self.should_fail.try_write().unwrap() = should_fail;
        }

        /// 预置用户配额
        pub fn with_quota(self, user_id: &str, quota: UserQuota) -> Self {
            self.quotas.try_write().unwrap().insert(user_id.to_string(), quota);
            self
        }

        /// 预置 API Key
        pub fn with_api_key(self, key_hash: &str) -> Self {
            self.api_keys.try_write().unwrap().insert(key_hash.to_string(), true);
            self
        }
    }

    #[async_trait]
    impl Database for MockDatabase {
        fn db_type(&self) -> &'static str {
            "mock"
        }

        async fn is_healthy(&self) -> bool {
            !*self.should_fail.read().await
        }

        async fn log_event(
            &self,
            _event_type: &str,
            _user_id: Option<&str>,
            _session_id: Option<&str>,
            _endpoint: Option<&str>,
            _latency_ms: Option<i64>,
            _success: Option<bool>,
            _model_used: Option<&str>,
            _error_code: Option<&str>,
            _error_message: Option<&str>,
            _context: Option<&str>,
            _uptime_ms: u64,
        ) -> Result<(), DbError> {
            if *self.should_fail.read().await {
                return Err(DbError::Internal("Mock database is configured to fail".to_string()));
            }
            Ok(())
        }

        async fn get_stats(&self) -> Result<TelemetryStats, DbError> {
            if *self.should_fail.read().await {
                return Err(DbError::Internal("Mock database is configured to fail".to_string()));
            }

            let logs = self.logs.read().await;
            let total = logs.len() as u64;
            let successful = logs.iter().filter(|(_, _, success)| *success).count() as u64;
            
            Ok(TelemetryStats {
                total_requests: total,
                successful_requests: successful,
                failed_requests: total - successful,
                avg_latency_ms: 0.0,
            })
        }

        async fn get_user_daily_usage(
            &self,
            user_id: &str,
            _date: chrono::DateTime<Utc>,
        ) -> Result<u32, DbError> {
            if *self.should_fail.read().await {
                return Err(DbError::Internal("Mock database is configured to fail".to_string()));
            }

            let quotas = self.quotas.read().await;
            let quota = quotas.get(user_id)
                .map(|q| q.used_today)
                .unwrap_or(0);
            Ok(quota)
        }

        async fn get_or_create_user_quota(
            &self,
            user_id: &str,
            default_limit: u32,
        ) -> Result<UserQuota, DbError> {
            if *self.should_fail.read().await {
                return Err(DbError::Internal("Mock database is configured to fail".to_string()));
            }

            let mut quotas = self.quotas.write().await;
            let quota = quotas.entry(user_id.to_string())
                .or_insert_with(|| UserQuota {
                    user_id: user_id.to_string(),
                    daily_limit: default_limit,
                    used_today: 0,
                    last_reset_date: Utc::now().format("%Y-%m-%d").to_string(),
                })
                .clone();
            Ok(quota)
        }

        async fn get_user_quota(&self, user_id: &str) -> Result<UserQuota, DbError> {
            if *self.should_fail.read().await {
                return Err(DbError::Internal("Mock database is configured to fail".to_string()));
            }

            let quotas = self.quotas.read().await;
            quotas.get(user_id)
                .cloned()
                .ok_or_else(|| DbError::NotFound(format!("用户配额不存在：{}", user_id)))
        }

        async fn increment_user_usage(&self, user_id: &str) -> Result<(), DbError> {
            if *self.should_fail.read().await {
                return Err(DbError::Internal("Mock database is configured to fail".to_string()));
            }

            let mut quotas = self.quotas.write().await;
            if let Some(quota) = quotas.get_mut(user_id) {
                quota.used_today = quota.used_today.saturating_add(1);
            }
            Ok(())
        }

        async fn record_api_usage_and_increment(
            &self,
            user_id: &str,
            endpoint: &str,
            _model_used: Option<&str>,
            _latency_ms: i64,
        ) -> Result<(), DbError> {
            if *self.should_fail.read().await {
                return Err(DbError::Internal("Mock database is configured to fail".to_string()));
            }

            // 记录日志
            self.logs.write().await.push((
                user_id.to_string(),
                endpoint.to_string(),
                true,
            ));

            // 增加配额
            self.increment_user_usage(user_id).await
        }

        async fn log_api_usage(
            &self,
            user_id: &str,
            endpoint: &str,
            _model_used: Option<&str>,
            _latency_ms: i64,
            success: bool,
        ) -> Result<(), DbError> {
            if *self.should_fail.read().await {
                return Err(DbError::Internal("Mock database is configured to fail".to_string()));
            }

            self.logs.write().await.push((
                user_id.to_string(),
                endpoint.to_string(),
                success,
            ));
            Ok(())
        }

        async fn set_user_quota(&self, user_id: &str, daily_limit: u32) -> Result<(), DbError> {
            if *self.should_fail.read().await {
                return Err(DbError::Internal("Mock database is configured to fail".to_string()));
            }

            let mut quotas = self.quotas.write().await;
            if let Some(quota) = quotas.get_mut(user_id) {
                quota.daily_limit = daily_limit;
            }
            Ok(())
        }

        async fn save_api_key(
            &self,
            key_hash: &str,
            _key_prefix: &str,
            _description: Option<&str>,
            _expires_at: Option<chrono::DateTime<Utc>>,
        ) -> Result<(), DbError> {
            if *self.should_fail.read().await {
                return Err(DbError::Internal("Mock database is configured to fail".to_string()));
            }

            self.api_keys.write().await.insert(key_hash.to_string(), true);
            Ok(())
        }

        async fn verify_api_key(&self, key_hash: &str) -> Result<bool, DbError> {
            if *self.should_fail.read().await {
                return Err(DbError::Internal("Mock database is configured to fail".to_string()));
            }

            let api_keys = self.api_keys.read().await;
            Ok(api_keys.get(key_hash).copied().unwrap_or(false))
        }

        async fn revoke_api_key(&self, key_hash: &str) -> Result<bool, DbError> {
            if *self.should_fail.read().await {
                return Err(DbError::Internal("Mock database is configured to fail".to_string()));
            }

            let mut api_keys = self.api_keys.write().await;
            if let Some(active) = api_keys.get_mut(key_hash) {
                *active = false;
                Ok(true)
            } else {
                Ok(false)
            }
        }

        async fn list_api_keys(&self) -> Result<Vec<ApiKeyInfo>, DbError> {
            if *self.should_fail.read().await {
                return Err(DbError::Internal("Mock database is configured to fail".to_string()));
            }

            let api_keys = self.api_keys.read().await;
            Ok(api_keys.iter()
                .filter(|(_, active)| **active)
                .map(|(hash, _)| ApiKeyInfo {
                    key_prefix: format!("{}...", &hash[..8.min(hash.len())]),
                    created_at_ts: 0,
                    expires_at_ts: None,
                    is_active: true,
                    last_used_at_ts: None,
                    description: None,
                })
                .collect())
        }

        async fn execute_sql(
            &self,
            _query: &str,
            _count: i64,
            _user_id: &str,
        ) -> Result<u64, DbError> {
            if *self.should_fail.read().await {
                return Err(DbError::Internal("Mock database is configured to fail".to_string()));
            }
            // Mock 实现：返回影响 1 行
            Ok(1)
        }
    }

    /// Mock API 客户端
    pub struct MockApiClient {
        /// 预设的响应
        pub responses: RwLock<Vec<String>>,
        /// 是否应该失败
        pub should_fail: RwLock<bool>,
        /// 记录的消息历史
        pub message_history: RwLock<Vec<Vec<Message>>>,
    }

    impl MockApiClient {
        pub fn new() -> Self {
            Self {
                responses: RwLock::new(vec!["Mock response".to_string()]),
                should_fail: RwLock::new(false),
                message_history: RwLock::new(Vec::new()),
            }
        }

        /// 设置预设响应
        pub fn with_response(self, response: String) -> Self {
            self.responses.try_write().unwrap().push(response);
            self
        }

        /// 设置是否应该失败
        pub fn set_should_fail(&self, should_fail: bool) {
            *self.should_fail.try_write().unwrap() = should_fail;
        }

        /// 获取最后发送的消息
        pub async fn last_message(&self) -> Option<Vec<Message>> {
            self.message_history.read().await.last().cloned()
        }

        pub async fn chat(&self, messages: &[Message]) -> Result<String, ApiError> {
            if *self.should_fail.read().await {
                return Err(ApiError::timeout("Mock API client is configured to fail"));
            }

            // 记录消息
            self.message_history.write().await.push(messages.to_vec());

            // 返回预设响应
            let mut responses = self.responses.write().await;
            if responses.is_empty() {
                responses.push("Mock response".to_string());
            }
            Ok(responses.remove(0))
        }

        pub fn model(&self) -> &str {
            "mock-model"
        }

        pub fn client_name(&self) -> &str {
            "Mock API Client"
        }
    }

    impl Default for MockApiClient {
        fn default() -> Self {
            Self::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::mocks::*;
    use crate::db::Database;
    use crate::infrastructure::external::Message;

    #[tokio::test]
    async fn test_mock_database_basic() {
        let db = MockDatabase::new();

        // 测试健康检查
        assert!(db.is_healthy().await);

        // 测试配额获取
        let quota = db.get_or_create_user_quota("user1", 100).await.unwrap();
        assert_eq!(quota.daily_limit, 100);
        assert_eq!(quota.used_today, 0);

        // 测试配额增加
        db.increment_user_usage("user1").await.unwrap();
        let quota = db.get_or_create_user_quota("user1", 100).await.unwrap();
        assert_eq!(quota.used_today, 1);
    }

    #[tokio::test]
    async fn test_mock_database_failure() {
        let db = MockDatabase::new();
        db.set_should_fail(true);

        // 测试失败情况
        assert!(!db.is_healthy().await);
        assert!(db.get_or_create_user_quota("user1", 100).await.is_err());
    }

    #[tokio::test]
    async fn test_mock_api_client() {
        let client = MockApiClient::new();
        client.set_should_fail(false);

        let messages = vec![Message::user("test".to_string())];
        let response = client.chat(&messages).await.unwrap();
        assert_eq!(response, "Mock response".to_string());

        // 验证消息被记录
        let last_msg = client.last_message().await;
        assert!(last_msg.is_some());
    }

    #[tokio::test]
    async fn test_mock_api_client_failure() {
        let client = MockApiClient::new();
        client.set_should_fail(true);

        let messages = vec![Message::user("test".to_string())];
        assert!(client.chat(&messages).await.is_err());
    }
}
