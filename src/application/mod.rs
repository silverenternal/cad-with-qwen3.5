//! 应用层 - 用例和应用服务
//!
//! 本层包含：
//! - 应用服务（协调领域对象完成用户任务）
//! - 命令对象（请求 DTO）
//! - 事件处理器
//!
//! **依赖**: domain 层 + infrastructure 层的 trait

pub mod service;
pub mod command;

// 应用服务按需导出（避免未使用警告）
