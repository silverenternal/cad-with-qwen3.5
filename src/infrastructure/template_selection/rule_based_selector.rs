//! 基于规则的模板选择器
//!
//! 通过关键词匹配规则来选择模板类型
//!
//! # 使用方式
//!
//! ## 从 OCR 文本选择（需要外部 OCR 集成）
//! ```ignore
//! let selector = RuleBasedTemplateSelector::new(config);
//! let ocr_text = call_external_ocr_service(image_data).await?; // 调用外部 OCR
//! let (template_type, confidence) = selector.select_from_ocr_text(&ocr_text);
//! ```
//!
//! ## 多模态模型直接分类（推荐）
//! ```ignore
//! // 使用 MultimodalTemplateClassifier 进行分类
//! use crate::infrastructure::template_selection::MultimodalTemplateClassifier;
//! let classifier = MultimodalTemplateClassifier::new(config, api_client);
//! let result = classifier.classify(image_data).await?;
//! ```

use crate::domain::service::CulvertType;
use std::collections::HashMap;

/// 模板选择器配置
#[derive(Debug, Clone)]
pub struct TemplateSelectorConfig {
    /// 置信度阈值（0.0-1.0），低于此值需要人工确认
    pub confidence_threshold: f32,
    /// 是否启用日志
    pub enable_logging: bool,
}

impl Default for TemplateSelectorConfig {
    fn default() -> Self {
        Self {
            confidence_threshold: 0.6,
            enable_logging: true,
        }
    }
}

/// 基于规则的模板选择器
pub struct RuleBasedTemplateSelector {
    config: TemplateSelectorConfig,
    /// 关键词到模板类型的映射规则
    keyword_rules: HashMap<&'static str, CulvertType>,
    /// 权重规则（某些关键词权重更高）
    weight_rules: HashMap<&'static str, f32>,
}

impl RuleBasedTemplateSelector {
    /// 创建新的模板选择器
    pub fn new(config: TemplateSelectorConfig) -> Self {
        let mut selector = Self {
            config,
            keyword_rules: HashMap::new(),
            weight_rules: HashMap::new(),
        };
        selector.init_rules();
        selector
    }

    /// 获取配置
    pub fn config(&self) -> &TemplateSelectorConfig {
        &self.config
    }

    /// 初始化关键词规则
    fn init_rules(&mut self) {
        // 表格类
        self.add_rule("一览表", CulvertType::CulvertSettingTable, 1.0);
        self.add_rule("统计表", CulvertType::CulvertSettingTable, 0.9);
        self.add_rule("汇总", CulvertType::CulvertSettingTable, 0.8);

        self.add_rule("工程数量", CulvertType::CulvertQuantityTable, 1.0);
        self.add_rule("混凝土", CulvertType::CulvertQuantityTable, 0.7);
        self.add_rule("钢筋", CulvertType::CulvertQuantityTable, 0.6);
        self.add_rule("数量表", CulvertType::CulvertQuantityTable, 0.9);

        // 布置图类
        self.add_rule("布置图", CulvertType::CulvertLayout, 1.0);
        self.add_rule("立面图", CulvertType::CulvertLayout, 0.8);
        self.add_rule("平面图", CulvertType::CulvertLayout, 0.8);
        self.add_rule("横断面", CulvertType::CulvertLayout, 0.7);

        self.add_rule("暗涵", CulvertType::DarkCulvertLayout, 0.9);
        self.add_rule("分离式", CulvertType::DarkCulvertLayout, 0.8);

        // 钢筋构造图类（按孔径）
        // 注意：孔径关键词权重更高，应该先匹配
        self.add_rule("2m 孔径", CulvertType::BoxCulvertReinforcement2m, 1.0);
        self.add_rule("2m", CulvertType::BoxCulvertReinforcement2m, 0.8);
        self.add_rule("2 米", CulvertType::BoxCulvertReinforcement2m, 0.8);
        self.add_rule("孔径 2", CulvertType::BoxCulvertReinforcement2m, 0.9);

        self.add_rule("3m 孔径", CulvertType::BoxCulvertReinforcement3m, 1.0);
        self.add_rule("3m", CulvertType::BoxCulvertReinforcement3m, 0.8);
        self.add_rule("3 米", CulvertType::BoxCulvertReinforcement3m, 0.8);
        self.add_rule("孔径 3", CulvertType::BoxCulvertReinforcement3m, 0.9);

        self.add_rule("4m 孔径", CulvertType::BoxCulvertReinforcement4m, 1.0);
        self.add_rule("4m", CulvertType::BoxCulvertReinforcement4m, 0.8);
        self.add_rule("4 米", CulvertType::BoxCulvertReinforcement4m, 0.8);
        self.add_rule("孔径 4", CulvertType::BoxCulvertReinforcement4m, 0.9);

        // 通用关键词（权重较低，作为补充）
        self.add_rule("钢筋构造", CulvertType::BoxCulvertReinforcement2m, 0.5);
        self.add_rule("涵身钢筋", CulvertType::BoxCulvertReinforcement2m, 0.5);

        // 斜涵类（权重较高，优先匹配）
        self.add_rule("斜涵斜布", CulvertType::SkewedReinforcementCombination, 1.0);
        self.add_rule("斜涵", CulvertType::SkewedBoxCulvertReinforcement2m, 0.9);
        self.add_rule("30°斜度", CulvertType::SkewedBoxCulvertReinforcement2m, 1.0);
        self.add_rule("30°", CulvertType::SkewedBoxCulvertReinforcement2m, 0.7);
        self.add_rule("斜度", CulvertType::SkewedBoxCulvertReinforcement2m, 0.6);

        // 细部构造类
        self.add_rule("防水", CulvertType::JointWaterproofing, 0.9);
        self.add_rule("止水带", CulvertType::WaterStopInstallation, 1.0);
        self.add_rule("接缝", CulvertType::JointWaterproofing, 0.8);

        self.add_rule("帽石", CulvertType::CapStoneReinforcement, 0.9);
        self.add_rule("涵长调整", CulvertType::CulvertLengthAdjustment, 0.9);

        self.add_rule("基础钢筋网", CulvertType::FoundationReinforcementPlan, 0.8);
        self.add_rule("钢筋网平面", CulvertType::FoundationReinforcementPlan, 0.9);
        self.add_rule("钢筋网侧面", CulvertType::FoundationReinforcementSide, 0.9);

        // 方案图类（序号关键词权重最高）
        self.add_rule("（一）", CulvertType::CulvertLengthAdjustment1, 1.0);
        self.add_rule("(一)", CulvertType::CulvertLengthAdjustment1, 1.0);
        self.add_rule("（二）", CulvertType::CulvertLengthAdjustment2, 1.0);
        self.add_rule("(二)", CulvertType::CulvertLengthAdjustment2, 1.0);
        self.add_rule("（三）", CulvertType::CulvertLengthAdjustment3, 1.0);
        self.add_rule("(三)", CulvertType::CulvertLengthAdjustment3, 1.0);
        self.add_rule("方案图", CulvertType::CulvertLengthAdjustment1, 0.6);

        // 斜布钢筋类
        self.add_rule("斜布钢筋", CulvertType::SkewedReinforcementCombination, 1.0);
        self.add_rule("斜布", CulvertType::SkewedReinforcementCombination, 0.8);
    }

    /// 添加规则
    fn add_rule(&mut self, keyword: &'static str, template_type: CulvertType, weight: f32) {
        self.keyword_rules.insert(keyword, template_type);
        self.weight_rules.insert(keyword, weight);
    }

    /// 从 OCR 文本提取关键词并计算匹配度
    fn match_template(&self, text: &str) -> (CulvertType, f32) {
        let text_lower = text.to_lowercase();
        let mut scores: HashMap<CulvertType, f32> = HashMap::new();

        // 遍历所有关键词规则
        for (keyword, template_type) in &self.keyword_rules {
            if text_lower.contains(keyword) {
                let weight = self.weight_rules.get(keyword).unwrap_or(&1.0);
                *scores.entry(template_type.clone()).or_insert(0.0) += weight;
            }
        }

        // 找出得分最高的模板类型
        let mut best_type = CulvertType::CulvertLayout; // 默认类型
        let mut best_score = 0.0;

        for (template_type, score) in scores {
            if score > best_score {
                best_score = score;
                best_type = template_type;
            }
        }

        // 归一化分数（0-1 范围）
        let normalized_score = if best_score > 0.0 {
            (best_score / 3.0).min(1.0) // 假设最高 3 个关键词匹配
        } else {
            0.0
        };

        (best_type, normalized_score)
    }

    /// 直接从 OCR 文本选择模板（用于外部 OCR 集成）
    pub fn select_from_ocr_text(&self, text: &str) -> (CulvertType, f32) {
        self.match_template(text)
    }

    /// 从文本选择模板类型（用于测试）
    pub fn select_template_from_text(&self, text: &str) -> CulvertType {
        let (template_type, _confidence) = self.match_template(text);
        template_type
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_matching() {
        let selector = RuleBasedTemplateSelector::new(TemplateSelectorConfig::default());

        // 测试表格类匹配
        let text1 = "涵洞设置一览表 统计表 汇总";
        let result1 = selector.select_template_from_text(text1);
        assert_eq!(result1, CulvertType::CulvertSettingTable);

        // 测试数量表匹配
        let text2 = "工程数量表 混凝土用量 钢筋规格";
        let result2 = selector.select_template_from_text(text2);
        assert_eq!(result2, CulvertType::CulvertQuantityTable);

        // 测试布置图匹配
        let text3 = "涵洞布置图 立面图 平面图";
        let result3 = selector.select_template_from_text(text3);
        assert_eq!(result3, CulvertType::CulvertLayout);

        // 测试防水图匹配
        let text4 = "涵身接缝防水 止水带安装";
        let result4 = selector.select_template_from_text(text4);
        assert_eq!(result4, CulvertType::JointWaterproofing);
    }

    #[test]
    fn test_config_default() {
        let config = TemplateSelectorConfig::default();
        assert_eq!(config.confidence_threshold, 0.6);
        assert!(config.enable_logging);
    }
}
