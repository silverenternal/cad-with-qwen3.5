//! 测试数据工厂
//!
//! 提供统一的测试数据生成工具，用于集成测试和压力测试

use cad_ocr::server::{
    auth::AuthState,
    rate_limit::RateLimitState,
    quota::{QuotaState, QuotaFallbackPolicy},
    gray_release::GrayReleaseConfig,
};
use std::sync::Arc;

/// 测试数据工厂
/// 
/// 用于生成各种测试场景所需的数据和状态
pub struct TestFactory;

impl TestFactory {
    /// 生成有效的 API Key
    /// 
    /// # Returns
    /// 返回一个格式正确的 API Key（32 字符，字母数字）
    pub fn generate_valid_api_key() -> String {
        use rand::{Rng, distributions::Alphanumeric};
        rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(32)
            .map(char::from)
            .collect()
    }

    /// 生成无效的 API Key
    /// 
    /// # Returns
    /// 返回一个格式正确但不存在的 API Key（用于测试认证失败）
    pub fn generate_invalid_api_key() -> String {
        format!("invalid_{}", Self::generate_valid_api_key())
    }

    /// 生成 Base64 图片数据
    /// 
    /// # Arguments
    /// * `size_kb` - 图片大小（KB），默认至少 1KB
    /// 
    /// # Returns
    /// 返回一个合法的 Base64 字符串（约指定大小）
    pub fn generate_base64_image(size_kb: usize) -> String {
        // Base64 字符集：A-Za-z0-9+/，长度必须是 4 的倍数
        let target_chars = size_kb * 1024;
        let repeat_count = (target_chars / 4).max(256); // 至少 256 个"AAAA"
        "AAAA".repeat(repeat_count)
    }

    /// 生成测试用的图纸类型
    /// 
    /// # Returns
    /// 返回一个预定义的图纸类型名称
    pub fn generate_drawing_type() -> &'static str {
        const TYPES: &[&str] = &[
            "建筑平面图",
            "结构平面图",
            "结构配筋图",
            "市政道路断面图",
            "基坑支护图",
        ];
        use rand::Rng;
        let idx = rand::thread_rng().gen_range(0..TYPES.len());
        TYPES[idx]
    }

    /// 生成测试用的问题
    /// 
    /// # Arguments
    /// * `length` - 问题长度（字符数），None 则随机
    /// 
    /// # Returns
    /// 返回一个测试问题字符串
    pub fn generate_question(length: Option<usize>) -> String {
        match length {
            Some(len) => "请分析这张图纸".chars().cycle().take(len).collect(),
            None => "请分析这张图纸，指出主要结构和潜在问题".to_string(),
        }
    }

    /// 生成测试用的会话 ID
    /// 
    /// # Returns
    /// 返回一个格式正确的会话 ID
    pub fn generate_session_id() -> String {
        format!("test_session_{}", Self::generate_valid_api_key()[..16].to_string())
    }

    /// 创建测试用的 AuthState
    /// 
    /// # Arguments
    /// * `with_api_keys` - 是否添加有效的 API Key
    /// 
    /// # Returns
    /// 返回一个配置好的 AuthState
    pub async fn create_auth_state(with_api_keys: bool) -> Arc<AuthState> {
        let auth_state = AuthState::new();
        if with_api_keys {
            // 添加一个测试 API Key
            let test_key = Self::generate_valid_api_key();
            auth_state.add_api_key(test_key).await;
        }
        Arc::new(auth_state)
    }

    /// 创建测试用的 RateLimitState
    /// 
    /// # Arguments
    /// * `rps` - 每秒请求数限制，默认 100
    /// * `burst` - 突发倍数，默认 1.5
    /// 
    /// # Returns
    /// 返回一个配置好的 RateLimitState
    pub fn create_rate_limit_state(rps: Option<u32>, burst: Option<f64>) -> Arc<RateLimitState> {
        let rps = rps.unwrap_or(100);
        let burst = burst.unwrap_or(1.5);
        Arc::new(RateLimitState::new(rps, burst))
    }

    /// 创建测试用的 QuotaState
    /// 
    /// # Arguments
    /// * `daily_limit` - 每日配额限制，默认 1000
    /// * `fallback` - 降级策略，默认 MemoryMode
    /// 
    /// # Returns
    /// 返回一个配置好的 QuotaState
    pub fn create_quota_state(
        daily_limit: Option<u32>,
        fallback: Option<QuotaFallbackPolicy>,
    ) -> Arc<QuotaState> {
        let daily_limit = daily_limit.unwrap_or(1000);
        let fallback = fallback.unwrap_or(QuotaFallbackPolicy::MemoryMode);
        Arc::new(QuotaState::new(daily_limit).with_fallback_policy(fallback))
    }

    /// 创建测试用的 GrayReleaseConfig
    /// 
    /// # Arguments
    /// * `enabled` - 是否启用灰度发布
    /// * `whitelist` - 灰度用户 ID 列表
    /// 
    /// # Returns
    /// 返回一个配置好的 GrayReleaseConfig
    pub fn create_gray_release_config(
        enabled: bool,
        whitelist: Option<Vec<String>>,
    ) -> Arc<tokio::sync::RwLock<GrayReleaseConfig>> {
        let mut config = GrayReleaseConfig::default();
        config.enabled = enabled;
        if let Some(ids) = whitelist {
            config.whitelist = ids.into_iter().collect();
        }
        Arc::new(tokio::sync::RwLock::new(config))
    }

    /// 生成测试用户 ID
    /// 
    /// # Returns
    /// 返回一个测试用户 ID
    pub fn generate_user_id() -> String {
        format!("test_user_{}", uuid::Uuid::new_v4())
    }

    /// 生成测试用的错误消息
    /// 
    /// # Arguments
    /// * `error_type` - 错误类型
    /// 
    /// # Returns
    /// 返回一个格式化的错误消息
    pub fn generate_error_message(error_type: &str) -> String {
        format!("[TEST_ERROR] {}: Test error message for {}", error_type, chrono::Utc::now().to_rfc3339())
    }
}

/// 测试场景构建器
/// 
/// 用于快速构建常见的测试场景配置
pub struct TestScenarioBuilder {
    auth_state: Option<Arc<AuthState>>,
    rate_limit: Option<Arc<RateLimitState>>,
    quota_state: Option<Arc<QuotaState>>,
    gray_release: Option<Arc<tokio::sync::RwLock<GrayReleaseConfig>>>,
    with_auth: bool,
    with_rate_limit: bool,
    with_quota: bool,
    with_gray_release: bool,
}

impl TestScenarioBuilder {
    pub fn new() -> Self {
        Self {
            auth_state: None,
            rate_limit: None,
            quota_state: None,
            gray_release: None,
            with_auth: false,
            with_rate_limit: false,
            with_quota: false,
            with_gray_release: false,
        }
    }

    /// 启用认证
    pub fn with_auth(mut self) -> Self {
        self.with_auth = true;
        self
    }

    /// 启用限流
    pub fn with_rate_limit(mut self) -> Self {
        self.with_rate_limit = true;
        self
    }

    /// 启用配额限制
    pub fn with_quota(mut self) -> Self {
        self.with_quota = true;
        self
    }

    /// 启用灰度发布
    pub fn with_gray_release(mut self) -> Self {
        self.with_gray_release = true;
        self
    }

    /// 构建测试场景
    /// 
    /// # Returns
    /// 返回一个包含所有配置好的状态的元组
    pub async fn build(self) -> (
        Option<Arc<AuthState>>,
        Option<Arc<RateLimitState>>,
        Option<Arc<QuotaState>>,
        Option<Arc<tokio::sync::RwLock<GrayReleaseConfig>>>,
    ) {
        let auth_state = if self.with_auth {
            Some(TestFactory::create_auth_state(true).await)
        } else {
            None
        };

        let rate_limit = if self.with_rate_limit {
            Some(TestFactory::create_rate_limit_state(None, None))
        } else {
            None
        };

        let quota_state = if self.with_quota {
            Some(TestFactory::create_quota_state(None, None))
        } else {
            None
        };

        let gray_release = if self.with_gray_release {
            Some(TestFactory::create_gray_release_config(true, None))
        } else {
            None
        };

        (auth_state, rate_limit, quota_state, gray_release)
    }
}

impl Default for TestScenarioBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_generate_valid_api_key() {
        let key = TestFactory::generate_valid_api_key();
        assert_eq!(key.len(), 32);
        assert!(key.chars().all(|c| c.is_alphanumeric()));
    }

    #[tokio::test]
    async fn test_generate_invalid_api_key() {
        let key = TestFactory::generate_invalid_api_key();
        assert!(key.starts_with("invalid_"));
        assert!(key.len() > 8);
    }

    #[test]
    fn test_generate_base64_image() {
        let base64 = TestFactory::generate_base64_image(1); // 1KB
        assert!(base64.len() >= 1024);
        assert!(base64.chars().all(|c| c.is_alphanumeric() || c == '+' || c == '/' || c == '='));
    }

    #[test]
    fn test_generate_drawing_type() {
        let drawing_type = TestFactory::generate_drawing_type();
        assert!(!drawing_type.is_empty());
        assert!(drawing_type.len() <= 50);
    }

    #[test]
    fn test_generate_question() {
        let question = TestFactory::generate_question(None);
        assert!(!question.is_empty());
        
        let long_question = TestFactory::generate_question(Some(100));
        assert_eq!(long_question.chars().count(), 100);
    }

    #[test]
    fn test_generate_session_id() {
        let session_id = TestFactory::generate_session_id();
        assert!(session_id.starts_with("test_session_"));
        assert!(session_id.len() <= 128);
    }

    #[tokio::test]
    async fn test_create_auth_state() {
        let auth_state = TestFactory::create_auth_state(true).await;
        // Just verify the auth state was created (can't access private fields)
        // The add_api_key method should have been called successfully
        drop(auth_state);
    }

    #[test]
    fn test_create_rate_limit_state() {
        let rate_limit = TestFactory::create_rate_limit_state(Some(50), Some(2.0));
        // Just verify it was created successfully
        drop(rate_limit);
    }

    #[test]
    fn test_create_quota_state() {
        let quota_state = TestFactory::create_quota_state(Some(500), None);
        assert_eq!(quota_state.default_daily_limit, 500);
        assert_eq!(quota_state.fallback_policy, QuotaFallbackPolicy::MemoryMode);
    }

    #[tokio::test]
    async fn test_create_gray_release_config() {
        let gray_release = TestFactory::create_gray_release_config(true, Some(vec!["user1".to_string()]));
        let config = gray_release.read().await;
        assert!(config.enabled);
        assert!(config.whitelist.contains(&"user1".to_string()));
    }

    #[test]
    fn test_generate_user_id() {
        let user_id1 = TestFactory::generate_user_id();
        let user_id2 = TestFactory::generate_user_id();
        assert!(user_id1.starts_with("test_user_"));
        assert_ne!(user_id1, user_id2); // UUID 应该是唯一的
    }

    #[test]
    fn test_generate_error_message() {
        let error_msg = TestFactory::generate_error_message("TEST");
        assert!(error_msg.contains("[TEST_ERROR]"));
        assert!(error_msg.contains("TEST"));
    }

    #[tokio::test]
    async fn test_scenario_builder_minimal() {
        let (auth, rate_limit, quota, gray) = TestScenarioBuilder::new().build().await;
        assert!(auth.is_none());
        assert!(rate_limit.is_none());
        assert!(quota.is_none());
        assert!(gray.is_none());
    }

    #[tokio::test]
    async fn test_scenario_builder_full() {
        let (auth, rate_limit, quota, gray) = TestScenarioBuilder::new()
            .with_auth()
            .with_rate_limit()
            .with_quota()
            .with_gray_release()
            .build()
            .await;
        assert!(auth.is_some());
        assert!(rate_limit.is_some());
        assert!(quota.is_some());
        assert!(gray.is_some());
    }
}
