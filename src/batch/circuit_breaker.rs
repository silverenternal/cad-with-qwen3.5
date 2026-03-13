//! 熔断器 - 防止连续失败导致资源浪费

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// 熔断器状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// 闭合状态 - 正常请求
    Closed,
    /// 打开状态 - 拒绝所有请求
    Open,
    /// 半开状态 - 允许少量请求测试
    HalfOpen,
}

/// 熔断器配置
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// 失败阈值，达到此值后打开熔断器
    pub failure_threshold: usize,
    /// 成功阈值，半开状态下达到此值后闭合熔断器
    pub success_threshold: usize,
    /// 熔断器打开后的超时时间
    pub timeout: Duration,
    /// 半开状态下允许的最大探测请求数
    pub max_probes_in_half_open: usize,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 2,
            timeout: Duration::from_secs(30),
            max_probes_in_half_open: 3,  // 半开状态最多允许 3 个探测请求
        }
    }
}

/// 熔断器
pub struct CircuitBreaker {
    /// 失败计数
    failure_count: AtomicUsize,
    /// 成功计数（半开状态）
    success_count: AtomicUsize,
    /// 熔断器是否打开
    is_open: AtomicBool,
    /// 是否处于半开状态
    is_half_open: AtomicBool,
    /// 熔断器打开的时间
    opened_at: RwLock<Option<Instant>>,
    /// 配置
    config: CircuitBreakerConfig,
    /// 半开状态下的探测请求计数
    probe_count: AtomicUsize,
}

impl CircuitBreaker {
    /// 创建熔断器
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            failure_count: AtomicUsize::new(0),
            success_count: AtomicUsize::new(0),
            is_open: AtomicBool::new(false),
            is_half_open: AtomicBool::new(false),
            opened_at: RwLock::new(None),
            config,
            probe_count: AtomicUsize::new(0),
        }
    }

    /// 记录成功
    pub fn record_success(&self) {
        if self.is_half_open.load(Ordering::SeqCst) {
            let count = self.success_count.fetch_add(1, Ordering::SeqCst) + 1;
            if count >= self.config.success_threshold {
                // 达到成功阈值，闭合熔断器
                self.reset();
            }
        }
        // 正常状态下也重置失败计数
        self.failure_count.store(0, Ordering::SeqCst);
    }

    /// 记录失败
    pub async fn record_failure(&self) {
        // 半开状态下失败，立即重新打开熔断器
        if self.is_half_open.load(Ordering::SeqCst) {
            self.open().await;
            return;
        }

        let count = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;

        if count >= self.config.failure_threshold && !self.is_open.load(Ordering::SeqCst) {
            // 达到失败阈值，打开熔断器
            self.open().await;
        }
    }

    /// 打开熔断器
    async fn open(&self) {
        self.is_open.store(true, Ordering::SeqCst);
        self.is_half_open.store(false, Ordering::SeqCst);
        *self.opened_at.write().await = Some(Instant::now());
    }

    /// 重置熔断器
    fn reset(&self) {
        self.is_open.store(false, Ordering::SeqCst);
        self.is_half_open.store(false, Ordering::SeqCst);
        self.failure_count.store(0, Ordering::SeqCst);
        self.success_count.store(0, Ordering::SeqCst);
    }

    /// 检查是否允许请求
    pub async fn allow_request(&self) -> bool {
        // 半开状态：限制探测请求数量
        if self.is_half_open.load(Ordering::SeqCst) {
            let probe_count = self.probe_count.fetch_add(1, Ordering::SeqCst) + 1;
            return probe_count <= self.config.max_probes_in_half_open;
        }

        if !self.is_open.load(Ordering::SeqCst) {
            // 闭合状态，允许请求
            return true;
        }

        // 检查是否超时
        if let Some(opened_at) = *self.opened_at.read().await {
            if opened_at.elapsed() >= self.config.timeout {
                // 超时，切换到半开状态
                self.is_half_open.store(true, Ordering::SeqCst);
                self.is_open.store(false, Ordering::SeqCst);  // 清除 open 标志
                self.success_count.store(0, Ordering::SeqCst);
                self.probe_count.store(1, Ordering::SeqCst);  // 第 1 个探测请求已使用
                return true;
            }
        }

        // 熔断器打开且未超时，拒绝请求
        false
    }

    /// 获取当前状态
    pub fn state(&self) -> CircuitState {
        if self.is_open.load(Ordering::SeqCst) {
            CircuitState::Open
        } else if self.is_half_open.load(Ordering::SeqCst) {
            CircuitState::HalfOpen
        } else {
            CircuitState::Closed
        }
    }

    /// 获取失败计数
    pub fn failure_count(&self) -> usize {
        self.failure_count.load(Ordering::SeqCst)
    }
}

/// 共享熔断器（线程安全）
pub type SharedCircuitBreaker = Arc<CircuitBreaker>;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_breaker() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout: Duration::from_millis(100),
            max_probes_in_half_open: 3,
        };
        let cb = CircuitBreaker::new(config);

        // 初始状态应为闭合
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.allow_request().await);

        // 记录 3 次失败，应打开
        for _ in 0..3 {
            cb.record_failure().await;
        }
        assert_eq!(cb.state(), CircuitState::Open);
        
        // 打开状态下请求会被拒绝
        assert!(!cb.allow_request().await);

        // 等待超时
        tokio::time::sleep(Duration::from_millis(150)).await;

        // 超时后应该允许请求（并切换到半开状态）
        assert!(cb.allow_request().await);
        // 注意：state() 在 allow_request() 后会切换到 HalfOpen
        assert_eq!(cb.state(), CircuitState::HalfOpen);

        // 记录 2 次成功，应闭合
        cb.record_success();
        cb.record_success();
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_probe_limit() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 2,
            timeout: Duration::from_millis(50),
            max_probes_in_half_open: 2,  // 只允许 2 个探测请求
        };
        let cb = CircuitBreaker::new(config);

        // 记录 2 次失败，打开熔断器
        cb.record_failure().await;
        cb.record_failure().await;
        assert_eq!(cb.state(), CircuitState::Open);

        // 等待超时
        tokio::time::sleep(Duration::from_millis(100)).await;

        // 第 1 个探测请求应该允许（触发切换到半开状态）
        assert!(cb.allow_request().await);
        assert_eq!(cb.state(), CircuitState::HalfOpen);

        // 第 2 个探测请求应该允许
        assert!(cb.allow_request().await);

        // 第 3 个探测请求应该被拒绝（超过限制）
        assert!(!cb.allow_request().await);

        // 记录失败应该重新打开熔断器
        cb.record_failure().await;
        assert_eq!(cb.state(), CircuitState::Open);
    }

    #[tokio::test]
    async fn test_circuit_breaker_boundary_zero_failure_threshold() {
        // 边界测试：失败阈值为 1，一次失败就打开
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 1,
            timeout: Duration::from_millis(50),
            max_probes_in_half_open: 1,
        };
        let cb = CircuitBreaker::new(config);

        // 初始状态应该是闭合
        assert_eq!(cb.state(), CircuitState::Closed);

        // 1 次失败就应该打开
        cb.record_failure().await;
        assert_eq!(cb.state(), CircuitState::Open);

        // 等待超时
        tokio::time::sleep(Duration::from_millis(100)).await;

        // 半开状态
        assert!(cb.allow_request().await);
        assert_eq!(cb.state(), CircuitState::HalfOpen);

        // 1 次成功就应该闭合
        cb.record_success();
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_stress_concurrent_failures() {
        // 压力测试：并发记录大量失败
        let config = CircuitBreakerConfig {
            failure_threshold: 10,
            success_threshold: 2,
            timeout: Duration::from_millis(100),
            max_probes_in_half_open: 3,
        };
        let cb = Arc::new(CircuitBreaker::new(config));

        // 并发记录 50 次失败
        let mut handles = vec![];
        for _ in 0..50 {
            let cb_clone = cb.clone();
            handles.push(tokio::spawn(async move {
                cb_clone.record_failure().await;
            }));
        }

        // 等待所有任务完成
        for handle in handles {
            handle.await.unwrap();
        }

        // 熔断器应该打开
        assert_eq!(cb.state(), CircuitState::Open);
    }

    #[tokio::test]
    async fn test_circuit_breaker_stress_concurrent_requests() {
        // 压力测试：并发检查是否允许请求
        let config = CircuitBreakerConfig {
            failure_threshold: 5,
            success_threshold: 2,
            timeout: Duration::from_millis(100),
            max_probes_in_half_open: 3,
        };
        let cb = Arc::new(CircuitBreaker::new(config));

        // 先记录 5 次失败打开熔断器
        for _ in 0..5 {
            cb.record_failure().await;
        }
        assert_eq!(cb.state(), CircuitState::Open);

        // 等待超时
        tokio::time::sleep(Duration::from_millis(150)).await;

        // 并发检查 20 次
        let mut handles = vec![];
        for _ in 0..20 {
            let cb_clone = cb.clone();
            handles.push(tokio::spawn(async move {
                cb_clone.allow_request().await
            }));
        }

        // 统计结果
        let mut allowed = 0;
        let mut denied = 0;
        for handle in handles {
            if handle.await.unwrap() {
                allowed += 1;
            } else {
                denied += 1;
            }
        }

        // 应该有 3 个允许（max_probes_in_half_open），17 个拒绝
        assert_eq!(allowed, 3);
        assert_eq!(denied, 17);
    }
}
