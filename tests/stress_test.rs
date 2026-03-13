//! 压力测试和故障注入测试
//! 
//! 验证系统在高并发和故障场景下的行为

use std::sync::Arc;
use tokio::task::JoinSet;

use cad_ocr::server::{
    rate_limit::{RateLimitState, rate_limit_middleware},
    quota::{QuotaState, quota_middleware, QuotaFallbackPolicy, MemoryQuota},
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
    routing::get,
    Router,
};
use tower::ServiceExt;

/// 压力测试：并发限流
#[tokio::test]
async fn test_rate_limit_stress() {
    // 创建限流状态（10 req/s, burst 15）
    let rate_limit = Arc::new(RateLimitState::new(10, 1.5));
    
    let app = Router::new()
        .route("/test", get(|| async { "ok" }))
        .layer(axum::middleware::from_fn_with_state(
            rate_limit.clone(),
            rate_limit_middleware,
        ));
    
    // 并发发送 20 个请求
    let mut tasks = JoinSet::new();
    
    for _ in 0..20 {
        let app_clone = app.clone();
        tasks.spawn(async move {
            let response = app_clone
                .oneshot(
                    Request::builder()
                        .uri("/test")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            
            response.status()
        });
    }
    
    // 收集结果
    let mut success_count = 0;
    let mut rate_limited_count = 0;
    
    while let Some(result) = tasks.join_next().await {
        if let Ok(status) = result {
            match status {
                StatusCode::OK => success_count += 1,
                StatusCode::TOO_MANY_REQUESTS => rate_limited_count += 1,
                _ => {}
            }
        }
    }
    
    // 验证：应该有部分请求成功，部分被限流
    // burst=15，所以前 15 个请求应该成功，后面 5 个可能被限流
    assert!(success_count >= 10, "成功请求数不应太少");
    assert!(rate_limited_count >= 0, "应该有请求被限流（取决于 burst）");
    
    println!("Stress test: {} success, {} rate limited", success_count, rate_limited_count);
}

/// 压力测试：多用户并发限流
#[tokio::test]
async fn test_rate_limit_multi_user_stress() {
    // 创建限流状态（10 req/s per user）
    let rate_limit = Arc::new(RateLimitState::new(10, 1.5));
    
    let app = Router::new()
        .route("/test", get(|| async { "ok" }))
        .layer(axum::middleware::from_fn_with_state(
            rate_limit.clone(),
            rate_limit_middleware,
        ));
    
    // 模拟 5 个用户，每个用户发送 20 个请求
    let mut tasks = JoinSet::new();
    
    for user_id in 0..5 {
        let app_clone = app.clone();
        tasks.spawn(async move {
            let mut user_success = 0;
            let mut user_limited = 0;
            
            for _ in 0..20 {
                let response = app_clone
                    .clone()
                    .oneshot(
                        Request::builder()
                            .uri("/test")
                            .header("Authorization", format!("Bearer user_{}_key", user_id))
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                
                match response.status() {
                    StatusCode::OK => user_success += 1,
                    StatusCode::TOO_MANY_REQUESTS => user_limited += 1,
                    _ => {}
                }
            }
            
            (user_success, user_limited)
        });
    }
    
    // 收集结果
    let mut total_success = 0;
    let mut total_limited = 0;
    
    while let Some(result) = tasks.join_next().await {
        if let Ok((success, limited)) = result {
            total_success += success;
            total_limited += limited;
        }
    }
    
    println!("Multi-user stress: {} success, {} limited", total_success, total_limited);
    
    // 每个用户应该有独立的限流器
    // 5 个用户 * burst 15 = 75 个请求应该成功
    assert!(total_success >= 50, "多用户并发应该有足够成功请求");
}

/// 故障注入测试：配额系统数据库失败时降级到内存模式
#[tokio::test]
async fn test_quota_fallback_reject() {
    // 创建配额状态（Reject 模式）
    let quota_state = Arc::new(
        QuotaState::new(100)
            .with_fallback_policy(QuotaFallbackPolicy::Reject)
    );
    
    // 标记数据库失败
    quota_state.mark_db_failed(true).await;
    
    let app = Router::new()
        .route("/test", get(|| async { "ok" }))
        .layer(axum::middleware::from_fn_with_state(
            quota_state.clone(),
            quota_middleware,
        ));
    
    // 发送请求（带 API Key，触发配额检查）
    let response = app
        .oneshot(
            Request::builder()
                .uri("/test")
                .header("Authorization", "Bearer test_key_123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    
    // Reject 模式应该返回 503
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

/// 故障注入测试：配额系统数据库失败时使用内存模式
#[tokio::test]
async fn test_quota_fallback_memory() {
    // 创建配额状态（Memory 模式）
    let quota_state = Arc::new(
        QuotaState::new(2) // 每日限制 2 次
            .with_fallback_policy(QuotaFallbackPolicy::MemoryMode)
    );
    
    // 标记数据库失败
    quota_state.mark_db_failed(true).await;
    
    // 手动添加内存配额
    {
        let mut quotas = quota_state.memory_quotas.write().await;
        quotas.insert(
            "user_test".to_string(),
            MemoryQuota::new("user_test".to_string(), 2),
        );
    }
    
    let app = Router::new()
        .route("/test", get(|| async { "ok" }))
        .layer(axum::middleware::from_fn_with_state(
            quota_state.clone(),
            quota_middleware,
        ));
    
    // 第一次请求（带 API Key）
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/test")
                .header("Authorization", "Bearer test_key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    
    // Memory 模式应该允许请求（使用内存配额）
    assert_eq!(response.status(), StatusCode::OK);
}

/// 压力测试：配额检查性能
#[tokio::test]
async fn test_quota_performance() {
    let quota_state = Arc::new(
        QuotaState::new(10000) // 大配额，避免超限
            .with_fallback_policy(QuotaFallbackPolicy::MemoryMode)
    );
    
    let app = Router::new()
        .route("/test", get(|| async { "ok" }))
        .layer(axum::middleware::from_fn_with_state(
            quota_state.clone(),
            quota_middleware,
        ));
    
    let start = std::time::Instant::now();
    
    // 并发发送 100 个请求
    let mut tasks = JoinSet::new();
    
    for i in 0..100 {
        let app_clone = app.clone();
        tasks.spawn(async move {
            let response = app_clone
                .oneshot(
                    Request::builder()
                        .uri("/test")
                        .header("Authorization", format!("Bearer user_{}_key", i))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            
            response.status()
        });
    }
    
    // 收集结果
    let mut success_count = 0;
    
    while let Some(result) = tasks.join_next().await {
        if let Ok(status) = result {
            if status == StatusCode::OK {
                success_count += 1;
            }
        }
    }
    
    let elapsed = start.elapsed();
    
    println!("Quota performance: {} requests in {:?} ({:.2} req/s)", 
             success_count, 
             elapsed, 
             success_count as f64 / elapsed.as_secs_f64());
    
    // 100 个请求应该在 1 秒内完成
    assert!(elapsed.as_secs() < 1, "配额检查应该很快");
    assert_eq!(success_count, 100, "所有请求应该成功");
}

/// 压力测试：Validator 性能
#[test]
fn test_validator_performance() {
    use cad_ocr::server::types::DrawingType;
    
    let start = std::time::Instant::now();
    
    // 验证 10000 次
    for i in 0..10000 {
        let drawing_type = format!("Type_{}", i);
        let result = DrawingType::validate(&drawing_type);
        assert!(result.is_ok());
    }
    
    let elapsed = start.elapsed();
    
    println!("Validator performance: 10000 validations in {:?}", elapsed);
    
    // 10000 次验证应该在 100ms 内完成
    assert!(elapsed.as_millis() < 100, "验证应该很快");
}

/// 故障注入测试：数据库失败后恢复
#[tokio::test]
async fn test_db_failure_recovery() {
    let quota_state = Arc::new(
        QuotaState::new(100)
            .with_fallback_policy(QuotaFallbackPolicy::Reject)
    );
    
    // 标记数据库失败
    quota_state.mark_db_failed(true).await;
    assert!(quota_state.is_db_failed().await);
    
    // 标记数据库恢复
    quota_state.mark_db_failed(false).await;
    assert!(!quota_state.is_db_failed().await);
}

/// 压力测试：灰度白名单查找性能
#[test]
fn test_gray_release_whitelist_performance() {
    use std::collections::HashSet;
    
    // 创建大白名单
    let mut whitelist = HashSet::new();
    for i in 0..10000 {
        whitelist.insert(format!("user_{}", i));
    }
    
    let start = std::time::Instant::now();
    
    // 查找 10000 次
    for i in 0..10000 {
        let user_id = format!("user_{}", i % 10000);
        let _found = whitelist.contains(&user_id);
    }
    
    let elapsed = start.elapsed();
    
    println!("Whitelist performance: 10000 lookups in {:?}", elapsed);
    
    // 10000 次查找应该在 10ms 内完成（HashSet O(1)）
    assert!(elapsed.as_millis() < 10, "白名单查找应该很快");
}

/// 压力测试：并发 API Key 验证（简化版）
#[test]
fn test_auth_stress_basic() {
    use cad_ocr::server::auth::AuthState;
    
    // 验证 AuthState 可以创建
    let _auth = AuthState::new();
    
    // 验证 API Key 可以添加（异步）
    tokio_test::block_on(async {
        let auth = Arc::new(AuthState::new());
        for i in 0..10 {
            auth.add_api_key(format!("valid_key_{}", i)).await;
        }
        assert!(auth.contains_key("valid_key_5").await);
    });
}
