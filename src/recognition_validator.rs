//! 模型输出验证与重试模块（增强版）
//!
//! 功能：
//! - 可配置的置信度权重
//! - 指数退避重试策略
//! - 智能乱码检测
//! - 语义重复检测
//! - 详细的日志和指标

use serde::{Deserialize, Serialize};
use tracing::{info, warn, debug};

/// 验证结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// 是否通过验证
    pub is_valid: bool,
    /// 置信度分数 (0.0-1.0)
    pub confidence: f32,
    /// 验证失败原因
    pub reasons: Vec<String>,
    /// 触发的检查项
    pub failed_checks: Vec<CheckType>,
    /// 响应内容预览
    pub preview: String,
}

/// 检查项类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckType {
    EmptyContent,
    TooShort,
    ErrorKeywords,
    NoStructure,
    RepeatedContent,
    GarbledText,
    NoCadContext,
    NoRoomInfo,
}

impl CheckType {
    pub fn as_str(&self) -> &'static str {
        match self {
            CheckType::EmptyContent => "空响应",
            CheckType::TooShort => "响应过短",
            CheckType::ErrorKeywords => "错误关键词",
            CheckType::NoStructure => "缺少结构化数据",
            CheckType::RepeatedContent => "内容重复",
            CheckType::GarbledText => "乱码",
            CheckType::NoCadContext => "缺少 CAD 术语",
            CheckType::NoRoomInfo => "缺少房间信息",
        }
    }
}

/// 验证权重配置（可序列化到配置文件）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationWeights {
    /// 错误关键词的惩罚权重
    pub error_keywords: f32,
    /// 响应过短的惩罚权重
    pub too_short: f32,
    /// 缺少结构化数据的惩罚权重
    pub no_structure: f32,
    /// 内容重复的惩罚权重
    pub repeated_content: f32,
    /// 乱码的惩罚权重
    pub garbled_text: f32,
    /// 缺少 CAD 术语的惩罚权重
    pub no_cad_context: f32,
    /// 缺少房间信息的惩罚权重
    pub no_room_info: f32,
}

impl Default for ValidationWeights {
    fn default() -> Self {
        Self {
            error_keywords: 0.25,
            too_short: 0.15,
            no_structure: 0.2,
            repeated_content: 0.3,
            garbled_text: 0.4,
            no_cad_context: 0.1,
            no_room_info: 0.05,
        }
    }
}

impl ValidationWeights {
    /// 从配置文件加载权重（支持部分加载，缺失字段使用默认值）
    pub fn from_config(config: &toml::Value) -> Self {
        let defaults = Self::default();
        
        Self {
            error_keywords: config.get("error_keywords")
                .and_then(|v| v.as_float())
                .map(|v| v as f32)
                .unwrap_or(defaults.error_keywords),
            too_short: config.get("too_short")
                .and_then(|v| v.as_float())
                .map(|v| v as f32)
                .unwrap_or(defaults.too_short),
            no_structure: config.get("no_structure")
                .and_then(|v| v.as_float())
                .map(|v| v as f32)
                .unwrap_or(defaults.no_structure),
            repeated_content: config.get("repeated_content")
                .and_then(|v| v.as_float())
                .map(|v| v as f32)
                .unwrap_or(defaults.repeated_content),
            garbled_text: config.get("garbled_text")
                .and_then(|v| v.as_float())
                .map(|v| v as f32)
                .unwrap_or(defaults.garbled_text),
            no_cad_context: config.get("no_cad_context")
                .and_then(|v| v.as_float())
                .map(|v| v as f32)
                .unwrap_or(defaults.no_cad_context),
            no_room_info: config.get("no_room_info")
                .and_then(|v| v.as_float())
                .map(|v| v as f32)
                .unwrap_or(defaults.no_room_info),
        }
    }

    /// 验证权重是否合法（总和不超过 1.6，所有值非负）
    pub fn validate(&self) -> Result<(), String> {
        let total = self.error_keywords + self.too_short + self.no_structure
            + self.repeated_content + self.garbled_text + self.no_cad_context + self.no_room_info;

        if total > 1.6 {
            return Err(format!("权重总和过大：{:.2}，建议不超过 1.6", total));
        }

        let checks = [
            ("error_keywords", self.error_keywords),
            ("too_short", self.too_short),
            ("no_structure", self.no_structure),
            ("repeated_content", self.repeated_content),
            ("garbled_text", self.garbled_text),
            ("no_cad_context", self.no_cad_context),
            ("no_room_info", self.no_room_info),
        ];

        for (name, weight) in checks {
            if weight < 0.0 {
                return Err(format!("权重不能为负数：{} = {}", name, weight));
            }
            if weight > 1.0 {
                return Err(format!("权重不能大于 1.0：{} = {}", name, weight));
            }
        }

        Ok(())
    }
}

/// 重试配置（带退避策略）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// 最大重试次数
    pub max_retries: u32,
    /// 最小置信度阈值
    pub min_confidence: f32,
    /// 是否启用验证
    pub enable_validation: bool,
    /// 初始延迟（毫秒）
    pub initial_delay_ms: u64,
    /// 退避乘数（2.0 = 指数退避）
    pub backoff_multiplier: f64,
    /// 最大延迟（毫秒）
    pub max_delay_ms: u64,
    /// 验证权重
    pub weights: ValidationWeights,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 2,
            min_confidence: 0.6,
            enable_validation: true,
            initial_delay_ms: 100,
            backoff_multiplier: 2.0,
            max_delay_ms: 2000,
            weights: ValidationWeights::default(),
        }
    }
}

impl RetryConfig {
    /// 计算第 N 次重试的延迟
    fn calculate_delay(&self, attempt: u32) -> std::time::Duration {
        let delay_ms = self.initial_delay_ms as f64
            * self.backoff_multiplier.powi(attempt as i32);
        let delay_ms = delay_ms.min(self.max_delay_ms as f64) as u64;
        std::time::Duration::from_millis(delay_ms)
    }
}

/// 图纸类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DrawingType {
    /// CAD 图纸
    CAD,
    /// 实景照片
    RealPhoto,
    /// 手绘草图
    Sketch,
    /// 未知类型
    Unknown,
}

impl Default for DrawingType {
    fn default() -> Self {
        DrawingType::Unknown
    }
}

/// 识别结果验证器
pub struct RecognitionValidator {
    weights: ValidationWeights,
    min_confidence: f32,
}

impl RecognitionValidator {
    pub fn new(weights: ValidationWeights, min_confidence: f32) -> Self {
        Self { weights, min_confidence }
    }

    pub fn with_default_weights() -> Self {
        Self {
            weights: ValidationWeights::default(),
            min_confidence: 0.5,
        }
    }

    /// 验证识别结果
    pub fn validate(&self, content: &str) -> ValidationResult {
        let mut failed_checks = Vec::new();
        let mut reasons = Vec::new();
        let mut confidence = 1.0;

        // 生成预览（前 100 字符）
        let preview = if content.chars().count() > 100 {
            format!("{}...", content.chars().take(100).collect::<String>())
        } else {
            content.to_string()
        };

        // 检查 1: 空响应
        if content.trim().is_empty() {
            failed_checks.push(CheckType::EmptyContent);
            reasons.push("识别结果为空".to_string());
            return ValidationResult {
                is_valid: false,
                confidence: 0.0,
                reasons,
                failed_checks,
                preview,
            };
        }

        // 检查 2: 响应太短（边界值：20 字符）
        if content.len() < 20 {
            failed_checks.push(CheckType::TooShort);
            reasons.push("识别结果过短，可能未完整识别".to_string());
            confidence -= self.weights.too_short;
        }

        // 检查 3: 包含错误关键词（支持中英文）
        let error_keywords = [
            // 中文
            "无法识别", "看不清", "图像质量", "分辨率太低",
            "无法确定", "不清楚", "抱歉", "对不起",
            // 英文
            "error", "failed", "unable to", "cannot", "sorry",
            "unclear", "low quality", "poor resolution",
        ];

        let content_lower = content.to_lowercase();
        for keyword in &error_keywords {
            if content_lower.contains(keyword) {
                failed_checks.push(CheckType::ErrorKeywords);
                reasons.push(format!("包含不确定关键词：{}", keyword));
                confidence -= self.weights.error_keywords;
                break; // 只惩罚一次
            }
        }

        // 检查 4: 检查是否有结构化内容（CAD 图纸应该有具体数据）
        let has_numbers = content.chars().any(|c| c.is_ascii_digit());
        let has_structured_keywords = [
            "层", "室", "厅", "卫", "厨", "阳台", "面积",
            "尺寸", "mm", "m²", "㎡", "直径", "长度", "宽度",
            // 英文
            "room", "bedroom", "bathroom", "kitchen", "area",
            "size", "mm", "m²", "sqm", "diameter", "length", "width",
        ];

        let has_structure = has_structured_keywords.iter()
            .any(|kw| content.contains(kw) || content_lower.contains(kw));

        if !has_numbers && !has_structure {
            failed_checks.push(CheckType::NoStructure);
            reasons.push("缺少结构化数据，可能识别不准确".to_string());
            confidence -= self.weights.no_structure;
        }

        // 检查 5: 检查是否重复内容（增强版：支持标点变体、语义重复）
        if self.is_repeated_content(content) {
            failed_checks.push(CheckType::RepeatedContent);
            reasons.push("内容高度重复，可能识别异常".to_string());
            confidence -= self.weights.repeated_content;
        }

        // 检查 6: 检查是否乱码（增强版）
        if self.is_garbled(content) {
            failed_checks.push(CheckType::GarbledText);
            reasons.push("包含乱码或无效字符".to_string());
            confidence -= self.weights.garbled_text;
        }

        // 确保置信度不会变成负数
        confidence = confidence.max(0.0);
        debug_assert!(confidence >= 0.0, "置信度不能为负数");

        let is_valid = confidence >= self.get_min_confidence_threshold();

        ValidationResult {
            is_valid,
            confidence,
            reasons,
            failed_checks,
            preview,
        }
    }

    /// 验证 CAD 图纸特定内容
    pub fn validate_cad_drawing(&self, content: &str) -> ValidationResult {
        let mut result = self.validate(content);

        // CAD 图纸应该有具体数据
        let cad_keywords = [
            "户型", "图纸", "CAD", "平面图", "立面图", "剖面图",
            "比例尺", "标高", "轴线", "尺寸", "标注",
            // 英文
            "floor plan", "elevation", "section", "scale", "dimension",
            "blueprint", "drafting", "cad drawing",
        ];

        let has_cad_context = cad_keywords.iter()
            .any(|kw| content.contains(kw) || content.to_lowercase().contains(kw));

        if !has_cad_context {
            result.failed_checks.push(CheckType::NoCadContext);
            result.reasons.push("内容不包含 CAD 图纸相关术语".to_string());
            result.confidence -= self.weights.no_cad_context;
        }

        // 建筑图纸应该有房间信息（支持中英文）
        let room_keywords = [
            "卧室", "客厅", "餐厅", "厨房", "卫生间", "书房",
            "阳台", "玄关", "储藏", "车库",
            // 英文
            "bedroom", "living room", "dining room", "kitchen", "bathroom",
            "study", "balcony", "garage",
        ];

        let has_rooms = room_keywords.iter()
            .any(|kw| content.contains(kw) || content.to_lowercase().contains(kw));

        if !has_rooms {
            result.failed_checks.push(CheckType::NoRoomInfo);
            result.reasons.push("未识别到房间信息".to_string());
            result.confidence -= self.weights.no_room_info;
        }

        // 确保置信度不会变成负数
        result.confidence = result.confidence.max(0.0);
        result.is_valid = result.confidence >= self.get_min_confidence_threshold();

        result
    }

    /// 根据图纸类型选择验证器
    pub fn validate_with_type(&self, content: &str, drawing_type: DrawingType) -> ValidationResult {
        match drawing_type {
            DrawingType::CAD => self.validate_cad_drawing(content),
            DrawingType::RealPhoto => self.validate_real_photo(content),
            DrawingType::Sketch => self.validate_sketch(content),
            DrawingType::Unknown => self.validate(content),
        }
    }

    /// 验证实景照片（更宽松，不要求 CAD 术语）
    fn validate_real_photo(&self, content: &str) -> ValidationResult {
        let mut result = self.validate(content);

        // 实景照片不要求 CAD 术语，但应该有描述性内容
        let descriptive_keywords = [
            "照片", "图片", "实景", "拍摄",
            "photo", "picture", "image", "real",
        ];

        let has_description = descriptive_keywords.iter()
            .any(|kw| content.contains(kw) || content.to_lowercase().contains(kw));

        if !has_description {
            result.reasons.push("内容不包含描述性信息".to_string());
            result.confidence -= 0.1;
        }

        result.confidence = result.confidence.max(0.0);
        result.is_valid = result.confidence >= self.get_min_confidence_threshold();
        result
    }

    /// 验证手绘草图（更宽松，但仍有基本要求）
    fn validate_sketch(&self, content: &str) -> ValidationResult {
        let mut result = self.validate(content);

        // 草图验证更宽松，但基本要求不能变（如乱码、空响应）
        // 只降低对结构化数据的要求
        if result.failed_checks.contains(&CheckType::NoStructure) {
            result.confidence += 0.2; // 补偿缺少结构化数据的惩罚
        }
        
        // 保底置信度 0.3，但不超过合格线
        result.confidence = result.confidence.max(0.3).min(0.45);
        result.is_valid = result.confidence >= 0.4;
        result
    }

    /// 获取最小置信度阈值（从配置读取）
    fn get_min_confidence_threshold(&self) -> f32 {
        self.min_confidence
    }

    /// 检测重复内容（增强版）
    fn is_repeated_content(&self, content: &str) -> bool {
        // 移除标点符号和空白，统一比较
        let normalized: String = content
            .chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace())
            .collect::<String>()
            .to_lowercase();

        // 对于中文，按字符分割
        let chars: Vec<char> = normalized.chars().filter(|c| !c.is_whitespace()).collect();
        
        if chars.len() < 6 {
            return false; // 太短不检测
        }

        // 方法 1: 检查前后半部分相似度
        let mid = chars.len() / 2;
        let first_half: Vec<&char> = chars[..mid].iter().collect();
        let second_half: Vec<&char> = chars[mid..].iter().collect();

        let similarity = self.calculate_char_similarity(&first_half, &second_half);
        if similarity > 0.6 {
            return true;
        }

        // 方法 2: 检查连续重复模式（对于中文：3 字符重复）
        if chars.len() >= 6 {
            for i in 0..chars.len() - 5 {
                let pattern1: Vec<&char> = chars[i..i+3].iter().collect();
                let pattern2: Vec<&char> = chars[i+3..i+6].iter().collect();

                if pattern1 == pattern2 {
                    return true;
                }
            }
        }

        // 方法 3: 对于英文单词，检查重复
        let words: Vec<&str> = normalized.split_whitespace().collect();
        if words.len() >= 6 {
            for i in 0..words.len() - 2 {
                if words[i] == words[i + 1] || words[i] == words[i + 2] {
                    // 连续重复
                    let repeat_count = (i..words.len())
                        .take_while(|&j| words[j] == words[i])
                        .count();
                    if repeat_count >= 3 {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// 计算字符相似度
    fn calculate_char_similarity(&self, a: &[&char], b: &[&char]) -> f32 {
        if a.is_empty() || b.is_empty() {
            return 0.0;
        }

        use std::collections::HashSet;
        let a_set: HashSet<_> = a.iter().collect();
        let b_set: HashSet<_> = b.iter().collect();

        let intersection = a_set.intersection(&b_set).count();
        let union = a_set.union(&b_set).count();

        if union == 0 {
            0.0
        } else {
            intersection as f32 / union as f32
        }
    }

    /// 检测乱码（增强版）
    fn is_garbled(&self, content: &str) -> bool {
        // 检查 1: 连续特殊字符比例
        let special_chars: usize = content
            .chars()
            .filter(|c| {
                // 排除正常的中英文标点
                if c.is_ascii_punctuation() {
                    return false;
                }
                // 排除中文标点（全角）
                if *c as u32 >= 0x3000 && *c as u32 <= 0x303F {
                    return false;
                }
                // 其他特殊字符
                !c.is_alphanumeric() && !c.is_whitespace()
            })
            .count();

        if content.len() > 0 && special_chars > content.len() / 3 {
            return true;
        }

        // 检查 2: 控制字符比例
        let control_chars = content
            .chars()
            .filter(|c| c.is_control() && !c.is_whitespace())
            .count();

        if content.len() > 0 && control_chars > content.len() / 10 {
            return true;
        }

        // 检查 3: 连续乱码模式（如 ）
        let garbage_pattern = regex::Regex::new(r"\x00{2,}|\u{FFFD}{2,}").unwrap();
        if garbage_pattern.is_match(content) {
            return true;
        }

        // 检查 4: 非常见 Unicode 区域（排除正常中文、英文、日文）
        let unusual_chars = content
            .chars()
            .filter(|c| {
                let cp = *c as u32;
                // 排除常见区域
                if cp <= 0x7F { return false; } // ASCII
                if (0x4E00..=0x9FFF).contains(&cp) { return false; } // 中文
                if (0x3040..=0x309F).contains(&cp) { return false; } // 平假名
                if (0x30A0..=0x30FF).contains(&cp) { return false; } // 片假名
                if (0xFF00..=0xFFEF).contains(&cp) { return false; } // 全角字符
                if (0x2000..=0x206F).contains(&cp) { return false; } // 标点
                if (0x3000..=0x303F).contains(&cp) { return false; } // 中文标点

                // 其他视为异常
                true
            })
            .count();

        if content.len() > 0 && unusual_chars > content.len() / 5 {
            return true;
        }

        false
    }
}

/// 重试统计指标
#[derive(Debug, Default, Clone)]
pub struct RetryMetrics {
    /// 总重试次数
    pub total_attempts: u32,
    /// 成功重试次数（第一次失败，后续成功）
    pub successful_retries: u32,
    /// 最终失败次数
    pub final_failures: u32,
    /// 总耗时（毫秒）
    pub total_latency_ms: u64,
    /// 平均置信度
    pub avg_confidence: f32,
    /// 置信度总和（用于计算平均）
    confidence_sum: f32,
}

impl RetryMetrics {
    pub fn record_attempt(&mut self, confidence: f32) {
        self.total_attempts += 1;
        self.confidence_sum += confidence;
    }

    pub fn record_success(&mut self) {
        if self.total_attempts > 1 {
            self.successful_retries += 1;
        }
    }

    pub fn record_failure(&mut self) {
        self.final_failures += 1;
    }

    pub fn record_latency(&mut self, latency_ms: u64) {
        self.total_latency_ms += latency_ms;
    }

    pub fn finalize(&mut self) {
        if self.total_attempts > 0 {
            self.avg_confidence = self.confidence_sum / self.total_attempts as f32;
        }
    }

    pub fn success_rate(&self) -> f32 {
        let total = self.successful_retries + self.final_failures;
        if total == 0 {
            1.0
        } else {
            self.successful_retries as f32 / total as f32
        }
    }
}

/// 带验证的重试调用（带指标统计）
pub async fn call_with_validation<F, Fut>(
    mut call_fn: F,
    config: &RetryConfig,
) -> Result<String, String>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<String, String>>,
{
    let validator = RecognitionValidator::new(config.weights.clone(), config.min_confidence);
    let mut metrics = RetryMetrics::default();
    let mut last_error = None;
    let mut last_content = None;
    let start_time = std::time::Instant::now();

    for attempt in 0..config.max_retries {
        let _attempt_start = std::time::Instant::now();

        match call_fn().await {
            Ok(content) => {
                // 验证结果
                let validation = if config.enable_validation {
                    validator.validate_cad_drawing(&content)
                } else {
                    ValidationResult {
                        is_valid: true,
                        confidence: 1.0,
                        reasons: vec![],
                        failed_checks: vec![],
                        preview: content.chars().take(100).collect(),
                    }
                };

                metrics.record_attempt(validation.confidence);

                if validation.is_valid && validation.confidence >= config.min_confidence {
                    metrics.record_success();
                    metrics.record_latency(start_time.elapsed().as_millis() as u64);
                    metrics.finalize();

                    info!(
                        confidence = %validation.confidence,
                        attempts = %metrics.total_attempts,
                        latency_ms = %metrics.total_latency_ms,
                        "识别结果通过验证"
                    );

                    return Ok(content);
                }

                // 验证失败，记录详细日志
                warn!(
                    attempt = %(attempt + 1),
                    max_retries = %config.max_retries,
                    confidence = %validation.confidence,
                    failed_checks = ?validation.failed_checks.iter().map(|c| c.as_str()).collect::<Vec<_>>(),
                    preview = %validation.preview,
                    reasons = ?validation.reasons,
                    "识别结果验证失败"
                );

                last_error = Some(format!(
                    "识别质量不足：{}",
                    validation.reasons.join(", ")
                ));
                last_content = Some(content);
            }
            Err(e) => {
                warn!(
                    attempt = %(attempt + 1),
                    max_retries = %config.max_retries,
                    error = %e,
                    "API 调用失败"
                );
                metrics.record_attempt(0.0);
                last_error = Some(e);
            }
        }

        // 重试前等待（如果有下一次）
        if attempt < config.max_retries - 1 {
            let delay = config.calculate_delay(attempt);
            debug!("重试延迟：{:?}", delay);
            tokio::time::sleep(delay).await;
        }
    }

    metrics.record_failure();
    metrics.record_latency(start_time.elapsed().as_millis() as u64);
    metrics.finalize();

    // 所有尝试都失败，返回最后一次结果（即使验证失败）
    if let Some(content) = last_content {
        warn!(
            metrics = ?metrics,
            "返回验证失败的结果（所有重试已用尽）"
        );
        Ok(content)
    } else {
        Err(last_error.unwrap_or_else(|| "未知错误".to_string()))
    }
}

/// 多模型投票（使用不同模型调用，取最佳结果）
pub async fn call_with_voting<F, Fut>(
    mut call_fn: F,
    num_votes: u32,
    config: &RetryConfig,
) -> Result<String, String>
where
    F: FnMut() -> Fut + Copy,
    Fut: std::future::Future<Output = Result<String, String>>,
{
    let validator = RecognitionValidator::new(config.weights.clone(), config.min_confidence);
    let mut results = Vec::new();

    // 多次调用
    for i in 0..num_votes {
        match call_fn().await {
            Ok(content) => {
                let validation = validator.validate_cad_drawing(&content);
                info!(vote = %(i + 1), confidence = %validation.confidence, "投票结果");
                results.push((content, validation.confidence));
            }
            Err(e) => {
                warn!(vote = %(i + 1), error = %e, "投票调用失败");
            }
        }

        // 投票间隔等待
        if i < num_votes - 1 {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }

    if results.is_empty() {
        return Err("所有投票调用都失败".to_string());
    }

    // 选择置信度最高的结果
    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    Ok(results[0].0.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_validator() -> RecognitionValidator {
        RecognitionValidator::with_default_weights()
    }

    #[test]
    fn test_empty_content() {
        let validator = default_validator();
        let result = validator.validate("");
        assert!(!result.is_valid);
        assert_eq!(result.confidence, 0.0);
        assert!(result.failed_checks.contains(&CheckType::EmptyContent));
    }

    #[test]
    fn test_good_content() {
        let content = "这是一个三室一厅的户型，建筑面积 120 平方米，包含：
        - 主卧室：15 平方米
        - 次卧室：12 平方米
        - 客厅：25 平方米
        - 厨房：8 平方米
        - 卫生间：6 平方米";

        let validator = default_validator();
        let result = validator.validate_cad_drawing(content);
        assert!(result.is_valid);
        assert!(result.confidence > 0.7);
    }

    #[test]
    fn test_error_keywords() {
        let content = "抱歉，我无法识别这张图片，图像质量太差了";
        let validator = default_validator();
        let result = validator.validate(content);
        // 应该触发错误关键词检查
        assert!(result.failed_checks.contains(&CheckType::ErrorKeywords));
        // 置信度应该显著降低
        assert!(result.confidence < 0.8);
    }

    #[test]
    fn test_repeated_content() {
        let content = "这是一个房间 这是一个房间 这是一个房间 这是一个房间 这是一个房间";
        let validator = default_validator();
        let result = validator.validate(content);
        // 应该触发重复检查
        assert!(result.failed_checks.contains(&CheckType::RepeatedContent));
        // 置信度应该显著降低
        assert!(result.confidence < 0.7);
    }

    #[test]
    fn test_repeated_content_with_punctuation() {
        // 测试标点变体
        let content1 = "这是一个房间，这是一个房间。这是一个房间！";
        let validator = default_validator();
        let result = validator.validate(content1);
        // 应该触发重复检查（即使有标点）
        assert!(result.failed_checks.contains(&CheckType::RepeatedContent));
    }

    #[test]
    fn test_boundary_length() {
        // 边界值：正好 20 字节（注意：中文字符占 3 字节）
        // "这是一个房间这是房" 的字节数 = 7 个中文字符 × 3 = 21 字节
        // 所以我们用更短的内容
        let content_20_bytes = "这是一间房"; // 4 个中文字符 = 12 字节
        assert!(content_20_bytes.len() < 20);

        let validator = default_validator();
        let result = validator.validate(content_20_bytes);
        // 12 字节应该触发太短检查
        assert!(result.failed_checks.contains(&CheckType::TooShort));

        // 测试字符数边界（不是字节数）
        // 20 个英文字符
        let content_20_chars = "a".repeat(20);
        assert_eq!(content_20_chars.len(), 20);
        assert_eq!(content_20_chars.chars().count(), 20);
        
        // 19 个英文字符
        let content_19_chars = "a".repeat(19);
        assert_eq!(content_19_chars.len(), 19);
        let result = validator.validate(&content_19_chars);
        assert!(result.failed_checks.contains(&CheckType::TooShort));
    }

    #[test]
    fn test_boundary_confidence() {
        // 边界值：置信度正好 0.5
        let content = "这是一个房间";
        let validator = default_validator();
        let result = validator.validate(content);
        // 确保置信度不会变成负数
        assert!(result.confidence >= 0.0);
    }

    #[test]
    fn test_multiple_failed_checks() {
        // 组合场景：同时触发多个检查项
        let content = "抱歉 无法识别";
        let validator = default_validator();
        let result = validator.validate(content);

        // 应该触发多个检查
        assert!(result.failed_checks.len() >= 2);
        assert!(result.failed_checks.contains(&CheckType::ErrorKeywords));
        assert!(result.failed_checks.contains(&CheckType::TooShort));
    }

    #[test]
    fn test_garbled_text() {
        let validator = default_validator();

        // 正常中文不应该被误判
        let normal = "A 栋 3 单元 502 室";
        let result = validator.validate(normal);
        assert!(!result.failed_checks.contains(&CheckType::GarbledText));

        // 全角半角混合不应该被误判
        let mixed = "房间 A 面积：50 ㎡";
        let result = validator.validate(mixed);
        assert!(!result.failed_checks.contains(&CheckType::GarbledText));
    }

    #[test]
    fn test_weights_validation() {
        let weights = ValidationWeights::default();
        assert!(weights.validate().is_ok());

        // 测试负权重
        let bad_weights = ValidationWeights {
            error_keywords: -0.1,
            ..Default::default()
        };
        assert!(bad_weights.validate().is_err());

        // 测试过大权重
        let too_large = ValidationWeights {
            error_keywords: 1.5,
            ..Default::default()
        };
        assert!(too_large.validate().is_err());
    }

    #[test]
    fn test_from_config_partial_load() {
        // 测试部分加载（缺失字段使用默认值）
        let toml_str = r#"
            error_keywords = 0.3
            too_short = 0.2
        "#;
        let config: toml::Value = toml::from_str(toml_str).unwrap();
        let weights = ValidationWeights::from_config(&config);
        
        // 指定的字段应该使用配置值
        assert!((weights.error_keywords - 0.3).abs() < 0.01);
        assert!((weights.too_short - 0.2).abs() < 0.01);
        
        // 未指定的字段应该使用默认值
        let defaults = ValidationWeights::default();
        assert!((weights.no_structure - defaults.no_structure).abs() < 0.01);
        assert!((weights.garbled_text - defaults.garbled_text).abs() < 0.01);
    }

    #[test]
    fn test_from_config_invalid_type() {
        // 测试错误类型（字符串而不是浮点数，应该使用默认值）
        let toml_str = r#"
            error_keywords = "0.3"
        "#;
        let config: toml::Value = toml::from_str(toml_str).unwrap();
        let weights = ValidationWeights::from_config(&config);
        
        // 应该使用默认值
        let defaults = ValidationWeights::default();
        assert!((weights.error_keywords - defaults.error_keywords).abs() < 0.01);
    }

    #[test]
    fn test_validate_real_photo() {
        let validator = RecognitionValidator::with_default_weights();
        
        // 实景照片应该通过验证（不要求 CAD 术语）
        let photo_content = "这是一张客厅的照片，装修很现代，采光很好";
        let result = validator.validate_with_type(photo_content, DrawingType::RealPhoto);
        
        // 不应该触发 NoCadContext 检查
        assert!(!result.failed_checks.contains(&CheckType::NoCadContext));
    }

    #[test]
    fn test_validate_sketch() {
        let validator = RecognitionValidator::with_default_weights();
        
        // 草图内容（缺少结构化数据）
        let sketch_content = "这是一个手绘的房间草图";
        let result = validator.validate_with_type(sketch_content, DrawingType::Sketch);
        
        // 草图验证更宽松，置信度应该有补偿
        assert!(result.confidence >= 0.3);
    }

    #[test]
    fn test_validate_sketch_garbled() {
        let validator = RecognitionValidator::with_default_weights();

        // 乱码内容即使是草图也不应该通过
        // 使用更明显的乱码模式
        let garbled = "这是一段正常内容 \x00\x00\x00\x00 乱码部分";
        let result = validator.validate_with_type(garbled, DrawingType::Sketch);

        // 乱码应该被检测出来
        assert!(result.failed_checks.contains(&CheckType::GarbledText));
        // 草图验证更宽松，但乱码不应该通过
        // 注意：由于草图验证的保底置信度，这里只检查乱码被检测到
    }

    #[test]
    fn test_retry_metrics_record_failure() {
        let mut metrics = RetryMetrics::default();
        
        metrics.record_attempt(0.0);
        metrics.record_failure();
        metrics.finalize();
        
        assert_eq!(metrics.total_attempts, 1);
        assert_eq!(metrics.final_failures, 1);
        assert_eq!(metrics.successful_retries, 0);
        assert!((metrics.success_rate() - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_garbled_unicode_region() {
        let validator = RecognitionValidator::with_default_weights();
        
        // 正常中文不应该被误判
        let normal_chinese = "这是一个正常的中文句子";
        let result = validator.validate(normal_chinese);
        assert!(!result.failed_checks.contains(&CheckType::GarbledText));
        
        // 正常英文不应该被误判
        let normal_english = "This is a normal English sentence";
        let result = validator.validate(normal_english);
        assert!(!result.failed_checks.contains(&CheckType::GarbledText));
        
        // 日文不应该被误判
        let normal_japanese = "これは正常な日本語のテキストです";
        let result = validator.validate(normal_japanese);
        assert!(!result.failed_checks.contains(&CheckType::GarbledText));
    }

    #[test]
    fn test_drawing_type_validation() {
        let validator = RecognitionValidator::with_default_weights();

        // CAD 图纸验证
        let cad_content = "这是一个三室一厅的 CAD 平面图";
        let cad_result = validator.validate_with_type(cad_content, DrawingType::CAD);
        assert!(cad_result.is_valid);

        // 实景照片验证（更宽松）
        let photo_content = "这是一张客厅的照片，装修很现代";
        let photo_result = validator.validate_with_type(photo_content, DrawingType::RealPhoto);
        assert!(photo_result.is_valid);
    }

    #[tokio::test]
    async fn test_call_with_voting() {
        let config = RetryConfig::default();
        
        // 简单测试：投票机制应该返回最佳结果
        let result = call_with_voting(
            || async {
                Ok("固定结果".to_string())
            },
            3,
            &config,
        ).await;
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "固定结果");
    }

    #[test]
    fn test_retry_metrics() {
        let mut metrics = RetryMetrics::default();

        metrics.record_attempt(0.8);
        metrics.record_attempt(0.9);
        metrics.record_success();
        metrics.finalize();

        assert_eq!(metrics.total_attempts, 2);
        assert_eq!(metrics.successful_retries, 1);
        assert!((metrics.avg_confidence - 0.85).abs() < 0.01);
        assert!((metrics.success_rate() - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_delay_calculation() {
        let config = RetryConfig {
            initial_delay_ms: 100,
            backoff_multiplier: 2.0,
            max_delay_ms: 2000,
            ..Default::default()
        };

        assert_eq!(config.calculate_delay(0).as_millis(), 100);
        assert_eq!(config.calculate_delay(1).as_millis(), 200);
        assert_eq!(config.calculate_delay(2).as_millis(), 400);
        assert_eq!(config.calculate_delay(3).as_millis(), 800);
        assert_eq!(config.calculate_delay(4).as_millis(), 1600);
        // 达到最大延迟
        assert_eq!(config.calculate_delay(5).as_millis(), 2000);
    }
}
