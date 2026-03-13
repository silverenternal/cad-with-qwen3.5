//! 模板选择服务实现模块
//!
//! 提供三种模板选择方式：
//! 1. **混合分类**（推荐）- 规则优先，低置信度时用多模态模型，平衡成本和准确率
//! 2. **多模态模型分类** - 使用 llava/qwen 等模型直接分类，准确率 90%+
//! 3. **基于规则匹配** - 需要外部提供 OCR 文本

pub mod rule_based_selector;
pub mod multimodal_classifier;
pub mod hybrid_classifier;
pub mod template_cache;

pub use rule_based_selector::{RuleBasedTemplateSelector, TemplateSelectorConfig};
pub use multimodal_classifier::{MultimodalTemplateClassifier, ClassifierConfig};
pub use hybrid_classifier::{
    HybridTemplateClassifier, HybridClassifierConfig,
    ClassificationStrategy,
};
pub use template_cache::TemplateCacheConfig;
