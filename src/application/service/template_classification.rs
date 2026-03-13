//! 模板分类应用服务 - 应用层批量处理和缓存管理
//!
//! ## 架构职责
//! - 领域层：`TemplateClassifier` trait 负责单张图片分类
//! - 应用层：`TemplateClassificationAppService` 负责批量处理、缓存、并发控制
//!
//! ## 依赖关系
//! ```text
//! TemplateClassificationAppService
//!     ├── Arc<dyn TemplateClassifier>  (领域层)
//!     ├── Arc<TemplateCache>           (基础设施层)
//!     └── 并发控制、超时、重试等应用逻辑
//! ```

use std::sync::Arc;
use std::time::Duration;
use crate::domain::{DomainResult, DomainError};
use crate::domain::service::template_selection::{TemplateClassifier, ClassificationResult};
use crate::infrastructure::template_selection::template_cache::{
    TemplateClassificationCache, TemplateCacheConfig,
};
use tracing::{info, warn, debug, instrument};

/// 应用服务配置
#[derive(Debug, Clone)]
pub struct TemplateClassificationAppConfig {
    /// 批量处理最大并发数
    pub batch_max_concurrency: usize,
    /// 单个分类超时（秒）
    pub classification_timeout_secs: u64,
    /// 是否启用缓存
    pub enable_cache: bool,
    /// 缓存配置
    pub cache_config: TemplateCacheConfig,
}

impl Default for TemplateClassificationAppConfig {
    fn default() -> Self {
        Self {
            batch_max_concurrency: 10,
            classification_timeout_secs: 60,
            enable_cache: true,
            cache_config: TemplateCacheConfig::default(),
        }
    }
}

/// 批量分类请求
#[derive(Debug, Clone)]
pub struct BatchClassificationRequest {
    /// 图片数据列表
    pub images: Vec<Vec<u8>>,
    /// 最大并发数（可选，覆盖配置）
    pub max_concurrency: Option<usize>,
}

/// 批量分类响应
#[derive(Debug, Clone)]
pub struct BatchClassificationResponse {
    /// 分类结果列表
    pub results: Vec<ClassificationResult>,
    /// 缓存命中数
    pub cache_hits: u64,
    /// 缓存未命中数
    pub cache_misses: u64,
    /// 总耗时（毫秒）
    pub total_duration_ms: u64,
}

/// 模板分类应用服务
///
/// ## 功能
/// - 批量分类（带并发控制）
/// - 缓存管理
/// - 超时和重试
/// - 指标统计
pub struct TemplateClassificationAppService {
    classifier: Arc<dyn TemplateClassifier>,
    cache: Arc<TemplateClassificationCache>,
    config: TemplateClassificationAppConfig,
}

impl TemplateClassificationAppService {
    /// 创建新服务
    pub fn new(
        classifier: Arc<dyn TemplateClassifier>,
        config: TemplateClassificationAppConfig,
    ) -> Self {
        let cache = Arc::new(TemplateClassificationCache::new(
            config.cache_config.clone()
        ));

        Self {
            classifier,
            cache,
            config,
        }
    }

    /// 创建带外部缓存的服务（用于共享缓存）
    pub fn with_cache(
        classifier: Arc<dyn TemplateClassifier>,
        cache: Arc<TemplateClassificationCache>,
        config: TemplateClassificationAppConfig,
    ) -> Self {
        Self {
            classifier,
            cache,
            config,
        }
    }

    /// 分类单张图片（带缓存和超时）
    #[instrument(skip(self, image_data), fields(image_size = image_data.len()))]
    pub async fn classify(&self, image_data: &[u8]) -> DomainResult<ClassificationResult> {
        // 1. 检查缓存
        if self.config.enable_cache {
            if let Some(cached_type) = self.cache.get(image_data).await {
                debug!("缓存命中，类型：{:?}", cached_type);
                return Ok(ClassificationResult {
                    template_type: cached_type,
                    confidence: 1.0, // 缓存命中置信度设为 1.0
                    needs_review: false,
                    source: "cache".to_string(),
                });
            }
        }

        // 2. 带超时调用分类器
        let result = tokio::time::timeout(
            Duration::from_secs(self.config.classification_timeout_secs),
            self.classifier.classify(image_data)
        )
        .await
        .map_err(|_| DomainError::external_service(
            "TemplateClassificationAppService",
            "classify",
            format!("分类超时 (>{:?})", self.config.classification_timeout_secs)
        ))?
        .map_err(|e| {
            warn!("分类失败：{}", e);
            e
        })?;

        // 3. 缓存结果
        if self.config.enable_cache {
            self.cache.insert(image_data, result.template_type.clone()).await;
        }

        Ok(result)
    }

    /// 批量分类（带并发控制）
    #[instrument(skip(self, request), fields(image_count = request.images.len()))]
    pub async fn classify_batch(
        &self,
        request: BatchClassificationRequest,
    ) -> DomainResult<BatchClassificationResponse> {
        use futures::stream::{self, StreamExt};
        use std::time::Instant;

        let start = Instant::now();
        let max_concurrency = request.max_concurrency
            .unwrap_or(self.config.batch_max_concurrency);

        info!(
            "开始批量分类：{} 张图片，并发数：{}",
            request.images.len(),
            max_concurrency
        );

        // 获取初始缓存统计
        let initial_stats = self.cache.stats().await;

        // 并发处理
        let results = stream::iter(request.images.iter())
            .map(|image_data| async move {
                self.classify(image_data.as_slice()).await
            })
            .buffered(max_concurrency)
            .collect::<Vec<_>>()
            .await;

        // 收集结果
        let mut ok_results = Vec::with_capacity(results.len());
        for result in results {
            match result {
                Ok(r) => ok_results.push(r),
                Err(e) => {
                    warn!("批量分类中部分失败：{}", e);
                    return Err(e);
                }
            }
        }

        // 计算缓存统计
        let final_stats = self.cache.stats().await;
        let duration_ms = start.elapsed().as_millis() as u64;

        info!(
            "批量分类完成：{} 张成功，耗时 {}ms，缓存命中率：{:.2}%",
            ok_results.len(),
            duration_ms,
            self.cache.hit_rate().await * 100.0
        );

        Ok(BatchClassificationResponse {
            results: ok_results,
            cache_hits: final_stats.hits - initial_stats.hits,
            cache_misses: final_stats.misses - initial_stats.misses,
            total_duration_ms: duration_ms,
        })
    }

    /// 获取缓存统计
    pub async fn cache_stats(&self) -> CacheStats {
        let stats = self.cache.stats().await;
        CacheStats {
            entries: stats.entries,
            hits: stats.hits,
            misses: stats.misses,
            hit_rate: stats.hit_rate(),
        }
    }

    /// 清空缓存
    pub async fn clear_cache(&self) {
        self.cache.clear().await;
        info!("缓存已清空");
    }

    /// 获取缓存命中率
    pub async fn hit_rate(&self) -> f64 {
        self.cache.hit_rate().await
    }
}

/// 缓存统计信息（应用层视图）
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub entries: usize,
    pub hits: u64,
    pub misses: u64,
    pub hit_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use crate::domain::model::drawing::CulvertType;

    /// Mock 分类器用于测试
    struct MockClassifier;

    #[async_trait::async_trait]
    impl TemplateClassifier for MockClassifier {
        async fn classify(&self, _image_data: &[u8]) -> DomainResult<ClassificationResult> {
            Ok(ClassificationResult {
                template_type: CulvertType::CulvertLayout,
                confidence: 0.9,
                needs_review: false,
                source: "mock".to_string(),
            })
        }
    }

    #[tokio::test]
    async fn test_app_service_basic() {
        let classifier = Arc::new(MockClassifier);
        let config = TemplateClassificationAppConfig::default();
        let service = TemplateClassificationAppService::new(classifier, config);

        let result = service.classify(b"test_image").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().template_type, CulvertType::CulvertLayout);
    }

    #[tokio::test]
    async fn test_app_service_cache() {
        let classifier = Arc::new(MockClassifier);
        let config = TemplateClassificationAppConfig::default();
        let service = TemplateClassificationAppService::new(classifier, config);

        // 第一次调用
        let result1 = service.classify(b"test_image").await.unwrap();
        assert_eq!(result1.source, "mock");

        // 第二次调用，应该命中缓存
        let result2 = service.classify(b"test_image").await.unwrap();
        assert_eq!(result2.source, "cache");
        assert_eq!(result2.confidence, 1.0);
    }
}
