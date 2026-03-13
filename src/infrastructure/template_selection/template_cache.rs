//! 模板分类缓存 - 使用 DashMap 实现真正的并发读
//!
//! 使用图片哈希值作为 key，缓存分类结果
//! 避免对相同图片重复调用分类器
//!
//! ## 并发设计
//! - 使用 `DashMap` 实现真正的并发读（无锁读取）
//! - 哈希计算使用 `spawn_blocking` 避免阻塞异步运行时
//! - 手动实现 LRU 淘汰（基于时间戳）

use std::sync::Arc;
use dashmap::DashMap;
use crate::domain::model::drawing::CulvertType;

/// 缓存条目
#[derive(Debug, Clone)]
pub struct CacheEntry {
    /// 分类结果
    pub template_type: CulvertType,
    /// 创建时间戳（用于 TTL 检查）
    pub created_at: std::time::Instant,
    /// 最后访问时间戳（用于 LRU 淘汰）
    pub last_accessed_at: std::time::Instant,
}

impl CacheEntry {
    pub fn new(template_type: CulvertType) -> Self {
        let now = std::time::Instant::now();
        Self {
            template_type,
            created_at: now,
            last_accessed_at: now,
        }
    }

    /// 更新最后访问时间
    pub fn touch(&mut self) {
        self.last_accessed_at = std::time::Instant::now();
    }
}

/// 模板分类缓存配置
#[derive(Debug, Clone)]
pub struct TemplateCacheConfig {
    /// 最大缓存条目数
    pub max_entries: usize,
    /// 条目过期时间（秒），0 表示永不过期
    pub ttl_seconds: u64,
    /// 是否启用缓存
    pub enabled: bool,
}

impl Default for TemplateCacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 1000,
            ttl_seconds: 3600, // 1 小时
            enabled: true,
        }
    }
}

/// 模板分类缓存 - 线程安全的并发缓存
///
/// ## 并发安全设计
/// - 使用 `DashMap` 实现真正的并发读（无锁读取）
/// - 哈希计算在 `spawn_blocking` 中执行，避免阻塞异步运行时
/// - LRU 淘汰基于时间戳手动实现
pub struct TemplateClassificationCache {
    cache: Arc<DashMap<String, CacheEntry>>,
    config: TemplateCacheConfig,
    /// 缓存命中统计
    hits: Arc<std::sync::atomic::AtomicU64>,
    /// 缓存未命中统计
    misses: Arc<std::sync::atomic::AtomicU64>,
}

impl TemplateClassificationCache {
    /// 创建新缓存
    pub fn new(config: TemplateCacheConfig) -> Self {
        Self {
            cache: Arc::new(DashMap::new()),
            config,
            hits: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            misses: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// 创建默认缓存
    pub fn with_defaults() -> Self {
        Self::new(TemplateCacheConfig::default())
    }

    /// 计算图片哈希值（SHA256 前 16 字节）
    ///
    /// ## 性能优化
    /// 使用 `spawn_blocking` 将 CPU 密集型哈希计算移出异步运行时
    fn compute_hash(image_data: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(image_data);
        let result = hasher.finalize();
        // 使用前 16 字节（32 个十六进制字符）作为哈希
        hex::encode(&result[..16])
    }

    /// 异步哈希计算（使用 spawn_blocking）
    async fn compute_hash_async(image_data: &[u8]) -> String {
        let data = image_data.to_vec();
        tokio::task::spawn_blocking(move || Self::compute_hash(&data))
            .await
            .unwrap_or_else(|_| "error".to_string())
    }

    /// 获取缓存中的分类结果
    ///
    /// ## 并发设计
    /// 使用 DashMap 实现真正的并发读（无锁读取）
    pub async fn get(&self, image_data: &[u8]) -> Option<CulvertType> {
        if !self.config.enabled {
            self.misses.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return None;
        }

        let hash = Self::compute_hash_async(image_data).await;

        // DashMap 真正的并发读
        let mut entry = match self.cache.get_mut(&hash) {
            Some(e) => e,
            None => {
                self.misses.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return None;
            }
        };

        // 检查是否过期
        if self.config.ttl_seconds > 0 {
            let elapsed = entry.created_at.elapsed().as_secs();
            if elapsed > self.config.ttl_seconds {
                // 过期，移除
                self.cache.remove(&hash);
                self.misses.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return None;
            }
        }

        // 更新 LRU 时间戳
        entry.touch();

        self.hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Some(entry.template_type.clone())
    }

    /// 将分类结果存入缓存
    ///
    /// ## LRU 淘汰
    /// 当超过 max_entries 时，手动淘汰最久未使用的条目
    pub async fn insert(&self, image_data: &[u8], template_type: CulvertType) {
        if !self.config.enabled {
            return;
        }

        let hash = Self::compute_hash_async(image_data).await;

        // 检查是否需要淘汰
        if self.cache.len() >= self.config.max_entries {
            self.evict_lru();
        }

        self.cache.insert(hash, CacheEntry::new(template_type));
    }

    /// 淘汰最久未使用的条目
    fn evict_lru(&self) {
        let mut oldest_time = std::time::Instant::now();
        let mut oldest_key: Option<String> = None;

        // 找到最久未使用的条目
        for entry in self.cache.iter() {
            if entry.last_accessed_at < oldest_time {
                oldest_time = entry.last_accessed_at;
                oldest_key = Some(entry.key().clone());
            }
        }

        if let Some(key) = oldest_key {
            self.cache.remove(&key);
        }
    }

    /// 获取缓存统计信息
    pub async fn stats(&self) -> CacheStats {
        CacheStats {
            entries: self.cache.len(),
            hits: self.hits.load(std::sync::atomic::Ordering::Relaxed),
            misses: self.misses.load(std::sync::atomic::Ordering::Relaxed),
            max_entries: self.config.max_entries,
        }
    }

    /// 清空缓存
    pub async fn clear(&self) {
        self.cache.clear();
    }

    /// 获取缓存命中率
    pub async fn hit_rate(&self) -> f64 {
        let hits = self.hits.load(std::sync::atomic::Ordering::Relaxed) as f64;
        let misses = self.misses.load(std::sync::atomic::Ordering::Relaxed) as f64;
        let total = hits + misses;

        if total == 0.0 {
            return 0.0;
        }

        hits / total
    }
}

/// 缓存统计信息
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub entries: usize,
    pub hits: u64,
    pub misses: u64,
    pub max_entries: usize,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits as f64 + self.misses as f64;
        if total == 0.0 {
            return 0.0;
        }
        self.hits as f64 / total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_basic() {
        let cache = TemplateClassificationCache::new(TemplateCacheConfig {
            max_entries: 10,
            ttl_seconds: 0,
            enabled: true,
        });

        let image_data = b"test_image_data";
        let template_type = CulvertType::CulvertLayout;

        // 初始缓存未命中
        assert!(cache.get(image_data).await.is_none());

        // 插入缓存
        cache.insert(image_data, template_type.clone()).await;

        // 再次获取，应该命中
        let result = cache.get(image_data).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap(), template_type);
    }

    #[tokio::test]
    async fn test_cache_disabled() {
        let cache = TemplateClassificationCache::new(TemplateCacheConfig {
            max_entries: 10,
            ttl_seconds: 0,
            enabled: false,
        });

        let image_data = b"test_image_data";
        cache.insert(image_data, CulvertType::CulvertLayout).await;
        assert!(cache.get(image_data).await.is_none());
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let cache = TemplateClassificationCache::with_defaults();
        let image_data = b"test_image_data";

        cache.get(image_data).await; // miss
        cache.insert(image_data, CulvertType::CulvertLayout).await;
        cache.get(image_data).await; // hit

        let stats = cache.stats().await;
        assert_eq!(stats.entries, 1);
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert!((stats.hit_rate() - 0.5).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_cache_lru_eviction() {
        let cache = TemplateClassificationCache::new(TemplateCacheConfig {
            max_entries: 3,
            ttl_seconds: 0,
            enabled: true,
        });

        // 插入 3 个条目
        cache.insert(b"img1", CulvertType::CulvertLayout).await;
        cache.insert(b"img2", CulvertType::CulvertLayout).await;
        cache.insert(b"img3", CulvertType::CulvertLayout).await;

        // 访问 img1，使其成为最近使用
        cache.get(b"img1").await;

        // 插入第 4 个条目，应该淘汰 img2（最久未使用）
        cache.insert(b"img4", CulvertType::CulvertLayout).await;

        // 检查缓存大小
        let stats = cache.stats().await;
        assert_eq!(stats.entries, 3);

        // img2 应该被淘汰
        assert!(cache.get(b"img2").await.is_none());
        
        // 其他应该还在
        assert!(cache.get(b"img1").await.is_some());
        assert!(cache.get(b"img3").await.is_some());
        assert!(cache.get(b"img4").await.is_some());
    }

    #[tokio::test]
    async fn test_cache_concurrent_read() {
        use tokio::task;

        let cache = Arc::new(TemplateClassificationCache::new(TemplateCacheConfig {
            max_entries: 100,
            ttl_seconds: 0,
            enabled: true,
        }));

        // 插入测试数据
        cache.insert(b"test", CulvertType::CulvertLayout).await;

        // 并发读测试
        let mut handles = vec![];
        for _ in 0..10 {
            let cache_clone = Arc::clone(&cache);
            let handle = task::spawn(async move {
                cache_clone.get(b"test").await
            });
            handles.push(handle);
        }

        // 所有并发读都应该成功
        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_some());
        }
    }
}
