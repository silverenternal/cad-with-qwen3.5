//! 模板选择服务 trait - 定义核心业务逻辑
//!
//! ## 架构设计原则
//! - 领域层只定义核心业务逻辑（分类接口）
//! - 批量操作、缓存管理等应用逻辑放在应用层
//! - 依赖倒置：高层模块不依赖低层模块的具体实现

use crate::domain::DomainResult;
use crate::domain::model::drawing::CulvertType;

/// 向后兼容：涵洞图纸类型别名
///
/// 已迁移到 `crate::domain::model::drawing::CulvertType`
pub type CulvertDrawingType = CulvertType;

/// 分类结果 - 领域层核心类型
#[derive(Debug, Clone)]
pub struct ClassificationResult {
    /// 分类的模板类型
    pub template_type: CulvertType,
    /// 置信度分数（0.0-1.0）
    pub confidence: f32,
    /// 是否需要人工复核
    pub needs_review: bool,
    /// 分类来源描述（用于日志和调试）
    pub source: String,
}

/// 模板分类器 trait - 领域层核心接口
///
/// ## 职责
/// - 单张图片的分类
/// - 返回带置信度的分类结果
///
/// ## 不负责
/// - 批量处理（应用层职责）
/// - 缓存管理（基础设施层职责）
#[async_trait::async_trait]
pub trait TemplateClassifier: Send + Sync {
    /// 分类单张图片
    async fn classify(&self, image_data: &[u8]) -> DomainResult<ClassificationResult>;
}

/// 基于规则的分类器 trait（可选）
///
/// 用于从文本内容快速分类
pub trait RuleBasedClassifier: Send + Sync {
    /// 从 OCR 文本分类
    fn classify_from_text(&self, text: &str) -> (CulvertType, f32);
}
