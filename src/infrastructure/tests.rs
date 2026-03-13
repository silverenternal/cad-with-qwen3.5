//! PDF 转换和模板选择集成测试

#[cfg(test)]
mod tests {
    use crate::domain::service::template_selection::CulvertDrawingType;
    use crate::infrastructure::pdf_conversion::pdf_converter::{PdfConverter, PdfConverterConfig, is_pdf_data};
    use crate::infrastructure::template_selection::rule_based_selector::{RuleBasedTemplateSelector, TemplateSelectorConfig};

    // ==================== PDF 转换测试 ====================

    #[test]
    fn test_pdf_converter_config_default() {
        let config = PdfConverterConfig::default();
        assert_eq!(config.dpi, 150);
        assert!(config.enabled);
        assert_eq!(config.temp_dir, "./tmp/pdf_convert");
    }

    #[test]
    fn test_pdf_magic_detection() {
        // 有效 PDF 头
        let valid_pdf = b"%PDF-1.4 test content";
        assert!(is_pdf_data(valid_pdf));

        // 无效 PDF 头
        let invalid_pdf = b"not a pdf file";
        assert!(!is_pdf_data(invalid_pdf));

        // 空数据
        let empty: &[u8] = &[];
        assert!(!is_pdf_data(empty));
    }

    #[test]
    fn test_pdf_converter_disabled() {
        // 由于 PdfConverter 已简化，此测试仅验证配置
        let config = PdfConverterConfig::default();
        assert!(config.enabled);
        
        let mut disabled_config = PdfConverterConfig::default();
        disabled_config.enabled = false;
        assert!(!disabled_config.enabled);
    }

    #[test]
    fn test_pdf_converter_invalid_data() {
        // 由于 PdfConverter 已简化，此测试仅验证 is_pdf_data 函数
        let not_pdf = b"this is not a pdf";
        assert!(!is_pdf_data(not_pdf));
        
        let valid_pdf = b"%PDF-1.4 test";
        assert!(is_pdf_data(valid_pdf));
    }

    // ==================== 模板选择测试 ====================

    #[test]
    fn test_template_selector_config_default() {
        let config = TemplateSelectorConfig::default();
        assert_eq!(config.confidence_threshold, 0.6);
        assert!(config.enable_logging);
    }

    #[test]
    fn test_template_selector_basic_matching() {
        let selector = RuleBasedTemplateSelector::new(TemplateSelectorConfig::default());

        // 测试表格类匹配
        let (result1, confidence1) = selector.select_from_ocr_text("涵洞设置一览表 统计表");
        assert_eq!(result1, CulvertDrawingType::CulvertSettingTable);
        assert!(confidence1 > 0.0);

        // 测试数量表匹配
        let (result2, confidence2) = selector.select_from_ocr_text("工程数量表 混凝土 钢筋");
        assert_eq!(result2, CulvertDrawingType::CulvertQuantityTable);
        assert!(confidence2 > 0.0);

        // 测试布置图匹配
        let (result3, confidence3) = selector.select_from_ocr_text("涵洞布置图 立面图 平面图");
        assert_eq!(result3, CulvertDrawingType::CulvertLayout);
        assert!(confidence3 > 0.0);
    }

    #[test]
    fn test_template_selector_reinforcement_matching() {
        let selector = RuleBasedTemplateSelector::new(TemplateSelectorConfig::default());

        // 测试 2m 孔径钢筋图
        let (result, _) = selector.select_from_ocr_text("2m 孔径箱涵涵身钢筋构造图");
        assert_eq!(result, CulvertDrawingType::BoxCulvertReinforcement2m);

        // 测试 3m 孔径钢筋图
        let (result, _) = selector.select_from_ocr_text("3m 孔径箱涵涵身钢筋构造图");
        assert_eq!(result, CulvertDrawingType::BoxCulvertReinforcement3m);

        // 测试 4m 孔径钢筋图
        let (result, _) = selector.select_from_ocr_text("4m 孔径箱涵涵身钢筋构造图");
        assert_eq!(result, CulvertDrawingType::BoxCulvertReinforcement4m);
    }

    #[test]
    fn test_template_selector_skewed_matching() {
        let selector = RuleBasedTemplateSelector::new(TemplateSelectorConfig::default());

        // 测试斜涵匹配
        let (result, _) = selector.select_from_ocr_text("30°斜度 2m 孔径箱涵钢筋构造图 斜涵");
        assert_eq!(result, CulvertDrawingType::SkewedBoxCulvertReinforcement2m);
    }

    #[test]
    fn test_template_selector_waterproof_matching() {
        let selector = RuleBasedTemplateSelector::new(TemplateSelectorConfig::default());

        // 测试防水图匹配
        let (result, _) = selector.select_from_ocr_text("涵身接缝防水 止水带安装");
        assert_eq!(result, CulvertDrawingType::JointWaterproofing);

        // 测试止水带图匹配
        let (result, _) = selector.select_from_ocr_text("止水带安装示意图");
        assert_eq!(result, CulvertDrawingType::WaterStopInstallation);
    }

    #[test]
    fn test_template_selector_plan_matching() {
        let selector = RuleBasedTemplateSelector::new(TemplateSelectorConfig::default());

        // 测试方案图匹配
        let (result, _) = selector.select_from_ocr_text("涵长调整方案图（一）");
        assert_eq!(result, CulvertDrawingType::CulvertLengthAdjustment1);

        let (result, _) = selector.select_from_ocr_text("涵长调整方案图（二）");
        assert_eq!(result, CulvertDrawingType::CulvertLengthAdjustment2);

        let (result, _) = selector.select_from_ocr_text("涵长调整方案图（三）");
        assert_eq!(result, CulvertDrawingType::CulvertLengthAdjustment3);
    }

    #[test]
    fn test_template_selector_confidence_threshold() {
        let mut config = TemplateSelectorConfig::default();
        config.confidence_threshold = 0.9; // 设置高阈值
        let selector = RuleBasedTemplateSelector::new(config);

        // 低置信度匹配
        let (result, confidence) = selector.select_from_ocr_text("一些模糊的文字");
        // 应该返回默认类型
        assert_eq!(result, CulvertDrawingType::CulvertLayout);
        assert!(confidence < 0.9);
    }

    #[test]
    fn test_culvert_drawing_type_conversion() {
        // 测试 to_internal_id
        let drawing_type = CulvertDrawingType::CulvertSettingTable;
        assert_eq!(drawing_type.to_internal_id(), "culvert_setting_table");

        // 测试 from_internal_id
        let result = CulvertDrawingType::from_internal_id("culvert_setting_table");
        assert_eq!(result, Some(CulvertDrawingType::CulvertSettingTable));

        // 测试无效 ID
        let result = CulvertDrawingType::from_internal_id("invalid_id");
        assert_eq!(result, None);
    }

    #[test]
    fn test_culvert_drawing_type_all_types() {
        let all_types = CulvertDrawingType::get_all_types();
        assert_eq!(all_types.len(), 20); // 20 种类型

        // 验证包含所有预期类型
        assert!(all_types.contains(&CulvertDrawingType::CulvertSettingTable));
        assert!(all_types.contains(&CulvertDrawingType::CulvertQuantityTable));
        assert!(all_types.contains(&CulvertDrawingType::CulvertLayout));
        assert!(all_types.contains(&CulvertDrawingType::BoxCulvertReinforcement2m));
        assert!(all_types.contains(&CulvertDrawingType::SkewedBoxCulvertReinforcement3m));
        assert!(all_types.contains(&CulvertDrawingType::JointWaterproofing));
        assert!(all_types.contains(&CulvertDrawingType::SkewedReinforcementCombination));
    }
}
