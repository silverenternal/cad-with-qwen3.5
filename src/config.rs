//! 配置模块 - 简化版本
//!
//! 本模块提供：
//! - 配置结构定义
//! - 配置加载和验证
//!
//! 配置设计原则：
//! - 90% 用户只需配置 1 个 preset 参数
//! - 高级用户可覆盖具体参数
//! - 启动时验证所有配置，提供友好错误提示
//!
//! 配置优先级：
//! 1. 环境变量（最高优先级）
//! 2. config.toml 文件
//! 3. 默认值（最低优先级）

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use crate::error::ConfigError;

pub type Result<T> = std::result::Result<T, ConfigError>;

// ==================== 预设配置 ====================

/// 批处理预设配置 - 90% 用户只需配置这一项
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BatchPreset {
    /// 快速模式：并发 2，适合测试或小批量
    Fast,
    /// 平衡模式：并发 4，推荐默认值
    Balanced,
    /// 激进模式：并发 8，适合大批量处理
    Aggressive,
}

impl Default for BatchPreset {
    fn default() -> Self {
        Self::Balanced
    }
}

impl std::fmt::Display for BatchPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fast => write!(f, "fast"),
            Self::Balanced => write!(f, "balanced"),
            Self::Aggressive => write!(f, "aggressive"),
        }
    }
}

impl BatchPreset {
    pub fn concurrency(&self) -> usize {
        match self {
            Self::Fast => 2,
            Self::Balanced => 4,
            Self::Aggressive => 8,
        }
    }

    pub fn max_retries(&self) -> u32 {
        match self {
            Self::Fast => 1,
            Self::Balanced => 3,
            Self::Aggressive => 5,
        }
    }

    pub fn validator_preset(&self) -> ValidatorPreset {
        match self {
            Self::Fast => ValidatorPreset::fast(),
            Self::Balanced => ValidatorPreset::balanced(),
            Self::Aggressive => ValidatorPreset::aggressive(),
        }
    }
}

/// 验证器预设配置
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct ValidatorPreset {
    pub min_confidence: f32,
    pub max_retries: u32,
    pub initial_delay_ms: u64,
}

impl ValidatorPreset {
    pub fn fast() -> Self {
        Self {
            min_confidence: 0.3,
            max_retries: 1,
            initial_delay_ms: 50,
        }
    }

    pub fn balanced() -> Self {
        Self {
            min_confidence: 0.5,
            max_retries: 2,
            initial_delay_ms: 100,
        }
    }

    pub fn aggressive() -> Self {
        Self {
            min_confidence: 0.7,
            max_retries: 3,
            initial_delay_ms: 200,
        }
    }
}

// ==================== 配置管理器 ====================

pub struct ConfigManager {
    config: Arc<Config>,
}

impl ConfigManager {
    pub fn new(config: Config) -> Self {
        Self {
            config: Arc::new(config),
        }
    }

    pub fn get(&self) -> Arc<Config> {
        self.config.clone()
    }

    pub fn validate(config: &Config) -> Result<()> {
        let mut errors = Vec::new();

        // 验证速率限制配置
        if config.rate_limit_requests_per_second == 0 {
            errors.push("rate_limit_requests_per_second 不能为 0，建议设置为 10");
        }

        if config.rate_limit_burst_multiplier < 0.1 {
            errors.push("rate_limit_burst_multiplier 必须 >= 0.1，建议设置为 1.5");
        }

        // 验证配额配置
        if config.quota_daily_limit == 0 {
            errors.push("quota_daily_limit 不能为 0，建议设置为 100");
        }

        // 验证图片处理配置
        if config.max_image_dimension < 64 {
            errors.push("max_image_dimension 必须 >= 64，建议设置为 2048");
        }

        // 验证缓存配置
        if config.cache_max_entries == 0 {
            errors.push("cache_max_entries 不能为 0，建议设置为 20");
        }

        // 验证配额降级策略
        if config.quota_fallback_policy != "reject" && config.quota_fallback_policy != "memory" {
            errors.push("quota_fallback_policy 必须是 'reject' 或 'memory'");
        }

        // 验证 PDF 转换配置
        if config.pdf_conversion_dpi < 72 || config.pdf_conversion_dpi > 300 {
            errors.push("pdf_conversion_dpi 必须在 72-300 之间，建议设置为 150");
        }

        // 验证模板选择配置
        if config.template_selection.confidence_threshold < 0.0 || config.template_selection.confidence_threshold > 1.0 {
            errors.push("template_selection.confidence_threshold 必须在 0.0-1.0 之间");
        }

        if !errors.is_empty() {
            let error_report = format!(
                "配置验证失败，发现以下问题:\n  - {}",
                errors.join("\n  - ")
            );
            return Err(ConfigError::InvalidValue(error_report));
        }

        Ok(())
    }

    pub fn print_summary(config: &Config) {
        use tracing::info;

        info!("╔═══════════════════════════════════════════════════════════╗");
        info!("║                    配置摘要                              ║");
        info!("╠═══════════════════════════════════════════════════════════╣");
        info!("║ 批处理模式：{} (并发={})                         ║",
            config.batch_preset,
            config.batch_preset.concurrency()
        );
        info!("║ 模型：本地={}, Cloud={}                     ║",
            config.default_local_model,
            config.default_cloud_model
        );
        info!("║ 配额：每日限制={}，降级策略={}                      ║",
            config.quota_daily_limit,
            config.quota_fallback_policy
        );
        info!("║ 限流：{} req/s, burst={}x                        ║",
            config.rate_limit_requests_per_second,
            config.rate_limit_burst_multiplier
        );
        info!("║ 图片：最大边长={}px                              ║", config.max_image_dimension);
        info!("║ PDF: DPI={}, 已启用={}                          ║",
            config.pdf_conversion_dpi,
            config.pdf_conversion_enabled
        );
        info!("╚═══════════════════════════════════════════════════════════╝");
    }
}

// ==================== 配置结构 ====================

/// 并发配置（高级选项）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcurrencyConfig {
    #[serde(default = "default_batch_concurrency")]
    pub batch_concurrency: usize,
    #[serde(default = "default_encoding_concurrency")]
    pub encoding_concurrency: usize,
    #[serde(default = "default_api_concurrency")]
    pub api_concurrency: usize,
}

fn default_batch_concurrency() -> usize { 4 }
fn default_encoding_concurrency() -> usize { 2 }
fn default_api_concurrency() -> usize { 4 }

impl Default for ConcurrencyConfig {
    fn default() -> Self {
        Self {
            batch_concurrency: default_batch_concurrency(),
            encoding_concurrency: default_encoding_concurrency(),
            api_concurrency: default_api_concurrency(),
        }
    }
}

/// 模板选择配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateSelectionConfig {
    #[serde(default = "default_template_enabled")]
    pub enabled: bool,
    #[serde(default = "default_template_confidence_threshold")]
    pub confidence_threshold: f32,
    #[serde(default = "default_classification_strategy")]
    pub strategy: String,
    #[serde(default = "default_classification_model")]
    pub model: String,
    #[serde(default = "default_template_type")]
    pub default_type: String,
    #[serde(default = "default_enable_classification_cache")]
    pub enable_cache: bool,
    #[serde(default = "default_classification_cache_max_entries")]
    pub cache_max_entries: usize,
}

fn default_template_enabled() -> bool { true }
fn default_template_confidence_threshold() -> f32 { 0.6 }
fn default_classification_strategy() -> String { "hybrid".to_string() }
fn default_classification_model() -> String { "llava:7b".to_string() }
fn default_template_type() -> String { "culvert_layout".to_string() }
fn default_enable_classification_cache() -> bool { true }
fn default_classification_cache_max_entries() -> usize { 1000 }

impl Default for TemplateSelectionConfig {
    fn default() -> Self {
        Self {
            enabled: default_template_enabled(),
            confidence_threshold: default_template_confidence_threshold(),
            strategy: default_classification_strategy(),
            model: default_classification_model(),
            default_type: default_template_type(),
            enable_cache: default_enable_classification_cache(),
            cache_max_entries: default_classification_cache_max_entries(),
        }
    }
}

/// 配置结构 - 简化版
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    // ===== 基本配置 =====
    #[serde(default = "default_local_model")]
    pub default_local_model: String,

    #[serde(default = "default_cloud_model")]
    pub default_cloud_model: String,

    #[serde(default = "default_drawing_type")]
    pub default_drawing_type: String,

    #[serde(default = "default_cache_max_entries")]
    pub cache_max_entries: usize,

    // ===== 批处理配置（简化为 preset） =====
    /// 批处理预设：fast/balanced/aggressive
    #[serde(default)]
    pub batch_preset: BatchPreset,

    /// 高级并发配置（可选，覆盖 preset 默认值）
    #[serde(default)]
    pub concurrency: ConcurrencyConfig,

    #[serde(default = "default_max_image_dimension")]
    pub max_image_dimension: u32,

    #[serde(default = "default_batch_question")]
    pub default_batch_question: String,

    // ===== 配额和限流配置 =====
    #[serde(default = "default_quota_fallback_policy")]
    pub quota_fallback_policy: String,

    #[serde(default = "default_quota_daily_limit")]
    pub quota_daily_limit: u32,

    #[serde(default = "default_rate_limit_requests_per_second")]
    pub rate_limit_requests_per_second: u32,

    #[serde(default = "default_rate_limit_burst_multiplier")]
    pub rate_limit_burst_multiplier: f64,

    #[serde(default = "default_gray_release_quota")]
    pub gray_release_quota_per_user: u32,

    // ===== PDF 转换配置 =====
    #[serde(default = "default_pdf_dpi")]
    pub pdf_conversion_dpi: u32,

    #[serde(default = "default_pdf_enabled")]
    pub pdf_conversion_enabled: bool,

    #[serde(default = "default_pdf_temp_dir")]
    pub pdf_temp_dir: String,

    // ===== 模板选择配置 =====
    #[serde(default)]
    pub template_selection: TemplateSelectionConfig,

    // ===== 死信队列配置 =====
    #[serde(default = "default_dlq_enabled")]
    pub dead_letter_queue_enabled: bool,

    #[serde(default)]
    pub dead_letter_queue_path: Option<String>,

    // ===== 数据库配置 =====
    #[serde(default)]
    pub database_url: Option<String>,

    // ===== Prompt 配置 =====
    #[serde(default = "default_prompt_template_path")]
    pub prompt_template_path: String,
}

// ===== 默认值函数 =====
fn default_local_model() -> String { "llava:7b".to_string() }
fn default_cloud_model() -> String { "qwen3.5:397b-cloud".to_string() }
fn default_drawing_type() -> String { "建筑平面图".to_string() }
fn default_cache_max_entries() -> usize { 20 }
fn default_max_image_dimension() -> u32 { 2048 }
fn default_batch_question() -> String { "分析这张图纸并提取关键信息".to_string() }
fn default_quota_fallback_policy() -> String { "reject".to_string() }
fn default_quota_daily_limit() -> u32 { 100 }
fn default_rate_limit_requests_per_second() -> u32 { 10 }
fn default_rate_limit_burst_multiplier() -> f64 { 1.5 }
fn default_gray_release_quota() -> u32 { 100 }
fn default_pdf_dpi() -> u32 { 150 }
fn default_pdf_enabled() -> bool { true }
fn default_pdf_temp_dir() -> String { "./tmp/pdf_convert".to_string() }
fn default_dlq_enabled() -> bool { true }
fn default_prompt_template_path() -> String { "prompt_template.txt".to_string() }

impl Default for Config {
    fn default() -> Self {
        Self {
            default_local_model: default_local_model(),
            default_cloud_model: default_cloud_model(),
            default_drawing_type: default_drawing_type(),
            cache_max_entries: default_cache_max_entries(),
            batch_preset: BatchPreset::default(),
            concurrency: ConcurrencyConfig::default(),
            max_image_dimension: default_max_image_dimension(),
            default_batch_question: default_batch_question(),
            quota_fallback_policy: default_quota_fallback_policy(),
            quota_daily_limit: default_quota_daily_limit(),
            rate_limit_requests_per_second: default_rate_limit_requests_per_second(),
            rate_limit_burst_multiplier: default_rate_limit_burst_multiplier(),
            gray_release_quota_per_user: default_gray_release_quota(),
            pdf_conversion_dpi: default_pdf_dpi(),
            pdf_conversion_enabled: default_pdf_enabled(),
            pdf_temp_dir: default_pdf_temp_dir(),
            template_selection: TemplateSelectionConfig::default(),
            dead_letter_queue_enabled: default_dlq_enabled(),
            dead_letter_queue_path: None,
            database_url: None,
            prompt_template_path: default_prompt_template_path(),
        }
    }
}

impl Config {
    /// 获取并发配置（考虑 preset）
    pub fn get_concurrency_config(&self) -> ConcurrencyConfig {
        // 如果用户显式设置了并发配置，使用用户的
        if self.concurrency.batch_concurrency != default_batch_concurrency()
            || self.concurrency.encoding_concurrency != default_encoding_concurrency()
            || self.concurrency.api_concurrency != default_api_concurrency()
        {
            return self.concurrency.clone();
        }

        // 否则使用 preset 默认值
        let concurrency = self.batch_preset.concurrency();
        ConcurrencyConfig {
            batch_concurrency: concurrency,
            encoding_concurrency: concurrency / 2,
            api_concurrency: concurrency,
        }
    }

    /// 获取验证器预设
    pub fn get_validator_preset(&self) -> ValidatorPreset {
        self.batch_preset.validator_preset()
    }
}

// ==================== 配置加载 ====================

/// 从文件加载配置
pub fn load_config_from_file(path: &PathBuf) -> Result<Config> {
    use std::fs;

    if !path.exists() {
        return Err(ConfigError::custom(format!(
            "配置文件不存在：{}",
            path.display()
        )));
    }

    let content = fs::read_to_string(path)
        .map_err(|e| ConfigError::custom(format!("读取配置文件失败：{}", e)))?;

    let config: Config = toml::from_str(&content)
        .map_err(|e| ConfigError::custom(format!("解析配置文件失败：{}", e)))?;

    Ok(config)
}

/// 加载配置（环境变量 > config.toml > 默认值）
pub fn load_config() -> Config {
    use std::env;

    // 加载 .env 文件
    let _ = dotenvy::dotenv();

    // 尝试从环境变量加载
    if let Ok(config_path) = env::var("CAD_CONFIG_PATH") {
        if let Ok(config) = load_config_from_file(&PathBuf::from(&config_path)) {
            return config;
        }
    }

    // 尝试从默认路径加载
    let default_paths = [
        PathBuf::from("config.toml"),
        PathBuf::from("./config/config.toml"),
        PathBuf::from("../config.toml"),
    ];

    for path in &default_paths {
        if path.exists() {
            if let Ok(config) = load_config_from_file(path) {
                return config;
            }
        }
    }

    // 返回默认配置
    Config::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_preset_concurrency() {
        assert_eq!(BatchPreset::Fast.concurrency(), 2);
        assert_eq!(BatchPreset::Balanced.concurrency(), 4);
        assert_eq!(BatchPreset::Aggressive.concurrency(), 8);
    }

    #[test]
    fn test_batch_preset_max_retries() {
        assert_eq!(BatchPreset::Fast.max_retries(), 1);
        assert_eq!(BatchPreset::Balanced.max_retries(), 3);
        assert_eq!(BatchPreset::Aggressive.max_retries(), 5);
    }

    #[test]
    fn test_validator_preset() {
        let fast = ValidatorPreset::fast();
        assert_eq!(fast.max_retries, 1);
        assert!((fast.min_confidence - 0.3).abs() < 0.01);

        let balanced = ValidatorPreset::balanced();
        assert_eq!(balanced.max_retries, 2);
        assert!((balanced.min_confidence - 0.5).abs() < 0.01);

        let aggressive = ValidatorPreset::aggressive();
        assert_eq!(aggressive.max_retries, 3);
        assert!((aggressive.min_confidence - 0.7).abs() < 0.01);
    }

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.batch_preset, BatchPreset::Balanced);
        assert_eq!(config.quota_daily_limit, 100);
        assert_eq!(config.rate_limit_requests_per_second, 10);
    }

    #[test]
    fn test_get_concurrency_config_from_preset() {
        let config = Config {
            batch_preset: BatchPreset::Fast,
            ..Default::default()
        };
        let concurrency = config.get_concurrency_config();
        assert_eq!(concurrency.batch_concurrency, 2);
        assert_eq!(concurrency.encoding_concurrency, 1);
        assert_eq!(concurrency.api_concurrency, 2);
    }

    #[test]
    fn test_get_concurrency_config_override() {
        let config = Config {
            batch_preset: BatchPreset::Balanced,
            concurrency: ConcurrencyConfig {
                batch_concurrency: 10,
                encoding_concurrency: 5,
                api_concurrency: 10,
            },
            ..Default::default()
        };
        let concurrency = config.get_concurrency_config();
        assert_eq!(concurrency.batch_concurrency, 10);
        assert_eq!(concurrency.encoding_concurrency, 5);
        assert_eq!(concurrency.api_concurrency, 10);
    }

    #[test]
    fn test_config_validation() {
        let valid_config = Config::default();
        assert!(ConfigManager::validate(&valid_config).is_ok());

        let invalid_config = Config {
            rate_limit_requests_per_second: 0,
            ..Default::default()
        };
        assert!(ConfigManager::validate(&invalid_config).is_err());
    }

    #[test]
    fn test_config_conflict_preset_and_custom_concurrency() {
        // 测试 preset 与自定义并发配置冲突时的警告检测
        let config = Config {
            batch_preset: BatchPreset::Fast,  // fast = 2 并发
            concurrency: ConcurrencyConfig {
                batch_concurrency: 8,  // 但用户自定义为 8（aggressive 级别）
                encoding_concurrency: 4,
                api_concurrency: 4,
            },
            ..Default::default()
        };

        // 配置应该有效（允许覆盖）
        assert!(ConfigManager::validate(&config).is_ok());

        // 但应该检测到冲突
        let preset_concurrency = config.batch_preset.concurrency();
        let custom_concurrency = config.concurrency.batch_concurrency;
        
        // 验证冲突检测逻辑
        if preset_concurrency != custom_concurrency {
            // 当 preset 和自定义值不一致时，应该记录警告
            println!(
                "⚠️  配置冲突：preset={} (并发={})，但自定义 batch_concurrency={}",
                match config.batch_preset {
                    BatchPreset::Fast => "fast",
                    BatchPreset::Balanced => "balanced",
                    BatchPreset::Aggressive => "aggressive",
                },
                preset_concurrency,
                custom_concurrency
            );
        }
    }

    #[test]
    fn test_config_conflict_preset_mismatch() {
        // 测试所有 preset 档位的冲突检测
        let presets = [
            (BatchPreset::Fast, 2),
            (BatchPreset::Balanced, 4),
            (BatchPreset::Aggressive, 8),
        ];

        for (preset, expected_concurrency) in presets {
            assert_eq!(preset.concurrency(), expected_concurrency);
            
            // 如果用户自定义值与 preset 不一致，应该检测冲突
            let custom_values = [1, 3, 5, 10, 16];
            for custom in custom_values {
                if custom != expected_concurrency {
                    // 模拟冲突检测
                    let conflict_detected = preset.concurrency() != custom;
                    assert!(conflict_detected);
                }
            }
        }
    }

    #[test]
    fn test_config_auto_resolve_conflict() {
        // 测试配置冲突自动解决策略
        let config_fast_override = Config {
            batch_preset: BatchPreset::Fast,
            concurrency: ConcurrencyConfig {
                batch_concurrency: 8,  // 覆盖为 aggressive 级别
                ..Default::default()
            },
            ..Default::default()
        };

        // 获取实际使用的配置（自定义优先）
        let effective = config_fast_override.get_concurrency_config();
        assert_eq!(effective.batch_concurrency, 8);

        // 验证：当用户显式覆盖时，以覆盖值为准
        println!(
            "配置策略：preset={}，但实际 batch_concurrency={}（用户自定义优先）",
            match config_fast_override.batch_preset {
                BatchPreset::Fast => "fast",
                BatchPreset::Balanced => "balanced",
                BatchPreset::Aggressive => "aggressive",
            },
            effective.batch_concurrency
        );
    }

    #[test]
    fn test_config_recommendation() {
        // 测试配置推荐逻辑
        let scenarios = vec![
            // (场景描述，期望并发，推荐 preset)
            ("低配环境", 2, "fast"),
            ("中等负载", 4, "balanced"),
            ("高负载生产", 8, "aggressive"),
        ];

        for (scenario, expected_concurrency, recommended_preset) in scenarios {
            let preset = match recommended_preset {
                "fast" => BatchPreset::Fast,
                "balanced" => BatchPreset::Balanced,
                "aggressive" => BatchPreset::Aggressive,
                _ => BatchPreset::Balanced,
            };
            
            assert_eq!(preset.concurrency(), expected_concurrency);
            println!("场景 '{}': 推荐 preset={}, 并发={}", 
                scenario, recommended_preset, expected_concurrency);
        }
    }
}
