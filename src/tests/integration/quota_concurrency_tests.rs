//! 并发配额消耗测试
//!
//! 验证在高并发场景下配额检查的原子性和正确性

use std::sync::Arc;
use tokio::task;

/// 模拟配额服务
struct MockQuotaService {
    remaining: Arc<tokio::sync::Mutex<u32>>,
}

impl MockQuotaService {
    fn new(initial: u32) -> Self {
        Self {
            remaining: Arc::new(tokio::sync::Mutex::new(initial)),
        }
    }

    async fn consume(&self) -> bool {
        let mut remaining = self.remaining.lock().await;
        if *remaining > 0 {
            *remaining -= 1;
            true
        } else {
            false
        }
    }

    async fn get_remaining(&self) -> u32 {
        *self.remaining.lock().await
    }
}

#[tokio::test]
async fn test_concurrent_quota_consumption() {
    // 场景：100 个并发请求，配额只有 10 次
    let quota = Arc::new(MockQuotaService::new(10));
    let mut handles = vec![];

    // 创建 100 个并发任务
    for _ in 0..100 {
        let quota_clone = Arc::clone(&quota);
        let handle = task::spawn(async move {
            quota_clone.consume().await
        });
        handles.push(handle);
    }

    // 等待所有任务完成
    let mut success_count = 0;
    for handle in handles {
        if let Ok(success) = handle.await {
            if success {
                success_count += 1;
            }
        }
    }

    // 验证：只有 10 次成功，90 次失败
    assert_eq!(success_count, 10, "应该只有 10 次请求成功");
    assert_eq!(quota.get_remaining().await, 0, "配额应该用完");
}

#[tokio::test]
async fn test_quota_race_condition() {
    // 场景：配额刚好用完时的竞态条件
    let quota = Arc::new(MockQuotaService::new(1));
    let mut handles = vec![];

    // 创建 10 个并发任务，只有 1 个配额
    for _ in 0..10 {
        let quota_clone = Arc::clone(&quota);
        let handle = task::spawn(async move {
            quota_clone.consume().await
        });
        handles.push(handle);
    }

    let mut success_count = 0;
    for handle in handles {
        if let Ok(success) = handle.await {
            if success {
                success_count += 1;
            }
        }
    }

    // 验证：只有 1 次成功
    assert_eq!(success_count, 1, "应该只有 1 次请求成功");
}

#[tokio::test]
async fn test_quota_boundary_zero() {
    // 边界条件：配额为 0
    let quota = Arc::new(MockQuotaService::new(0));
    
    // 尝试消费
    let success = quota.consume().await;
    assert!(!success, "配额为 0 时应该失败");
}

#[tokio::test]
async fn test_quota_boundary_large() {
    // 边界条件：大配额（10000）
    let quota = Arc::new(MockQuotaService::new(10000));
    let mut handles = vec![];

    // 创建 10000 个并发任务
    for _ in 0..10000 {
        let quota_clone = Arc::clone(&quota);
        let handle = task::spawn(async move {
            quota_clone.consume().await
        });
        handles.push(handle);
    }

    let mut success_count = 0;
    for handle in handles {
        if let Ok(success) = handle.await {
            if success {
                success_count += 1;
            }
        }
    }

    // 验证：所有请求都成功
    assert_eq!(success_count, 10000, "所有请求应该成功");
    assert_eq!(quota.get_remaining().await, 0);
}

#[tokio::test]
async fn test_concurrent_quota_with_delay() {
    // 场景：带延迟的并发配额消费
    let quota = Arc::new(MockQuotaService::new(50));
    let mut handles = vec![];

    for i in 0..100 {
        let quota_clone = Arc::clone(&quota);
        let handle = task::spawn(async move {
            // 模拟一些处理延迟
            tokio::time::sleep(tokio::time::Duration::from_millis(i as u64 % 10)).await;
            quota_clone.consume().await
        });
        handles.push(handle);
    }

    let mut success_count = 0;
    for handle in handles {
        if let Ok(success) = handle.await {
            if success {
                success_count += 1;
            }
        }
    }

    assert_eq!(success_count, 50, "应该只有 50 次请求成功");
}
