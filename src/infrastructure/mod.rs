//! 基础设施层模块
//!
//! 包含所有外部依赖的具体实现：
//! - PDF 转换服务
//! - 模板选择服务（多模态模型）
//! - 置信度阈值处理
//! - 数据库访问
//! - 外部 API 客户端

pub mod external;
pub mod pdf_conversion;
pub mod template_selection;
pub mod confidence_handler;
#[cfg(test)]
mod tests;

// 核心类型导出（按需导出，避免未使用警告）

