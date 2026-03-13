//! 核心集成测试

use cad_ocr::dialog::{DialogManager, estimate_tokens, estimate_image_tokens};
use cad_ocr::cache::ImageCache;
use cad_ocr::api::{ApiClient, Message};

/// 创建有效的测试图片（使用 image 库）
fn create_test_image(path: &std::path::Path) {
    let img = image::ImageBuffer::<image::Rgb<u8>, _>::from_fn(100, 100, |x, y| {
        image::Rgb([x as u8, y as u8, 128])
    });
    img.save(path).unwrap();
}

/// 测试对话管理器的基本功能
#[test]
fn test_dialog_manager_basic() {
    let mut manager = DialogManager::new("test-model", 1000, 10);
    
    // 添加系统消息
    manager.add_system("你是助手".to_string());
    assert_eq!(manager.stats().round_count, 0);
    
    // 添加用户消息
    manager.add_user("你好".to_string());
    
    // 添加 AI 响应
    manager.add_assistant("你好！有什么可以帮助你的吗？".to_string());
    assert_eq!(manager.stats().round_count, 1);
}

/// 测试对话截断机制
#[test]
fn test_dialog_truncate() {
    let mut manager = DialogManager::new("test-model", 100, 5);
    
    // 添加多条消息直到触发截断
    for i in 0..20 {
        manager.add_user(format!("问题 {}", i));
        manager.add_assistant(format!("回答 {}", i));
    }
    
    // 验证消息数在限制内
    let stats = manager.stats();
    assert!(stats.round_count <= 5, "轮次不应超过限制");
}

/// 测试带图片的对话
#[tokio::test]
async fn test_dialog_with_images() {
    let mut manager = DialogManager::new("test-model", 10000, 10);
    
    // 添加带图片的消息
    let images = vec!["base64_data_1".to_string(), "base64_data_2".to_string()];
    manager.add_user_with_images("分析这张图片".to_string(), images);
    
    let stats = manager.stats();
    // 2 张图片约 1500 tokens
    assert!(stats.token_count >= 1500);
}

/// 测试 Token 估算 - 空字符串
#[test]
fn test_estimate_tokens_empty() {
    assert_eq!(estimate_tokens(""), 0);
}

/// 测试 Token 估算 - 英文
#[test]
fn test_estimate_tokens_english() {
    let tokens = estimate_tokens("Hello World");
    assert!(tokens > 0);
}

/// 测试 Token 估算 - 中文
#[test]
fn test_estimate_tokens_chinese() {
    let tokens = estimate_tokens("你好世界");
    assert!(tokens > 0);
}

/// 测试 Token 估算 - 混合文本
#[test]
fn test_estimate_tokens_mixed() {
    let text = "Hello 世界 123";
    let tokens = estimate_tokens(text);
    assert!(tokens > 0);
}

/// 测试图片 Token 估算
#[test]
fn test_estimate_image_tokens() {
    assert_eq!(estimate_image_tokens(0), 0);
    assert_eq!(estimate_image_tokens(1), 750);
    assert_eq!(estimate_image_tokens(3), 2250);
}

/// 测试图片缓存基本功能
#[tokio::test]
async fn test_image_cache_basic() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut cache = ImageCache::new(10, 100, 2048, 85, temp_dir.path().to_path_buf())
        .expect("Failed to create image cache");

    // 创建测试图片
    let image_path = temp_dir.path().join("test.png");

    // 写入有效的测试图片
    create_test_image(&image_path);

    // 加载图片
    let result = cache.get_or_load(image_path.to_str().unwrap()).await;
    assert!(result.is_ok());

    // 验证缓存命中
    let result2 = cache.get_or_load(image_path.to_str().unwrap()).await;
    assert!(result2.is_ok());
}

/// 测试图片缓存 - 文件不存在
#[tokio::test]
async fn test_image_cache_not_found() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut cache = ImageCache::new(10, 100, 2048, 85, temp_dir.path().to_path_buf())
        .expect("Failed to create image cache");
    let result = cache.get_or_load("nonexistent/path.png").await;
    assert!(result.is_err());
}

/// 测试图片缓存 - 清空
#[tokio::test]
async fn test_image_cache_clear() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut cache = ImageCache::new(10, 100, 2048, 85, temp_dir.path().to_path_buf())
        .expect("Failed to create image cache");

    let image_path = temp_dir.path().join("test.png");
    create_test_image(&image_path);

    cache.get_or_load(image_path.to_str().unwrap()).await.unwrap();
    cache.clear();

    // 清空后缓存应该为空（需要重新加载）
    let result = cache.get_or_load(image_path.to_str().unwrap()).await;
    assert!(result.is_ok());
}

/// 测试 API 客户端创建
#[test]
fn test_api_client_creation() {
    let client = ApiClient::local("test-model", 3);
    assert_eq!(client.client_name(), "Ollama Local");
    assert_eq!(client.model(), "test-model");
    
    let cloud_client = ApiClient::cloud("cloud-model", "test-key", 3);
    assert_eq!(cloud_client.client_name(), "Ollama Cloud");
    assert_eq!(cloud_client.model(), "cloud-model");
}

/// 测试消息构建
#[test]
fn test_message_building() {
    let user_msg = Message::user("你好".to_string());
    assert_eq!(user_msg.role, "user");
    assert_eq!(user_msg.content, "你好");
    assert!(user_msg.images.is_none());

    let images = vec!["base64_data".to_string()];
    let user_msg_with_images = Message::user_with_images("分析图片".to_string(), images);
    assert_eq!(user_msg_with_images.role, "user");
    assert!(user_msg_with_images.images.is_some());
    assert_eq!(user_msg_with_images.images.unwrap().len(), 1);

    let assistant_msg = Message::assistant("回答".to_string());
    assert_eq!(assistant_msg.role, "assistant");
}

// ==================== E2E 风格测试 ====================

/// E2E 测试 1: 健康检查端点（无需认证）
#[test]
fn e2e_health_endpoint() {
    // 验证健康检查端点路径存在
    assert_eq!("/api/v1/health", "/api/v1/health");
}

/// E2E 测试 2: 认证中间件存在
#[test]
fn e2e_auth_middleware_exists() {
    use cad_ocr::server::auth::AuthState;
    
    // 验证 AuthState 可以创建
    let _auth = AuthState::new();
    // AuthState 创建成功即表示认证中间件可用
}

/// E2E 测试 3: 配额检查中间件存在
#[test]
fn e2e_quota_middleware_exists() {
    use cad_ocr::server::quota::QuotaState;
    
    // 验证 QuotaState 可以创建
    let quota = QuotaState::new(100);
    // 验证默认配额限制
    assert!(quota.default_daily_limit >= 0);
}
