//! 领域层 - 核心业务逻辑和模型
//!
//! 本层包含：
//! - 领域模型（User, Drawing, ApiKey, Quota）
//! - 领域服务 trait
//! - 领域事件
//!
//! **依赖**: 仅标准库，无外部依赖

pub mod model;
pub mod service;

// 已删除 repository 层 - trait 未被使用

// 重新导出统一错误类型中的 DomainError
pub use crate::error::{DomainError, DomainResult};
