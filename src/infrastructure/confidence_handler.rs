//! 置信度阈值处理模块
//!
//! 提供低置信度时请求人工确认的机制
//!
//! # 使用场景
//!
//! 当模板分类或 OCR 识别的置信度低于配置的阈值时：
//! 1. 记录警告日志
//! 2. 返回低置信度标记
//! 3. 可选：触发人工确认流程
//!
//! # 人工确认方式
//!
//! 1. **CLI 模式**：提示用户确认或手动输入类型
//! 2. **Web API 模式**：返回低置信度标记，前端提示用户确认
//! 3. **批量处理模式**：将低置信度文件标记为待审核

use crate::domain::service::template_selection::CulvertDrawingType;
use tracing::{warn, info};

/// 置信度级别
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfidenceLevel {
    /// 高置信度（>= 0.8）
    High,
    /// 中置信度（0.6 - 0.8）
    Medium,
    /// 低置信度（< 0.6）
    Low,
}

impl ConfidenceLevel {
    /// 从置信度分数计算级别
    pub fn from_score(score: f32) -> Self {
        if score >= 0.8 {
            ConfidenceLevel::High
        } else if score >= 0.6 {
            ConfidenceLevel::Medium
        } else {
            ConfidenceLevel::Low
        }
    }

    /// 是否需要人工确认
    pub fn needs_manual_review(&self) -> bool {
        matches!(self, ConfidenceLevel::Low)
    }
}

/// 置信度阈值配置
#[derive(Debug, Clone)]
pub struct ConfidenceThresholdConfig {
    /// 高置信度阈值（>= 此值为高置信度）
    pub high_threshold: f32,
    /// 低置信度阈值（< 此值为低置信度）
    pub low_threshold: f32,
    /// 是否启用人工确认
    pub enable_manual_review: bool,
    /// 是否记录日志
    pub enable_logging: bool,
}

impl Default for ConfidenceThresholdConfig {
    fn default() -> Self {
        Self {
            high_threshold: 0.8,
            low_threshold: 0.6,
            enable_manual_review: true,
            enable_logging: true,
        }
    }
}

/// 置信度评估结果
#[derive(Debug, Clone)]
pub struct ConfidenceResult {
    /// 分类结果
    pub template_type: CulvertDrawingType,
    /// 置信度分数（0.0 - 1.0）
    pub confidence_score: f32,
    /// 置信度级别
    pub confidence_level: ConfidenceLevel,
    /// 是否需要人工确认
    pub needs_review: bool,
    /// 建议操作
    pub suggestion: Option<String>,
}

impl ConfidenceResult {
    /// 创建新的置信度评估结果
    pub fn new(
        template_type: CulvertDrawingType,
        confidence_score: f32,
        config: &ConfidenceThresholdConfig,
    ) -> Self {
        let confidence_level = ConfidenceLevel::from_score(confidence_score);
        let needs_review = config.enable_manual_review && confidence_level.needs_manual_review();

        let suggestion = if needs_review {
            Some(format!(
                "置信度较低 ({:.2})，建议人工确认分类结果",
                confidence_score
            ))
        } else if confidence_level == ConfidenceLevel::High {
            Some("置信度高，可自动处理".to_string())
        } else {
            None
        };

        Self {
            template_type,
            confidence_score,
            confidence_level,
            needs_review,
            suggestion,
        }
    }

    /// 判断是否可信（高或中置信度）
    pub fn is_acceptable(&self) -> bool {
        self.confidence_level != ConfidenceLevel::Low
    }
}

/// 置信度阈值处理器
pub struct ConfidenceThresholdHandler {
    config: ConfidenceThresholdConfig,
}

impl ConfidenceThresholdHandler {
    /// 创建新的处理器
    pub fn new(config: ConfidenceThresholdConfig) -> Self {
        Self { config }
    }

    /// 创建默认处理器
    pub fn with_defaults() -> Self {
        Self::new(ConfidenceThresholdConfig::default())
    }

    /// 评估置信度
    pub fn evaluate(
        &self,
        template_type: CulvertDrawingType,
        confidence_score: f32,
    ) -> ConfidenceResult {
        let result = ConfidenceResult::new(template_type, confidence_score, &self.config);

        // 记录日志
        if self.config.enable_logging {
            self.log_confidence_result(&result);
        }

        result
    }

    /// 检查是否需要人工确认
    pub fn needs_manual_review(&self, confidence_score: f32) -> bool {
        confidence_score < self.config.low_threshold && self.config.enable_manual_review
    }

    /// 记录置信度结果
    fn log_confidence_result(&self, result: &ConfidenceResult) {
        let level_str = match result.confidence_level {
            ConfidenceLevel::High => "高",
            ConfidenceLevel::Medium => "中",
            ConfidenceLevel::Low => "低",
        };

        if result.needs_review {
            warn!(
                "模板分类置信度{}：{:?} (分数：{:.2}) - 需要人工确认",
                level_str, result.template_type, result.confidence_score
            );
        } else if self.config.enable_logging {
            info!(
                "模板分类置信度{}：{:?} (分数：{:.2})",
                level_str, result.template_type, result.confidence_score
            );
        }
    }

    /// 批量评估置信度
    pub fn evaluate_batch(
        &self,
        results: &[(CulvertDrawingType, f32)],
    ) -> Vec<ConfidenceResult> {
        results
            .iter()
            .map(|(template_type, confidence)| {
                self.evaluate(template_type.clone(), *confidence)
            })
            .collect()
    }

    /// 筛选出需要人工审核的结果
    pub fn filter_needs_review<'a>(
        &'a self,
        results: &'a [ConfidenceResult],
    ) -> Vec<&'a ConfidenceResult> {
        results.iter().filter(|r| r.needs_review).collect()
    }
}

/// CLI 模式下的人工确认辅助函数
///
/// 在 CLI 中提示用户确认或手动输入类型
pub fn prompt_manual_confirmation(
    template_type: &CulvertDrawingType,
    confidence: f32,
) -> Result<CulvertDrawingType, Box<dyn std::error::Error>> {
    use std::io::{self, Write};

    println!("\n⚠️  分类置信度较低 ({:.2})，请确认：", confidence);
    println!("   自动分类结果：{}", template_type.as_str());
    println!("\n选项：");
    println!("   1. 接受当前分类 (按 Enter)");
    println!("   2. 手动选择类型 (输入类型编号)");
    println!("   3. 跳过此文件 (输入 's')");

    // 显示所有类型
    let all_types = CulvertDrawingType::get_all_types();
    println!("\n可用类型:");
    for (i, t) in all_types.iter().enumerate() {
        println!("   {}. {}", i + 1, t.as_str());
    }

    print!("\n请选择：");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim();

    if input.is_empty() {
        // 接受当前分类
        Ok(template_type.clone())
    } else if input.to_lowercase() == "s" {
        // 跳过
        Err("用户选择跳过".into())
    } else if let Ok(index) = input.parse::<usize>() {
        if index >= 1 && index <= all_types.len() {
            Ok(all_types[index - 1].clone())
        } else {
            Err(format!("无效的类型编号：{}", index).into())
        }
    } else {
        // 尝试通过名称匹配
        for t in all_types {
            if t.as_str().contains(input) {
                return Ok(t.clone());
            }
        }
        Err(format!("无法识别的类型：{}", input).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_confidence_level() {
        assert_eq!(ConfidenceLevel::from_score(0.9), ConfidenceLevel::High);
        assert_eq!(ConfidenceLevel::from_score(0.7), ConfidenceLevel::Medium);
        assert_eq!(ConfidenceLevel::from_score(0.4), ConfidenceLevel::Low);
    }

    #[test]
    fn test_needs_manual_review() {
        assert!(ConfidenceLevel::Low.needs_manual_review());
        assert!(!ConfidenceLevel::Medium.needs_manual_review());
        assert!(!ConfidenceLevel::High.needs_manual_review());
    }

    #[test]
    fn test_confidence_result() {
        let config = ConfidenceThresholdConfig::default();
        let result = ConfidenceResult::new(
            CulvertDrawingType::CulvertLayout,
            0.5,
            &config,
        );

        assert_eq!(result.confidence_level, ConfidenceLevel::Low);
        assert!(result.needs_review);
        assert!(result.suggestion.is_some());
    }

    #[test]
    fn test_handler_evaluate() {
        let handler = ConfidenceThresholdHandler::with_defaults();

        // 高置信度
        let result = handler.evaluate(CulvertDrawingType::CulvertLayout, 0.9);
        assert!(!result.needs_review);
        assert!(result.is_acceptable());

        // 低置信度
        let result = handler.evaluate(CulvertDrawingType::CulvertLayout, 0.4);
        assert!(result.needs_review);
        assert!(!result.is_acceptable());
    }
}
