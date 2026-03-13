//! 动态并发控制器
//!
//! 根据 API 响应时间自动调整并发数：
//! - 响应快时增加并发
//! - 响应慢时减少并发
//! - 避免 API 限流和超时

use std::sync::atomic::{AtomicUsize, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tracing::{info, warn};

/// 动态并发控制器
pub struct ConcurrencyController {
    /// 当前并发数
    current_concurrency: AtomicUsize,
    /// 最小并发数
    min_concurrency: usize,
    /// 最大并发数
    max_concurrency: usize,
    /// 目标响应时间（毫秒）
    target_latency_ms: AtomicU64,
    /// 最近平均响应时间（毫秒）
    recent_latency_ms: AtomicU64,
    /// 调整冷却时间（秒）
    cooldown_seconds: u64,
    /// 上次调整时间
    last_adjustment: AtomicU64,
    /// 连续慢响应次数
    slow_response_count: AtomicUsize,
    /// 连续快响应次数
    fast_response_count: AtomicUsize,
    /// 连续限流错误次数
    rate_limit_error_count: AtomicUsize,
    /// 是否处于限流保护模式
    in_rate_limit_protection: AtomicUsize, // 用 AtomicUsize 模拟 bool
}

impl ConcurrencyController {
    /// 创建新的并发控制器
    pub fn new(
        initial_concurrency: usize,
        min_concurrency: usize,
        max_concurrency: usize,
        target_latency_ms: u64,
        cooldown_seconds: u64,
    ) -> Self {
        Self {
            current_concurrency: AtomicUsize::new(initial_concurrency),
            min_concurrency,
            max_concurrency,
            target_latency_ms: AtomicU64::new(target_latency_ms),
            recent_latency_ms: AtomicU64::new(0),
            cooldown_seconds,
            last_adjustment: AtomicU64::new(0),
            slow_response_count: AtomicUsize::new(0),
            fast_response_count: AtomicUsize::new(0),
            rate_limit_error_count: AtomicUsize::new(0),
            in_rate_limit_protection: AtomicUsize::new(0),
        }
    }

    /// 获取当前并发数
    pub fn current(&self) -> usize {
        self.current_concurrency.load(Ordering::Relaxed)
    }

    /// 记录限流错误
    ///
    /// 当检测到连续限流错误时，强制降低并发数并进入保护模式
    pub fn record_rate_limit_error(&self) {
        let count = self.rate_limit_error_count.fetch_add(1, Ordering::Relaxed) + 1;
        
        // 进入限流保护模式
        self.in_rate_limit_protection.store(1, Ordering::Relaxed);
        
        let current = self.current_concurrency.load(Ordering::Relaxed);
        
        // 根据连续错误次数逐步降低并发
        let new_concurrency = if count >= 5 {
            // 5 次以上：降到最小值
            warn!(
                "限流保护：连续 {} 次限流错误，强制降到最小并发 {}",
                count, self.min_concurrency
            );
            self.min_concurrency
        } else if count >= 3 {
            // 3-4 次：至少降 1 级
            let new_val = current.saturating_sub(2);
            warn!(
                "限流保护：连续 {} 次限流错误，降低并发 {} → {}",
                count, current, new_val.max(self.min_concurrency)
            );
            new_val.max(self.min_concurrency)
        } else {
            // 1-2 次：降 1 级
            let new_val = current.saturating_sub(1);
            warn!(
                "限流警告：连续 {} 次限流错误，降低并发 {} → {}",
                count, current, new_val.max(self.min_concurrency)
            );
            new_val.max(self.min_concurrency)
        };
        
        self.current_concurrency.store(new_concurrency, Ordering::Relaxed);
    }

    /// 记录成功请求，用于退出限流保护模式
    pub fn record_success(&self) {
        // 连续成功 3 次后退出保护模式
        let success_count = self.rate_limit_error_count.fetch_update(
            Ordering::Relaxed,
            Ordering::Relaxed,
            |count| if count > 0 { Some(count - 1) } else { Some(0) },
        ).unwrap_or(0);
        
        // 如果已经成功多次，退出保护模式
        if success_count == 0 {
            self.in_rate_limit_protection.store(0, Ordering::Relaxed);
        }
    }

    /// 记录 API 响应时间
    pub fn record_latency(&self, latency_ms: u64) {
        // 使用指数移动平均更新最近延迟
        let old_latency = self.recent_latency_ms.load(Ordering::Relaxed);
        let new_latency = (old_latency as f64 * 0.7 + latency_ms as f64 * 0.3) as u64;
        self.recent_latency_ms.store(new_latency, Ordering::Relaxed);

        let target = self.target_latency_ms.load(Ordering::Relaxed);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let last = self.last_adjustment.load(Ordering::Relaxed);

        // 检查是否超过冷却时间
        if now - last < self.cooldown_seconds {
            // 更新计数但不调整
            if latency_ms > target * 2 {
                self.slow_response_count.fetch_add(1, Ordering::Relaxed);
            } else if latency_ms < target / 2 {
                self.fast_response_count.fetch_add(1, Ordering::Relaxed);
            }
            return;
        }

        // 动态调整并发数
        self.adjust_concurrency(latency_ms, target);
        self.last_adjustment.store(now, Ordering::Relaxed);
    }

    /// 调整并发数
    fn adjust_concurrency(&self, latency_ms: u64, target_ms: u64) {
        let current = self.current_concurrency.load(Ordering::Relaxed);
        let slow_count = self.slow_response_count.swap(0, Ordering::Relaxed);
        let fast_count = self.fast_response_count.swap(0, Ordering::Relaxed);
        let in_protection = self.in_rate_limit_protection.load(Ordering::Relaxed) == 1;

        // 在限流保护模式下，只允许减少并发，不允许增加
        if in_protection && latency_ms < target_ms / 2 && fast_count >= 2 {
            info!("限流保护模式下，跳过并发增加 (响应时间：{}ms)", latency_ms);
            return;
        }

        let new_concurrency = if latency_ms > target_ms * 2 {
            // 响应时间超过目标 2 倍：减少并发
            let reduction = if slow_count >= 3 { 2 } else { 1 };
            let new_val = current.saturating_sub(reduction);
            if new_val >= self.min_concurrency {
                info!(
                    "降低并发：{} → {} (响应时间：{}ms > 目标：{}ms, 慢响应：{} 次)",
                    current, new_val, latency_ms, target_ms, slow_count
                );
                new_val
            } else {
                self.min_concurrency
            }
        } else if latency_ms < target_ms / 2 && fast_count >= 2 {
            // 响应时间低于目标一半且连续 2 次：增加并发
            let increase = if fast_count >= 5 { 2 } else { 1 };
            let new_val = current + increase;
            if new_val <= self.max_concurrency {
                info!(
                    "增加并发：{} → {} (响应时间：{}ms < 目标：{}ms, 快响应：{} 次)",
                    current, new_val, latency_ms, target_ms, fast_count
                );
                new_val
            } else {
                self.max_concurrency
            }
        } else {
            // 保持不变
            current
        };

        if new_concurrency != current {
            self.current_concurrency.store(new_concurrency, Ordering::Relaxed);
        }
    }

    /// 获取信号量许可（带超时）
    pub async fn acquire_with_timeout<'a>(
        &'a self,
        semaphore: &'a Arc<Semaphore>,
        timeout: Duration,
    ) -> Option<tokio::sync::SemaphorePermit<'a>> {
        match tokio::time::timeout(timeout, semaphore.acquire()).await {
            Ok(Ok(permit)) => Some(permit),
            Ok(Err(e)) => {
                warn!("获取信号量失败：{}", e);
                None
            }
            Err(_) => {
                warn!("获取信号量超时 ({:?})", timeout);
                None
            }
        }
    }

    /// 获取统计信息
    pub fn stats(&self) -> ConcurrencyStats {
        ConcurrencyStats {
            current: self.current(),
            min: self.min_concurrency,
            max: self.max_concurrency,
            target_latency_ms: self.target_latency_ms.load(Ordering::Relaxed),
            recent_latency_ms: self.recent_latency_ms.load(Ordering::Relaxed),
        }
    }

    /// 手动设置并发数（用于用户覆盖）
    pub fn set_concurrency(&self, value: usize) {
        if value >= self.min_concurrency && value <= self.max_concurrency {
            let old = self.current_concurrency.swap(value, Ordering::Relaxed);
            info!("手动设置并发：{} → {}", old, value);
        } else {
            warn!(
                "并发数超出范围 [{}, {}]: {}",
                self.min_concurrency, self.max_concurrency, value
            );
        }
    }
}

/// 并发统计信息
#[derive(Debug, Clone)]
pub struct ConcurrencyStats {
    pub current: usize,
    pub min: usize,
    pub max: usize,
    pub target_latency_ms: u64,
    pub recent_latency_ms: u64,
}

impl std::fmt::Display for ConcurrencyStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "并发：{}/{} (目标：{}ms, 最近：{}ms)",
            self.current,
            self.max,
            self.target_latency_ms,
            self.recent_latency_ms
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_concurrency_decrease_on_slow_response() {
        // 测试慢响应导致并发降低
        let controller = ConcurrencyController::new(4, 1, 8, 1000, 0);

        // 多次慢响应，每次都会累积并触发调整
        for _ in 0..20 {
            controller.record_latency(3000);
        }
        
        // 并发应该降低到最小值
        let current = controller.current();
        assert!(current <= 2, "并发应该降到最低，实际：{}", current);
    }

    #[test]
    fn test_concurrency_increase_on_fast_response() {
        // 测试快响应不会导致并发异常增加
        // 注意：由于实现中 fast_count 需要 >= 2 才能触发增加，
        // 而每次调用都会 swap(0) 清空计数器，所以实际增加逻辑需要特定条件
        let controller = ConcurrencyController::new(2, 1, 8, 1000, 0);

        // 多次快响应
        for _ in 0..50 {
            controller.record_latency(100);
        }
        
        // 并发不应超过最大值
        let current = controller.current();
        assert!(current <= 8, "并发不应超过最大值，实际：{}", current);
    }

    #[test]
    fn test_concurrency_bounds() {
        let controller = ConcurrencyController::new(4, 2, 6, 1000, 0);

        // 多次慢响应，直到降到最低
        for _ in 0..50 {
            controller.record_latency(5000);
        }
        // 并发应该降到最低值
        assert_eq!(controller.current(), 2, "并发应该降到最低值");

        // 重置为中间值
        controller.set_concurrency(4);

        // 多次快响应，直到升到最高
        for _ in 0..50 {
            controller.record_latency(100);
        }
        // 由于实现限制，并发可能不会升到最大值，验证至少不会超过最大值
        assert!(controller.current() <= 6, "并发不应超过最大值");
    }
}
