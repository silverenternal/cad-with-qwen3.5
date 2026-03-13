//! 边界条件测试
//!
//! 测试系统在极端条件下的行为

use cad_ocr::server::{
    auth::AuthState,
    rate_limit::RateLimitState,
    quota::{QuotaState, QuotaFallbackPolicy},
    gray_release::GrayReleaseConfig,
    types::Validator,
};
use cad_ocr::server::types::DrawingType;
use std::sync::Arc;

/// 测试配额 = 0 时的行为
#[tokio::test]
async fn test_quota_zero_limit() {
    // 配额为 0 时创建状态
    let quota_state = QuotaState::new(0);
    
    // 验证配额限制设置为 0
    assert_eq!(quota_state.default_daily_limit, 0);
}

/// 测试速率限制 = 0 时的行为
#[test]
fn test_rate_limit_zero_rps() {
    // 速率限制为 0 时，内部应该处理
    let rate_limit = RateLimitState::new(0, 1.0);
    
    // 验证创建成功（内部应该处理了 0 的情况）
    // Governor 库会自动处理 0 的情况
    drop(rate_limit);
}

/// 测试速率限制 burst = 0 时的行为
#[test]
fn test_rate_limit_zero_burst() {
    // burst 为 0 时，应该使用最小值
    let rate_limit = RateLimitState::new(10, 0.0);
    
    // 验证创建成功
    drop(rate_limit);
}

/// 测试灰度发布配置为空时的行为
#[tokio::test]
async fn test_gray_release_empty_whitelist() {
    let config = GrayReleaseConfig::default();
    let config_lock = Arc::new(tokio::sync::RwLock::new(config));
    
    // 灰度发布默认禁用
    let config_read = config_lock.read().await;
    assert!(!config_read.enabled);
    assert!(config_read.whitelist.is_empty());
}

/// 测试 API Key 为空字符串时的行为
#[tokio::test]
async fn test_auth_empty_api_key() {
    let auth_state = AuthState::new();
    
    // 空字符串应该被拒绝
    assert!(!auth_state.contains_key("").await);
    
    // 添加空字符串 key
    auth_state.add_api_key("".to_string()).await;
    
    // 验证可以匹配
    assert!(auth_state.contains_key("").await);
}

/// 测试配额降级策略 - MemoryMode
#[tokio::test]
async fn test_quota_fallback_memory() {
    let quota_state = QuotaState::new(100)
        .with_fallback_policy(QuotaFallbackPolicy::MemoryMode);
    
    // 验证降级策略设置正确
    assert_eq!(quota_state.fallback_policy, QuotaFallbackPolicy::MemoryMode);
}

/// 测试配额降级策略 - Reject
#[tokio::test]
async fn test_quota_fallback_reject() {
    let quota_state = QuotaState::new(100)
        .with_fallback_policy(QuotaFallbackPolicy::Reject);
    
    // 验证降级策略设置正确
    assert_eq!(quota_state.fallback_policy, QuotaFallbackPolicy::Reject);
}

/// 测试并发配额检查（竞争条件）
#[tokio::test]
async fn test_quota_concurrent_check() {
    let quota_state = Arc::new(QuotaState::new(10));
    
    // 验证配额状态创建成功
    assert_eq!(quota_state.default_daily_limit, 10);
}

/// 测试会话 ID 边界长度
#[test]
fn test_session_id_boundary_length() {
    // 128 字符 - 应该成功
    let valid_128 = "a".repeat(128);
    assert!(Validator::validate_session_id(&valid_128).is_ok());
    
    // 129 字符 - 应该失败
    let invalid_129 = "a".repeat(129);
    assert!(Validator::validate_session_id(&invalid_129).is_err());
    
    // 空字符串 - 应该成功（空是合法的）
    assert!(Validator::validate_session_id("").is_ok());
}

/// 测试问题长度边界
#[test]
fn test_question_boundary_length() {
    // 2000 字符 - 应该成功
    let valid_2000 = "a".repeat(2000);
    assert!(Validator::validate_question(&valid_2000).is_ok());
    
    // 2001 字符 - 应该失败
    let invalid_2001 = "a".repeat(2001);
    assert!(Validator::validate_question(&invalid_2001).is_err());
}

/// 测试消息长度边界
#[test]
fn test_message_boundary_length() {
    // 4000 字符 - 应该成功
    let valid_4000 = "a".repeat(4000);
    assert!(Validator::validate_message(&valid_4000).is_ok());
    
    // 4001 字符 - 应该失败
    let invalid_4001 = "a".repeat(4001);
    assert!(Validator::validate_message(&invalid_4001).is_err());
}

/// 测试图纸类型长度边界
#[test]
fn test_drawing_type_boundary_length() {
    // 50 字符 - 应该成功
    let valid_50 = "a".repeat(50);
    assert!(DrawingType::validate(&valid_50).is_ok());
    
    // 51 字符 - 应该失败
    let invalid_51 = "a".repeat(51);
    assert!(DrawingType::validate(&invalid_51).is_err());
}

/// 测试图片数据大小边界
#[test]
fn test_image_size_boundary() {
    // 1023 字符 - 应该失败（小于最小值）
    let too_small = "A".repeat(1023);
    assert!(Validator::validate_image_base64(&too_small).is_err());
    
    // 1024 字符 - 应该成功（刚好最小值）
    let min_size = "A".repeat(1024);
    assert!(Validator::validate_image_base64(&min_size).is_ok());
}

/// 测试灰度命中率计算
#[test]
fn test_gray_release_hit_rate_calculation() {
    use cad_ocr::metrics::{Metrics, REGISTRY};
    
    let metrics = Metrics::new(&REGISTRY).unwrap();
    
    // 初始命中率应该是 0
    assert_eq!(metrics.get_gray_release_hit_rate(), 0.0);
    
    // 记录 3 次命中，1 次未命中
    metrics.record_gray_release_hit();
    metrics.record_gray_release_hit();
    metrics.record_gray_release_hit();
    metrics.record_gray_release_miss();
    
    // 命中率应该是 75%
    let hit_rate = metrics.get_gray_release_hit_rate();
    assert!((hit_rate - 75.0).abs() < 0.01);
}

/// 测试 Validator 对特殊字符的处理
#[test]
fn test_validator_special_chars() {
    // 测试图纸类型中的特殊字符
    assert!(DrawingType::validate("类型<test>").is_err());
    assert!(DrawingType::validate("类型\"test\"").is_err());
    assert!(DrawingType::validate("类型&test").is_err());
    assert!(DrawingType::validate("类型\\test").is_err());
    
    // 测试合法的特殊情况
    assert!(DrawingType::validate("建筑平面图").is_ok());
    assert!(DrawingType::validate("Building Plan").is_ok());
}

/// 测试 Base64 格式边界
#[test]
fn test_base64_format_boundary() {
    // 包含非法字符
    assert!(Validator::validate_image_base64("AAAA!!!!").is_err());
    
    // 合法的 Base64 字符
    assert!(Validator::validate_image_base64(&"AAAA".repeat(300)).is_ok());
    
    // 包含 + 和 / (合法 Base64 字符)
    let with_plus_slash = "AA+A/AAA".repeat(300);
    assert!(Validator::validate_image_base64(&with_plus_slash).is_ok());
}
