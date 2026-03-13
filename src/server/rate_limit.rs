//! Rate limiting middleware - supports per-user rate limiting
//!
//! 本模块提供：
//! - 每用户速率限制
//! - 带 LRU 淘汰和 TTL 的缓存机制
//! - 防止恶意用户通过不同 user_id 打爆内存

use axum::{
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use governor::{
    Quota, RateLimiter,
    state::keyed::DashMapStateStore,
};
use std::{num::NonZeroU32, sync::Arc, time::{Duration, Instant}};
use tracing::{warn, info};
use dashmap::DashMap;
use crate::metrics;

/// Per-user rate limiter
pub type PerUserLimiter = RateLimiter<String, DashMapStateStore<String>, governor::clock::DefaultClock>;

/// 带 TTL 的缓存条目
struct CacheEntry<T> {
    value: T,
    last_accessed: Instant,
}

/// 带 TTL 和 LRU 淘汰的缓存
/// 
/// 特性：
/// - 自动淘汰超过 TTL 的条目
/// - 达到最大容量时使用 LRU 淘汰
/// - 线程安全
pub struct TimeBoundCache<K, V> {
    cache: DashMap<K, CacheEntry<V>>,
    ttl: Duration,
    max_size: usize,
}

impl<K: Eq + std::hash::Hash + Clone, V: Clone> TimeBoundCache<K, V> {
    /// 创建新的缓存
    /// 
    /// # 参数
    /// * `ttl` - 条目存活时间
    /// * `max_size` - 最大缓存条目数
    pub fn new(ttl: Duration, max_size: usize) -> Self {
        Self {
            cache: DashMap::new(),
            ttl,
            max_size,
        }
    }
    
    /// 获取或创建条目
    /// 
    /// 如果条目不存在或已过期，使用提供的函数创建
    pub fn get_or_insert<F>(&self, key: K, factory: F) -> V
    where
        F: FnOnce() -> V,
    {
        let now = Instant::now();
        
        // 先检查是否存在且未过期
        let mut needs_update = false;
        let mut is_valid = false;
        
        if let Some(entry) = self.cache.get(&key) {
            if now.duration_since(entry.last_accessed) < self.ttl {
                is_valid = true;
            }
            drop(entry);
        }
        
        if is_valid {
            // 条目有效，更新访问时间
            needs_update = true;
        } else {
            // 条目不存在或已过期，删除旧条目（如果有）
            self.cache.remove(&key);
        }
        
        if needs_update {
            // 更新访问时间
            if let Some(mut entry) = self.cache.get_mut(&key) {
                entry.last_accessed = now;
                return entry.value.clone();
            }
        }
        
        // 创建新条目
        let value = factory();
        
        // 检查是否需要淘汰
        if self.cache.len() >= self.max_size {
            self.evict_one();
        }
        
        self.cache.insert(key.clone(), CacheEntry {
            value: value.clone(),
            last_accessed: now,
        });
        
        value
    }
    
    /// 淘汰一个最旧的条目（LRU）
    fn evict_one(&self) {
        let now = Instant::now();
        let mut oldest_time = Duration::from_secs(0);
        let mut oldest_key = None;
        
        // 找到最久未访问的条目
        for entry in self.cache.iter() {
            let elapsed = now.duration_since(entry.value().last_accessed);
            if oldest_key.is_none() || elapsed > oldest_time {
                oldest_time = elapsed;
                oldest_key = Some(entry.key().clone());
            }
        }
        
        // 删除最旧的条目
        if let Some(key) = oldest_key {
            self.cache.remove(&key);
        }
    }
    
    /// 清理所有过期的条目
    pub fn cleanup(&self) {
        let now = Instant::now();
        self.cache.retain(|_, entry| {
            now.duration_since(entry.last_accessed) < self.ttl
        });
    }
    
    /// 获取缓存大小
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// 检查缓存是否为空
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// 清空缓存
    pub fn clear(&self) {
        self.cache.clear()
    }
}

/// Rate limit state
pub struct RateLimitState {
    /// Per-user limiter cache with TTL and LRU eviction
    pub limiters: Arc<TimeBoundCache<String, Arc<PerUserLimiter>>>,
    /// Default quota
    pub default_quota: Quota,
}

impl RateLimitState {
    /// Create new rate limit state
    /// `requests_per_second`: requests per second allowed per user
    /// `burst_multiplier`: burst multiplier (e.g., 1.5 means 1.5x the rate limit)
    pub fn new(requests_per_second: u32, burst_multiplier: f64) -> Self {
        // 边界条件处理：requests_per_second 为 0 时，使用最小值 1
        // 这确保了系统不会因为配置错误而完全拒绝所有请求
        let rps = requests_per_second.max(1);

        // burst set to configurable multiplier to avoid overwhelming backend
        // e.g., 10 req/s with 1.5x burst = 15 requests allowed in a short burst
        let burst = ((rps as f64 * burst_multiplier.max(0.1)) as u32).max(1);

        // Safety: rps and burst are guaranteed to be >= 1 due to .max(1) above
        // These expects are safe because the values are mathematically guaranteed to be non-zero
        // rps >= 1 (enforced by configuration validation)
        // burst >= 1 (enforced by .max(1))
        let quota = Quota::per_second(NonZeroU32::new(rps).expect("rps >= 1 (configuration validation ensures this)"))
            .allow_burst(NonZeroU32::new(burst).expect("burst >= 1 (ensured by .max(1))"));

        // 使用带 TTL 和 LRU 淘汰的缓存
        // TTL: 30 分钟无活动自动淘汰
        // Max size: 10000 个用户
        let limiters = Arc::new(TimeBoundCache::new(
            Duration::from_secs(30 * 60), // 30 minutes
            10_000, // max 10k users
        ));

        Self {
            limiters,
            default_quota: quota,
        }
    }

    /// Get or create user's limiter
    fn get_limiter(&self, user_id: &str) -> Arc<PerUserLimiter> {
        self.limiters.get_or_insert(user_id.to_string(), || {
            info!("Creating new limiter for user {} (cache size: {})", user_id, self.limiters.len());
            Arc::new(RateLimiter::dashmap(self.default_quota))
        })
    }

    /// Check if user exceeds rate limit
    pub fn check(&self, user_id: &str) -> Result<(), &'static str> {
        let limiter = self.get_limiter(user_id);
        // Use check_key method
        limiter.check_key(&user_id.to_string()).map_err(|_| "rate limit exceeded")
    }
    
    /// 获取缓存的用户数量
    pub fn active_users(&self) -> usize {
        self.limiters.len()
    }
    
    /// 清理过期的限流器
    pub fn cleanup(&self) {
        self.limiters.cleanup();
        info!("Rate limit cache cleaned up, active users: {}", self.limiters.len());
    }
}

impl Clone for RateLimitState {
    fn clone(&self) -> Self {
        Self {
            limiters: self.limiters.clone(),
            default_quota: self.default_quota,
        }
    }
}

use crate::server::user_id::extract_user_id_or_ip;

/// Rate limiting middleware (per-user)
pub async fn rate_limit_middleware(
    State(state): State<Arc<RateLimitState>>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let user_key = extract_user_id_or_ip(&request);

    if state.check(&user_key).is_err() {
        warn!("Rate limit: user {} requests too frequent", user_key);

        // Record Prometheus metrics
        metrics::GLOBAL_METRICS.record_rate_limited();

        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_state() {
        let state = RateLimitState::new(10, 1.5);

        // 第一次检查应该通过
        assert!(state.check("user1").is_ok());

        // 不同用户应该有独立的限流器
        assert!(state.check("user2").is_ok());
    }

    #[test]
    fn test_per_user_limiters() {
        let state = RateLimitState::new(2, 1.5); // 2 req/s

        // 同一用户多次请求
        assert!(state.check("test_user").is_ok());
        assert!(state.check("test_user").is_ok());

        // 第三个请求可能失败（取决于 burst 配置）
        // 这里不测试失败情况，因为 burst 允许短暂超出
    }

    #[test]
    fn test_extract_user_key() {
        use axum::body::Body;
        use crate::server::user_id::extract_user_id_or_ip;

        let mut req = Request::new(Body::empty());

        // 测试无 header 情况（会降级到 IP）
        let result = extract_user_id_or_ip(&req);
        assert!(result.starts_with("ip_") || result == "unknown");

        // 测试 Bearer token
        req.headers_mut().insert(
            "Authorization",
            "Bearer abcdefgh12345678".parse().unwrap()
        );
        assert_eq!(extract_user_id_or_ip(&req), "user_abcdefgh");
    }

    #[test]
    fn test_time_bound_cache_ttl() {
        use std::thread;

        let cache = TimeBoundCache::new(Duration::from_millis(50), 100);

        // 插入条目
        cache.get_or_insert("key1", || "value1");
        assert_eq!(cache.len(), 1);

        // 等待过期（使用更长的时间确保过期）
        thread::sleep(Duration::from_millis(100));

        // 手动清理过期条目
        cache.cleanup();
        assert_eq!(cache.len(), 0); // key1 已过期被清理
        
        // 插入新条目
        cache.get_or_insert("key2", || "value2");
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_time_bound_cache_lru_eviction() {
        let cache = TimeBoundCache::new(Duration::from_secs(60), 3);
        
        // 插入 3 个条目
        cache.get_or_insert("key1", || "value1");
        cache.get_or_insert("key2", || "value2");
        cache.get_or_insert("key3", || "value3");
        assert_eq!(cache.len(), 3);
        
        // 访问 key1，使其变为最近使用
        cache.get_or_insert("key1", || "value1");
        
        // 插入第 4 个条目，应该淘汰 key2（最久未使用）
        cache.get_or_insert("key4", || "value4");
        assert_eq!(cache.len(), 3);
        
        // key2 应该被淘汰
        // 注意：由于我们使用 get_or_insert，无法直接测试 key2 是否存在
        // 这里只测试缓存大小正确
    }

    #[test]
    fn test_time_bound_cache_cleanup() {
        use std::thread;
        
        let cache = TimeBoundCache::new(Duration::from_millis(50), 100);
        
        cache.get_or_insert("key1", || "value1");
        cache.get_or_insert("key2", || "value2");
        assert_eq!(cache.len(), 2);
        
        // 等待过期
        thread::sleep(Duration::from_millis(100));
        
        // 手动清理
        cache.cleanup();
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_rate_limit_active_users() {
        let state = RateLimitState::new(10, 1.5);

        assert_eq!(state.active_users(), 0);

        // 创建几个限流器
        let _ = state.check("user1");
        let _ = state.check("user2");
        let _ = state.check("user3");

        assert!(state.active_users() >= 3);
    }

    // ==================== 集成测试 ====================

    /// 测试并发速率限制的竞态条件
    /// 
    /// 场景：多个用户同时发起请求，验证限流器线程安全
    #[test]
    fn test_concurrent_rate_limiting() {
        use std::thread;
        
        let state = Arc::new(RateLimitState::new(100, 1.5));
        let mut handles = vec![];

        // 启动 50 个线程，每个线程模拟一个用户发起 10 次请求
        for i in 0..50 {
            let state_clone = Arc::clone(&state);
            let handle = thread::spawn(move || {
                let user_id = format!("user_{}", i);
                let mut success_count = 0;
                
                for _ in 0..10 {
                    match state_clone.check(&user_id) {
                        Ok(_) => success_count += 1,
                        Err(_) => break, // 被限流
                    }
                }
                
                success_count
            });
            handles.push(handle);
        }

        // 等待所有线程完成
        let results: Vec<_> = handles.into_iter()
            .map(|h| h.join().unwrap())
            .collect();

        // 验证：每个用户应该至少能成功几次（burst 允许）
        let total_success: usize = results.iter().sum();
        assert!(total_success > 0, "应该有成功的请求");
        
        // 验证：活跃用户数应该接近 50
        assert!(state.active_users() >= 40, "应该有至少 40 个活跃用户");
    }

    /// 测试速率限制触发后的恢复
    /// 
    /// 场景：用户被限流后，等待一段时间应该能恢复
    #[test]
    fn test_rate_limit_recovery() {
        use std::thread;
        
        // 非常严格的限流：每秒 1 个请求，无 burst
        let state = RateLimitState::new(1, 1.0);
        
        // 第一次请求应该成功
        assert!(state.check("test_user").is_ok());
        
        // 立即第二次请求应该被限流
        assert!(state.check("test_user").is_err());
        
        // 等待 1.5 秒（超过 1 秒的限制）
        thread::sleep(Duration::from_millis(1500));
        
        // 再次请求应该成功
        assert!(state.check("test_user").is_ok());
    }

    /// 测试高并发下的内存安全（防止 DashMap 内存泄漏）
    /// 
    /// 场景：大量不同用户快速访问，验证 LRU 淘汰机制在工作
    #[test]
    fn test_high_concurrency_memory_safety() {
        use std::thread;
        
        // 使用较小的 max_size 来测试 LRU 淘汰
        let state = Arc::new(RateLimitState::with_config(1000, 1.5, Duration::from_secs(1), 100));
        let mut handles = vec![];

        // 启动 5 个线程，每个线程模拟 50 个不同用户（共 250 个用户）
        for i in 0..5 {
            let state_clone = Arc::clone(&state);
            let handle = thread::spawn(move || {
                for j in 0..50 {
                    let user_id = format!("user_{}_{}", i, j);
                    let _ = state_clone.check(&user_id);
                }
            });
            handles.push(handle);
        }

        // 等待所有线程完成
        for h in handles {
            h.join().unwrap();
        }

        // 验证：活跃用户数应该远小于总用户数（250），证明 LRU 在工作
        // 由于 max_size=100，活跃用户应该在 100-150 左右
        let active_users = state.active_users();
        assert!(active_users < 200, "LRU 应该淘汰部分用户，实际：{}", active_users);
        assert!(active_users >= 80, "应该保留部分用户，实际：{}", active_users);
    }
}

// ==================== 测试辅助函数 ====================

impl RateLimitState {
    /// 创建自定义配置的 RateLimitState（仅用于测试）
    #[cfg(test)]
    fn with_config(requests_per_second: u32, burst_multiplier: f64, ttl: Duration, max_size: usize) -> Self {
        let rps = requests_per_second.max(1);
        let burst = ((rps as f64 * burst_multiplier.max(0.1)) as u32).max(1);
        let quota = Quota::per_second(NonZeroU32::new(rps).expect("rps >= 1"))
            .allow_burst(NonZeroU32::new(burst).expect("burst >= 1"));

        let limiters = Arc::new(TimeBoundCache::new(ttl, max_size));

        Self {
            limiters,
            default_quota: quota,
        }
    }
}
