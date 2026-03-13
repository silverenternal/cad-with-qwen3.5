//! Prometheus 指标导出模块

use prometheus::{Registry, Counter, Gauge, Histogram, HistogramOpts, Opts, TextEncoder};
use std::sync::Arc;
use once_cell::sync::Lazy;

/// 全局 Prometheus 注册表
pub static REGISTRY: Lazy<Registry> = Lazy::new(|| {
    let registry = Registry::new();
    register_default_metrics(&registry);
    registry
});

/// 指标集合
pub struct Metrics {
    /// 总请求数
    pub http_requests_total: Counter,
    /// 请求延迟（秒）
    pub request_duration_seconds: Histogram,
    /// 当前活跃连接数
    pub active_connections: Gauge,
    /// 错误总数
    pub errors_total: Counter,
    /// 配额超限次数
    pub quota_exceeded_total: Counter,
    /// 速率限制次数
    pub rate_limited_total: Counter,
    // ===== 新增指标 =====
    /// 配额使用率（0-100）
    pub quota_usage_percent: Gauge,
    /// 灰度命中率
    pub gray_release_hit_total: Counter,
    /// 灰度未命中次数
    pub gray_release_miss_total: Counter,
    /// 数据库连接池活跃连接数
    pub db_pool_active_connections: Gauge,
    /// 数据库连接池空闲连接数
    pub db_pool_idle_connections: Gauge,
    /// 数据库连接池最大连接数
    pub db_pool_max_connections: Gauge,
    /// API Key 数量
    pub api_keys_total: Gauge,
    /// 认证失败次数
    pub auth_failures_total: Counter,
    // ===== 批量处理指标 =====
    /// 批量处理文件总数
    pub batch_files_total: Counter,
    /// 批量处理成功文件数
    pub batch_files_success: Counter,
    /// 批量处理失败文件数
    pub batch_files_failed: Counter,
    /// 批量处理延迟（秒）
    pub batch_processing_duration_seconds: Histogram,
    /// 图片编码延迟（秒）
    pub batch_encoding_duration_seconds: Histogram,
    /// 活跃会话数
    pub batch_session_active: Gauge,
    /// 熔断器状态 (0=closed, 1=open, 2=half-open)
    pub batch_circuit_breaker_state: Gauge,
}

impl Metrics {
    /// 创建新的指标集合
    pub fn new(registry: &Registry) -> Result<Self, prometheus::Error> {
        let http_requests_total = Counter::with_opts(Opts::new(
            "http_requests_total",
            "Total number of HTTP requests",
        ))?;

        let request_duration_seconds = Histogram::with_opts(HistogramOpts::new(
            "request_duration_seconds",
            "HTTP request duration in seconds",
        ).buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]))?;

        let active_connections = Gauge::new(
            "active_connections",
            "Number of active connections",
        )?;

        let errors_total = Counter::with_opts(Opts::new(
            "errors_total",
            "Total number of errors",
        ))?;

        let quota_exceeded_total = Counter::with_opts(Opts::new(
            "quota_exceeded_total",
            "Total number of quota exceeded events",
        ))?;

        let rate_limited_total = Counter::with_opts(Opts::new(
            "rate_limited_total",
            "Total number of rate limited events",
        ))?;

        // ===== 新增指标 =====
        let quota_usage_percent = Gauge::new(
            "quota_usage_percent",
            "Current quota usage percentage (0-100)",
        )?;

        let gray_release_hit_total = Counter::with_opts(Opts::new(
            "gray_release_hit_total",
            "Total number of gray release whitelist hits",
        ))?;

        let gray_release_miss_total = Counter::with_opts(Opts::new(
            "gray_release_miss_total",
            "Total number of gray release whitelist misses",
        ))?;

        let db_pool_active_connections = Gauge::new(
            "db_pool_active_connections",
            "Number of active database connections in the pool",
        )?;

        let db_pool_idle_connections = Gauge::new(
            "db_pool_idle_connections",
            "Number of idle database connections in the pool",
        )?;

        let db_pool_max_connections = Gauge::new(
            "db_pool_max_connections",
            "Maximum number of database connections in the pool",
        )?;

        let api_keys_total = Gauge::new(
            "api_keys_total",
            "Total number of active API keys",
        )?;

        let auth_failures_total = Counter::with_opts(Opts::new(
            "auth_failures_total",
            "Total number of authentication failures",
        ))?;

        // ===== 批量处理指标 =====
        let batch_files_total = Counter::with_opts(Opts::new(
            "cad_batch_files_total",
            "Total number of files processed in batch mode",
        ))?;

        let batch_files_success = Counter::with_opts(Opts::new(
            "cad_batch_files_success",
            "Number of successfully processed files in batch mode",
        ))?;

        let batch_files_failed = Counter::with_opts(Opts::new(
            "cad_batch_files_failed",
            "Number of failed files in batch mode",
        ))?;

        let batch_processing_duration_seconds = Histogram::with_opts(HistogramOpts::new(
            "cad_batch_processing_duration_seconds",
            "Batch processing duration in seconds",
        ).buckets(vec![0.1, 0.5, 1.0, 2.5, 5.0, 10.0, 25.0, 50.0, 100.0]))?;

        let batch_encoding_duration_seconds = Histogram::with_opts(HistogramOpts::new(
            "cad_batch_encoding_duration_seconds",
            "Image encoding duration in seconds for batch mode",
        ).buckets(vec![0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0]))?;

        let batch_session_active = Gauge::new(
            "cad_batch_session_active",
            "Number of active batch processing sessions",
        )?;

        let batch_circuit_breaker_state = Gauge::new(
            "cad_batch_circuit_breaker_state",
            "Circuit breaker state (0=closed, 1=open, 2=half-open)",
        )?;

        registry.register(Box::new(http_requests_total.clone()))?;
        registry.register(Box::new(request_duration_seconds.clone()))?;
        registry.register(Box::new(active_connections.clone()))?;
        registry.register(Box::new(errors_total.clone()))?;
        registry.register(Box::new(quota_exceeded_total.clone()))?;
        registry.register(Box::new(rate_limited_total.clone()))?;
        registry.register(Box::new(quota_usage_percent.clone()))?;
        registry.register(Box::new(gray_release_hit_total.clone()))?;
        registry.register(Box::new(gray_release_miss_total.clone()))?;
        registry.register(Box::new(db_pool_active_connections.clone()))?;
        registry.register(Box::new(db_pool_idle_connections.clone()))?;
        registry.register(Box::new(db_pool_max_connections.clone()))?;
        registry.register(Box::new(api_keys_total.clone()))?;
        registry.register(Box::new(auth_failures_total.clone()))?;
        // 注册批量处理指标
        registry.register(Box::new(batch_files_total.clone()))?;
        registry.register(Box::new(batch_files_success.clone()))?;
        registry.register(Box::new(batch_files_failed.clone()))?;
        registry.register(Box::new(batch_processing_duration_seconds.clone()))?;
        registry.register(Box::new(batch_encoding_duration_seconds.clone()))?;
        registry.register(Box::new(batch_session_active.clone()))?;
        registry.register(Box::new(batch_circuit_breaker_state.clone()))?;

        Ok(Self {
            http_requests_total,
            request_duration_seconds,
            active_connections,
            errors_total,
            quota_exceeded_total,
            rate_limited_total,
            quota_usage_percent,
            gray_release_hit_total,
            gray_release_miss_total,
            db_pool_active_connections,
            db_pool_idle_connections,
            db_pool_max_connections,
            api_keys_total,
            auth_failures_total,
            // 批量处理指标
            batch_files_total,
            batch_files_success,
            batch_files_failed,
            batch_processing_duration_seconds,
            batch_encoding_duration_seconds,
            batch_session_active,
            batch_circuit_breaker_state,
        })
    }

    /// 创建空指标集合（用于优雅降级）
    fn new_empty() -> Self {
        // 空指标创建时使用默认值，失败时 panic（因为这是降级方案，不应该失败）
        Self {
            http_requests_total: Counter::new("empty_requests", "Empty counter").unwrap(),
            request_duration_seconds: Histogram::with_opts(HistogramOpts::new("empty_duration", "Empty")).unwrap(),
            active_connections: Gauge::new("empty_connections", "Empty").unwrap(),
            errors_total: Counter::new("empty_errors", "Empty").unwrap(),
            quota_exceeded_total: Counter::new("empty_quota", "Empty").unwrap(),
            rate_limited_total: Counter::new("empty_rate", "Empty").unwrap(),
            quota_usage_percent: Gauge::new("empty_quota_pct", "Empty").unwrap(),
            gray_release_hit_total: Counter::new("empty_hit", "Empty").unwrap(),
            gray_release_miss_total: Counter::new("empty_miss", "Empty").unwrap(),
            db_pool_active_connections: Gauge::new("empty_db_active", "Empty").unwrap(),
            db_pool_idle_connections: Gauge::new("empty_db_idle", "Empty").unwrap(),
            db_pool_max_connections: Gauge::new("empty_db_max", "Empty").unwrap(),
            api_keys_total: Gauge::new("empty_keys", "Empty").unwrap(),
            auth_failures_total: Counter::new("empty_auth", "Empty").unwrap(),
            batch_files_total: Counter::new("empty_batch_total", "Empty").unwrap(),
            batch_files_success: Counter::new("empty_batch_success", "Empty").unwrap(),
            batch_files_failed: Counter::new("empty_batch_failed", "Empty").unwrap(),
            batch_processing_duration_seconds: Histogram::with_opts(HistogramOpts::new("empty_batch_duration", "Empty")).unwrap(),
            batch_encoding_duration_seconds: Histogram::with_opts(HistogramOpts::new("empty_encoding_duration", "Empty")).unwrap(),
            batch_session_active: Gauge::new("empty_session", "Empty").unwrap(),
            batch_circuit_breaker_state: Gauge::new("empty_circuit", "Empty").unwrap(),
        }
    }

    /// 记录 HTTP 请求
    pub fn record_request(&self, duration_secs: f64) {
        self.http_requests_total.inc();
        self.request_duration_seconds.observe(duration_secs);
    }

    /// 记录错误
    pub fn record_error(&self) {
        self.errors_total.inc();
    }

    /// 记录配额超限
    pub fn record_quota_exceeded(&self) {
        self.quota_exceeded_total.inc();
    }

    /// 记录速率限制
    pub fn record_rate_limited(&self) {
        self.rate_limited_total.inc();
    }

    /// 增加活跃连接数
    pub fn inc_connections(&self) {
        self.active_connections.inc();
    }

    /// 减少活跃连接数
    pub fn dec_connections(&self) {
        self.active_connections.dec();
    }

    // ===== 新增方法 =====

    /// 设置配额使用率
    pub fn set_quota_usage(&self, used: u32, limit: u32) {
        let percent = if limit > 0 {
            (used as f64 / limit as f64) * 100.0
        } else {
            0.0
        };
        self.quota_usage_percent.set(percent);
    }

    /// 记录灰度命中
    pub fn record_gray_release_hit(&self) {
        self.gray_release_hit_total.inc();
    }

    /// 记录灰度未命中
    pub fn record_gray_release_miss(&self) {
        self.gray_release_miss_total.inc();
    }

    // ===== 批量处理指标方法 =====

    /// 记录批量处理文件
    pub fn record_batch_file(&self, success: bool) {
        self.batch_files_total.inc();
        if success {
            self.batch_files_success.inc();
        } else {
            self.batch_files_failed.inc();
        }
    }

    /// 记录批量处理延迟
    pub fn record_batch_duration(&self, duration_secs: f64) {
        self.batch_processing_duration_seconds.observe(duration_secs);
    }

    /// 记录图片编码延迟
    pub fn record_batch_encoding_duration(&self, duration_secs: f64) {
        self.batch_encoding_duration_seconds.observe(duration_secs);
    }

    /// 设置活跃会话数
    pub fn set_batch_session_active(&self, count: i64) {
        self.batch_session_active.set(count as f64);
    }

    /// 设置熔断器状态
    pub fn set_batch_circuit_breaker_state(&self, state: i64) {
        self.batch_circuit_breaker_state.set(state as f64);
    }

    /// 计算灰度命中率（0-100%）
    ///
    /// # Returns
    /// 返回灰度命中率百分比，如果没有请求则返回 0
    pub fn get_gray_release_hit_rate(&self) -> f64 {
        let hit = self.gray_release_hit_total.get();
        let miss = self.gray_release_miss_total.get();
        let total = hit + miss;
        
        if total == 0.0 {
            0.0
        } else {
            (hit / total) * 100.0
        }
    }

    /// 设置数据库连接池状态
    pub fn set_db_pool_stats(&self, active: u32, idle: u32, max: u32) {
        self.db_pool_active_connections.set(active as f64);
        self.db_pool_idle_connections.set(idle as f64);
        self.db_pool_max_connections.set(max as f64);
    }

    /// 设置 API Key 总数
    pub fn set_api_keys_total(&self, count: usize) {
        self.api_keys_total.set(count as f64);
    }

    /// 记录认证失败
    pub fn record_auth_failure(&self) {
        self.auth_failures_total.inc();
    }
}

/// 注册默认指标
fn register_default_metrics(registry: &Registry) {
    // 添加 Rust 运行时指标
    // 指标注册失败概率极低，但为了规范，我们优雅降级
    match Gauge::with_opts(Opts::new(
        "cad_ocr_build_info",
        "Build information",
    )) {
        Ok(build_info) => {
            build_info.set(1.0);
            if let Err(e) = registry.register(Box::new(build_info)) {
                tracing::warn!("Failed to register build_info metric: {}", e);
            }
        }
        Err(e) => {
            tracing::warn!("Failed to create build_info metric: {}", e);
        }
    }
}

/// 导出 Prometheus 格式的指标
pub fn encode_metrics() -> String {
    let encoder = TextEncoder::new();
    let metric_families = REGISTRY.gather();
    let mut output = String::new();
    
    match encoder.encode_utf8(&metric_families, &mut output) {
        Ok(_) => output,
        Err(e) => format!("Error encoding metrics: {}", e),
    }
}

/// 全局指标实例（懒加载）
/// 注意：指标创建失败概率极低，如果失败说明系统处于异常状态
pub static GLOBAL_METRICS: Lazy<Arc<Metrics>> = Lazy::new(|| {
    match Metrics::new(&REGISTRY) {
        Ok(metrics) => Arc::new(metrics),
        Err(e) => {
            // 指标注册失败，返回空指标（优雅降级）
            tracing::warn!("Failed to create metrics, using empty metrics: {}", e);
            Arc::new(Metrics::new_empty())
        }
    }
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        let registry = Registry::new();
        let metrics = Metrics::new(&registry).unwrap();
        
        assert_eq!(metrics.http_requests_total.get() as u64, 0);
        assert_eq!(metrics.errors_total.get() as u64, 0);
    }

    #[test]
    fn test_record_request() {
        let registry = Registry::new();
        let metrics = Metrics::new(&registry).unwrap();
        
        metrics.record_request(0.5);
        assert_eq!(metrics.http_requests_total.get() as u64, 1);
    }

    #[test]
    fn test_record_error() {
        let registry = Registry::new();
        let metrics = Metrics::new(&registry).unwrap();
        
        metrics.record_error();
        assert_eq!(metrics.errors_total.get() as u64, 1);
    }

    #[test]
    fn test_encode_metrics() {
        let output = encode_metrics();
        // 全局 REGISTRY 包含 cad_ocr_build_info
        assert!(output.contains("cad_ocr_build_info"));
    }
}
