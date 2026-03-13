//! 统一错误类型模块
//!
//! # 设计原则
//! - 错误类型合并为 5 种以内，按处理策略分类
//! - 在 main.rs 统一处理和日志记录
//!
//! # 错误分类 (5 种)
//! - `Validation` - 验证错误（客户端错误，4xx）
//! - `NotFound` - 资源不存在（客户端错误，4xx）
//! - `Unauthorized` - 认证/授权错误（4xx）
//! - `External` - 外部服务错误（5xx，可重试）
//! - `Internal` - 内部错误（5xx，不可重试）

use thiserror::Error;

// ==================== 统一错误类型 ====================

/// 统一错误类型 - 所有错误最终转换为这个类型
#[derive(Debug, Error)]
pub enum Error {
    #[error("验证错误：{0}")]
    Validation(String),

    #[error("资源不存在：{0}")]
    NotFound(String),

    #[error("未授权：{0}")]
    Unauthorized(String),

    #[error("外部服务错误：{0}")]
    External(String),

    #[error("内部错误：{0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, Error>;

// ==================== 便捷构造方法 ====================

impl Error {
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self::Unauthorized(msg.into())
    }

    pub fn external(msg: impl Into<String>) -> Self {
        Self::External(msg.into())
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }

    /// 获取用户友好的错误消息
    pub fn user_message(&self) -> String {
        match self {
            Self::Validation(msg) => format!("输入验证失败：{}", msg),
            Self::NotFound(msg) => format!("资源不存在：{}", msg),
            Self::Unauthorized(msg) => format!("未授权访问：{}", msg),
            Self::External(msg) => format!("服务暂时不可用：{}", msg),
            Self::Internal(msg) => format!("内部错误：{}", msg),
        }
    }

    /// 是否可重试
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::External(_))
    }
}

// ==================== 从其他错误类型转换 ====================

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        use std::io::ErrorKind;
        match e.kind() {
            // 文件不存在 - 404
            ErrorKind::NotFound => Error::NotFound(format!("文件不存在：{}", e)),
            // 权限拒绝 - 401
            ErrorKind::PermissionDenied => Error::Unauthorized(format!("权限拒绝：{}", e)),
            // 内存不足 - 500 内部错误
            ErrorKind::OutOfMemory => Error::Internal(format!("内存不足：{}", e)),
            // 其他 IO 错误通常与外部系统相关，视为外部错误（可重试）
            _ => Error::External(format!("IO 错误：{}", e)),
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        // JSON 解析错误通常是验证错误（客户端发送了无效数据）
        Error::Validation(format!("JSON 解析失败：{}", e))
    }
}

impl From<image::ImageError> for Error {
    fn from(e: image::ImageError) -> Self {
        use image::ImageError as IE;
        match &e {
            // 文件不存在
            IE::IoError(io_e) if io_e.kind() == std::io::ErrorKind::NotFound => {
                Error::NotFound(format!("图片文件不存在：{}", e))
            }
            // 权限问题
            IE::IoError(io_e) if io_e.kind() == std::io::ErrorKind::PermissionDenied => {
                Error::Unauthorized(format!("图片文件权限拒绝：{}", e))
            }
            // 图片格式/解码错误 - 验证错误
            IE::Unsupported(_) | IE::Decoding(_) | IE::Encoding(_) => {
                Error::Validation(format!("图片格式错误：{}", e))
            }
            // 其他图片错误 - 外部错误（可能重试）
            _ => Error::External(format!("图片处理错误：{}", e)),
        }
    }
}

impl From<tokio::task::JoinError> for Error {
    fn from(e: tokio::task::JoinError) -> Self {
        Self::Internal(e.to_string())
    }
}

// ==================== 领域业务错误（内部使用） ====================

/// 领域业务错误 - 内部使用，最终会转换为 Error
#[derive(Debug, Error)]
pub enum DomainError {
    #[error("验证失败：{field} - {reason}")]
    Validation { field: String, reason: String },

    #[error("资源不存在：{entity}（ID: {id}）")]
    NotFound { entity: &'static str, id: String },

    #[error("配额已用尽：今日已使用 {current} 次，限制 {limit} 次/天")]
    QuotaExceeded { current: u32, limit: u32 },

    #[error("认证失败：{0}")]
    Authentication(String),

    #[error("授权失败：{0}")]
    Authorization(String),

    #[error("业务规则违反：{0}")]
    BusinessRule(String),

    #[error("外部服务调用失败：{service}::{endpoint}, 错误：{message}")]
    ExternalService { service: String, endpoint: String, message: String },
}

impl From<DomainError> for Error {
    fn from(e: DomainError) -> Self {
        match e {
            DomainError::Validation { field, reason } => {
                Error::Validation(format!("{}: {}", field, reason))
            }
            DomainError::NotFound { entity, id } => {
                Error::NotFound(format!("{} (ID: {})", entity, id))
            }
            DomainError::QuotaExceeded { current, limit } => {
                Error::Validation(format!("配额已用尽：{} / {}", current, limit))
            }
            DomainError::Authentication(msg) | DomainError::Authorization(msg) => {
                Error::Unauthorized(msg)
            }
            DomainError::BusinessRule(msg) => Error::Validation(msg),
            DomainError::ExternalService { service, endpoint, message } => {
                Error::External(format!("{}::{}: {}", service, endpoint, message))
            }
        }
    }
}

pub type DomainResult<T> = std::result::Result<T, DomainError>;

impl DomainError {
    pub fn validation(field: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Validation {
            field: field.into(),
            reason: reason.into(),
        }
    }

    pub fn not_found(entity: &'static str, id: impl Into<String>) -> Self {
        Self::NotFound { entity, id: id.into() }
    }

    pub fn quota_exceeded(current: u32, limit: u32) -> Self {
        Self::QuotaExceeded { current, limit }
    }

    pub fn external_service(
        service: impl Into<String>,
        endpoint: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::ExternalService {
            service: service.into(),
            endpoint: endpoint.into(),
            message: message.into(),
        }
    }
}

// ==================== 配置错误 ====================

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("IO 错误：{0}")]
    Io(#[from] std::io::Error),

    #[error("解析错误：{0}")]
    Parse(#[from] toml::de::Error),

    #[error("配置值无效：{0}")]
    InvalidValue(String),

    #[error("配置错误：{0}")]
    Other(String),
}

impl ConfigError {
    pub fn custom(msg: impl Into<String>) -> Self {
        Self::Other(msg.into())
    }
}

impl From<ConfigError> for Error {
    fn from(e: ConfigError) -> Self {
        match e {
            ConfigError::InvalidValue(msg) | ConfigError::Other(msg) => {
                Error::Internal(format!("配置错误：{}", msg))
            }
            _ => Error::Internal(e.to_string()),
        }
    }
}

// ==================== 缓存错误 ====================

#[derive(Debug, Error)]
pub enum CacheError {
    #[error("未找到：{0}")]
    NotFound(String),

    #[error("未命中：{0}")]
    Miss(String),

    #[error("写入失败：{0}")]
    Write(String),

    #[error("已过期：{0}")]
    Expired(String),

    #[error("图片处理错误：{0}")]
    ImageError(String),

    #[error("无效的缓存大小：{0}")]
    InvalidSize(usize),

    #[error("IO 错误：{0}")]
    IoError(#[from] std::io::Error),

    #[error("路径安全错误：{0}")]
    PathSecurity(String),
}

impl From<CacheError> for Error {
    fn from(e: CacheError) -> Self {
        match e {
            CacheError::NotFound(msg) => Error::NotFound(msg),
            CacheError::Miss(msg) => Error::NotFound(format!("缓存未命中：{}", msg)),
            CacheError::Write(msg) | CacheError::ImageError(msg) => Error::Internal(msg),
            _ => Error::Internal(e.to_string()),
        }
    }
}

impl From<PathSecurityError> for CacheError {
    fn from(e: PathSecurityError) -> Self {
        CacheError::PathSecurity(e.to_string())
    }
}

// ==================== 路径安全错误 ====================

#[derive(Debug, Error)]
pub enum PathSecurityError {
    #[error("路径遍历攻击检测：{0}")]
    TraversalAttempt(String),

    #[error("路径不在允许的目录内：{0}")]
    OutsideAllowedDir(String),

    #[error("无效的路径：{0}")]
    InvalidPath(String),

    #[error("IO 错误：{0}")]
    IoError(#[from] std::io::Error),
}

impl From<PathSecurityError> for Error {
    fn from(e: PathSecurityError) -> Self {
        match e {
            PathSecurityError::TraversalAttempt(msg) => {
                Error::Unauthorized(format!("路径遍历攻击：{}", msg))
            }
            PathSecurityError::OutsideAllowedDir(msg) => {
                Error::Unauthorized(format!("路径越界：{}", msg))
            }
            PathSecurityError::InvalidPath(msg) => Error::Validation(format!("无效路径：{}", msg)),
            PathSecurityError::IoError(e) => Error::Internal(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_conversion() {
        // 测试 DomainError 可以转换为 Error
        let domain_err = DomainError::validation("field", "test");
        let err: Error = domain_err.into();
        assert!(matches!(err, Error::Validation(_)));

        let domain_err = DomainError::not_found("User", "123");
        let err: Error = domain_err.into();
        assert!(matches!(err, Error::NotFound(_)));

        let domain_err = DomainError::quota_exceeded(100, 100);
        let err: Error = domain_err.into();
        assert!(matches!(err, Error::Validation(_)));

        let domain_err = DomainError::Authentication("invalid token".into());
        let err: Error = domain_err.into();
        assert!(matches!(err, Error::Unauthorized(_)));
    }

    #[test]
    fn test_error_display() {
        let err = Error::validation("测试错误");
        assert!(err.to_string().contains("验证错误"));
        assert!(err.to_string().contains("测试错误"));

        let err = Error::not_found("资源不存在");
        assert!(err.to_string().contains("资源不存在"));

        let err = Error::internal("服务器错误");
        assert!(err.to_string().contains("内部错误"));
    }

    #[test]
    fn test_retryable() {
        assert!(Error::external("timeout").is_retryable());
        assert!(!Error::validation("bad input").is_retryable());
        assert!(!Error::internal("panic").is_retryable());
        assert!(!Error::not_found("missing").is_retryable());
        assert!(!Error::unauthorized("auth failed").is_retryable());
    }

    #[test]
    fn test_user_message() {
        let err = Error::validation("字段不能为空");
        assert!(err.user_message().contains("验证失败"));

        let err = Error::not_found("用户 ID: 123");
        assert!(err.user_message().contains("资源不存在"));

        let err = Error::unauthorized("token 过期");
        assert!(err.user_message().contains("未授权"));

        let err = Error::external("API 超时");
        assert!(err.user_message().contains("服务暂时不可用"));

        let err = Error::internal("数据库连接失败");
        assert!(err.user_message().contains("内部错误"));
    }

    #[test]
    fn test_domain_error_helpers() {
        let err = DomainError::validation("email", "格式不正确");
        assert!(matches!(err, DomainError::Validation { .. }));

        let err = DomainError::not_found("User", "456");
        assert!(matches!(err, DomainError::NotFound { .. }));

        let err = DomainError::quota_exceeded(50, 50);
        assert!(matches!(err, DomainError::QuotaExceeded { .. }));

        let err = DomainError::external_service("ollama", "chat", "timeout");
        assert!(matches!(err, DomainError::ExternalService { .. }));
    }

    #[test]
    fn test_path_security_error_conversion() {
        let err = PathSecurityError::TraversalAttempt("../etc/passwd".into());
        let converted: Error = err.into();
        assert!(matches!(converted, Error::Unauthorized(_)));

        let err = PathSecurityError::OutsideAllowedDir("/tmp".into());
        let converted: Error = err.into();
        assert!(matches!(converted, Error::Unauthorized(_)));

        let err = PathSecurityError::InvalidPath("".into());
        let converted: Error = err.into();
        assert!(matches!(converted, Error::Validation(_)));
    }

    #[test]
    fn test_cache_error_conversion() {
        let err = CacheError::NotFound("image.png".into());
        let converted: Error = err.into();
        assert!(matches!(converted, Error::NotFound(_)));

        let err = CacheError::Miss("key".into());
        let converted: Error = err.into();
        assert!(matches!(converted, Error::NotFound(_)));

        let err = CacheError::Write("disk full".into());
        let converted: Error = err.into();
        assert!(matches!(converted, Error::Internal(_)));
    }
}
