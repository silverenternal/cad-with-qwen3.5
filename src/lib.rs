//! CAD 图纸识别 - 简易版库
//!
//! 架构分层：
//! - domain: 领域层（核心业务模型和规则）
//! - application: 应用层（用例和服务）
//! - infrastructure: 基础设施层（技术实现，包括外部 API 客户端）
//! - interfaces: 接口层（CLI, HTTP API）

pub mod batch;
pub mod batch_result;
pub mod cache;
pub mod cli;
pub mod config;
pub mod db;
pub mod dialog;
pub mod error;
pub mod metrics;
pub mod mock;
pub mod pdf_utils;
pub mod prompt;
pub mod recognition_validator;
pub mod security;
pub mod server;
pub mod telemetry;

// 领域分层架构（新增）
// 这些模块需要在 batch 等模块中访问，所以必须公开
pub mod domain;
pub mod application;
pub mod infrastructure;

// 集成测试
#[cfg(test)]
mod tests;

pub use config::{Config, ConfigManager};
pub use dialog::DialogManager;
pub use telemetry::{TelemetryRecorder, TelemetryEvent, TelemetryExport};
pub use metrics::{Metrics, GLOBAL_METRICS, REGISTRY, encode_metrics};
pub use batch_result::{BatchResult, FileResult, FileStatus, OutputFormat};
pub use server::types::DrawingType;

// 批量处理新架构导出
pub use batch::{BatchProcessor, BatchProcessorConfig};

// 重新导出 domain 中的常用类型
pub use domain::service::CulvertType;

// 确保 infrastructure 模块完全公开
pub use infrastructure::*;
