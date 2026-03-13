//! 领域服务 trait 模块
//!
//! 注意：这些 trait 目前未被使用，为未来扩展保留
//! 已禁用未使用的 trait 导出以减少编译器警告

// 以下 trait 未被使用，已注释掉
// pub mod quota;
// pub mod auth;
// pub mod analysis;
// pub mod pdf_conversion;

// 保留 template_selection 因为 ClassificationResult 和 TemplateClassifier 仍在使用
pub mod template_selection;

// 重新导出 trait 供应用层使用
// pub use quota::QuotaService;
// pub use auth::AuthService;
// pub use analysis::AnalysisService;
// pub use pdf_conversion::PdfConversionService;

// 导出 template_selection 相关内容
pub use template_selection::{TemplateClassifier, ClassificationResult, CulvertDrawingType};

// 导出统一的涵洞类型（推荐使用）
pub use crate::domain::model::drawing::CulvertType;
