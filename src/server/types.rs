//! API 响应结构

use serde::{Deserialize, Serialize};

/// 图纸类型枚举（CLI 和 Web API 共用）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DrawingType {
    BuildingPlan,
    StructurePlan,
    Reinforcement,
    RoadSection,
    Foundation,
    Custom(String),
}

impl DrawingType {
    /// 预定义的图纸类型列表（中文）
    pub const PREDEFINED_TYPES: &'static [&'static str] = &[
        "建筑平面图",
        "结构平面图",
        "结构配筋图",
        "市政道路断面图",
        "基坑支护图",
    ];

    /// 转换为显示字符串（中文，用于日志和 API）
    pub fn as_str(&self) -> &str {
        match self {
            DrawingType::BuildingPlan => "建筑平面图",
            DrawingType::StructurePlan => "结构平面图",
            DrawingType::Reinforcement => "结构配筋图",
            DrawingType::RoadSection => "市政道路断面图",
            DrawingType::Foundation => "基坑支护图",
            DrawingType::Custom(s) => s.as_str(),
        }
    }

    /// 验证图纸类型（统一验证逻辑）
    pub fn validate(drawing_type: &str) -> ValidationResult {
        if drawing_type.is_empty() {
            return Err("Drawing type cannot be empty".to_string());
        }

        if drawing_type.len() > 50 {
            return Err("Drawing type name cannot exceed 50 characters".to_string());
        }

        // 不允许包含特殊字符
        if drawing_type.chars().any(|c| matches!(c, '<' | '>' | '"' | '\'' | '&' | '\\')) {
            return Err("Drawing type cannot contain special characters: < > \" ' & \\".to_string());
        }

        Ok(())
    }

    /// 检查是否是预定义类型
    pub fn is_predefined(s: &str) -> bool {
        Self::PREDEFINED_TYPES.contains(&s)
    }

    /// 获取所有预定义类型
    pub fn get_predefined_types() -> &'static [&'static str] {
        Self::PREDEFINED_TYPES
    }
}

impl std::str::FromStr for DrawingType {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "建筑平面图" | "Building Plan" | "building_plan" => DrawingType::BuildingPlan,
            "结构平面图" | "Structure Plan" | "structure_plan" => DrawingType::StructurePlan,
            "结构配筋图" | "Reinforcement Plan" | "reinforcement" => DrawingType::Reinforcement,
            "市政道路断面图" | "Road Section" | "road_section" => DrawingType::RoadSection,
            "基坑支护图" | "Foundation Plan" | "foundation" => DrawingType::Foundation,
            _ => DrawingType::Custom(s.to_string()),
        })
    }
}

/// API 错误码
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ErrorCode {
    /// 无效请求
    InvalidRequest,
    /// 认证失败
    Unauthorized,
    /// 禁止访问
    Forbidden,
    /// 资源不存在
    NotFound,
    /// 速率限制
    RateLimited,
    /// 内部错误
    InternalError,
    /// 模型错误
    ModelError,
    /// 配额超限
    QuotaExceeded,
    /// 服务不可用
    ServiceUnavailable,
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorCode::InvalidRequest => write!(f, "INVALID_REQUEST"),
            ErrorCode::Unauthorized => write!(f, "UNAUTHORIZED"),
            ErrorCode::Forbidden => write!(f, "FORBIDDEN"),
            ErrorCode::NotFound => write!(f, "NOT_FOUND"),
            ErrorCode::RateLimited => write!(f, "RATE_LIMITED"),
            ErrorCode::InternalError => write!(f, "INTERNAL_ERROR"),
            ErrorCode::ModelError => write!(f, "MODEL_ERROR"),
            ErrorCode::QuotaExceeded => write!(f, "QUOTA_EXCEEDED"),
            ErrorCode::ServiceUnavailable => write!(f, "SERVICE_UNAVAILABLE"),
        }
    }
}

/// 通用 API 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<ApiError>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn error(code: ErrorCode, message: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(ApiError {
                code: code.to_string(),
                message,
            }),
        }
    }

    pub fn invalid_request(message: String) -> Self {
        Self::error(ErrorCode::InvalidRequest, message)
    }

    pub fn unauthorized(message: String) -> Self {
        Self::error(ErrorCode::Unauthorized, message)
    }

    pub fn quota_exceeded(message: String) -> Self {
        Self::error(ErrorCode::QuotaExceeded, message)
    }
}

/// API 错误
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

/// 验证结果
pub type ValidationResult = Result<(), String>;

/// 通用验证器 Trait
/// 
/// 为不同类型提供统一的验证接口
pub trait Validatable<T> {
    /// 验证数据
    fn validate(value: &T) -> ValidationResult;
}

/// 图片 Base64 数据验证器
pub struct ImageBase64Validator;

impl Validatable<String> for ImageBase64Validator {
    fn validate(data: &String) -> ValidationResult {
        if data.is_empty() {
            return Err("图片数据不能为空".to_string());
        }

        // 检查 Base64 格式
        if !data.chars().all(|c| c.is_alphanumeric() || c == '+' || c == '/' || c == '=') {
            return Err("图片数据必须是有效的 Base64 格式".to_string());
        }

        // 检查最小尺寸（约 1KB）
        if data.len() < 1024 {
            return Err("图片数据太小，请上传至少 1KB 的图片".to_string());
        }

        // 检查最大尺寸（约 10MB）
        if data.len() > 10 * 1024 * 1024 {
            return Err("图片数据太大，请上传不超过 10MB 的图片".to_string());
        }

        Ok(())
    }
}

/// 图纸类型验证器
pub struct DrawingTypeValidator;

impl Validatable<String> for DrawingTypeValidator {
    fn validate(drawing_type: &String) -> ValidationResult {
        DrawingType::validate(drawing_type)
    }
}

/// 问题文本验证器
pub struct QuestionValidator;

impl Validatable<String> for QuestionValidator {
    fn validate(question: &String) -> ValidationResult {
        if question.is_empty() {
            return Err("问题不能为空".to_string());
        }

        if question.len() > 2000 {
            return Err("问题不能超过 2000 个字符".to_string());
        }

        Ok(())
    }
}

/// 消息文本验证器
pub struct MessageValidator;

impl Validatable<String> for MessageValidator {
    fn validate(message: &String) -> ValidationResult {
        if message.is_empty() {
            return Err("消息不能为空".to_string());
        }

        if message.len() > 4000 {
            return Err("消息不能超过 4000 个字符".to_string());
        }

        Ok(())
    }
}

/// 会话 ID 验证器
pub struct SessionIdValidator;

impl Validatable<String> for SessionIdValidator {
    fn validate(session_id: &String) -> ValidationResult {
        if session_id.len() > 128 {
            return Err("会话 ID 不能超过 128 个字符".to_string());
        }

        // 检查是否只包含合法字符
        if !session_id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            return Err("会话 ID 只能包含字母、数字、连字符和下划线".to_string());
        }

        Ok(())
    }
}

/// 向后兼容的 Validator 工具结构体
///
/// 提供静态方法接口，内部调用各专用验证器
pub struct Validator;

impl Validator {
    /// 验证图片 Base64 数据
    pub fn validate_image_base64(data: &str) -> ValidationResult {
        ImageBase64Validator::validate(&data.to_string())
    }

    /// 验证图纸类型（使用统一的 DrawingType::validate）
    pub fn validate_drawing_type(drawing_type: &str) -> ValidationResult {
        DrawingTypeValidator::validate(&drawing_type.to_string())
    }

    /// 验证问题文本
    pub fn validate_question(question: &str) -> ValidationResult {
        QuestionValidator::validate(&question.to_string())
    }

    /// 验证消息文本
    pub fn validate_message(message: &str) -> ValidationResult {
        MessageValidator::validate(&message.to_string())
    }

    /// 验证会话 ID
    pub fn validate_session_id(session_id: &str) -> ValidationResult {
        SessionIdValidator::validate(&session_id.to_string())
    }
}

/// 健康检查响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub timestamp: String,
    /// 依赖服务检查详情
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checks: Option<std::collections::HashMap<String, String>>,
}

/// 图纸分析请求
#[derive(Debug, Clone, Deserialize)]
pub struct AnalyzeRequest {
    /// 图片的 Base64 数据
    pub image_base64: String,
    /// 图纸类型
    pub drawing_type: Option<String>,
    /// 用户问题
    pub question: Option<String>,
}

/// 图纸分析响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzeResponse {
    /// AI 分析结果
    pub content: String,
    /// 使用的模型
    pub model: String,
    /// 请求延迟（毫秒）
    pub latency_ms: u64,
}

/// 对话请求
#[derive(Debug, Clone, Deserialize)]
pub struct ChatRequest {
    /// 用户消息
    pub message: String,
    /// 可选的图片
    pub images: Option<Vec<String>>,
    /// 会话 ID（可选，用于多轮对话）
    pub session_id: Option<String>,
}

/// 对话响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    /// AI 回复内容
    pub content: String,
    /// 会话 ID
    pub session_id: String,
    /// 请求延迟（毫秒）
    pub latency_ms: u64,
}

/// 统计信息响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsResponse {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub avg_latency_ms: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_response_success() {
        let response = ApiResponse::success("test data".to_string());
        assert!(response.success);
        assert!(response.data.is_some());
        assert!(response.error.is_none());
    }

    #[test]
    fn test_api_response_error() {
        let response: ApiResponse<()> = ApiResponse::error(
            ErrorCode::InvalidRequest,
            "test error".to_string()
        );
        assert!(!response.success);
        assert!(response.data.is_none());
        assert!(response.error.is_some());
        let error = response.error.unwrap();
        assert_eq!(error.code, "INVALID_REQUEST");
        assert_eq!(error.message, "test error");
    }

    #[test]
    fn test_error_code_display() {
        assert_eq!(ErrorCode::InvalidRequest.to_string(), "INVALID_REQUEST");
        assert_eq!(ErrorCode::Unauthorized.to_string(), "UNAUTHORIZED");
        assert_eq!(ErrorCode::QuotaExceeded.to_string(), "QUOTA_EXCEEDED");
    }

    #[test]
    fn test_validate_image_base64_empty() {
        assert!(Validator::validate_image_base64("").is_err());
    }

    #[test]
    fn test_validate_image_base64_invalid_chars() {
        assert!(Validator::validate_image_base64("invalid!@#base64").is_err());
    }

    #[test]
    fn test_validate_image_base64_too_small() {
        assert!(Validator::validate_image_base64("SGVsbG8=").is_err()); // "Hello" 的 Base64
    }

    #[test]
    fn test_validate_image_base64_valid() {
        // 生成一个合法的 Base64 字符串（约 3KB，使用合法 Base64 字符）
        // Base64 字符集：A-Za-z0-9+/，长度必须是 4 的倍数
        let valid_base64 = "AAAA".repeat(768); // 3072 字符，合法 Base64
        assert!(Validator::validate_image_base64(&valid_base64).is_ok());
    }

    #[test]
    fn test_validate_drawing_type_empty() {
        assert!(Validator::validate_drawing_type("").is_err());
    }

    #[test]
    fn test_validate_drawing_type_too_long() {
        assert!(Validator::validate_drawing_type(&"a".repeat(51)).is_err());
    }

    #[test]
    fn test_validate_drawing_type_valid() {
        assert!(Validator::validate_drawing_type("建筑平面图").is_ok());
        assert!(Validator::validate_drawing_type("结构图").is_ok());
        assert!(Validator::validate_drawing_type("电气图").is_ok());
        assert!(Validator::validate_drawing_type("其他").is_ok());
    }

    #[test]
    fn test_validate_drawing_type_invalid() {
        // 测试特殊字符
        assert!(Validator::validate_drawing_type("类型<test>").is_err());
        assert!(Validator::validate_drawing_type("类型\"test\"").is_err());
        assert!(Validator::validate_drawing_type("类型&test").is_err());
    }

    #[test]
    fn test_validate_question_empty() {
        assert!(Validator::validate_question("").is_err());
    }

    #[test]
    fn test_validate_question_too_long() {
        assert!(Validator::validate_question(&"a".repeat(2001)).is_err());
    }

    #[test]
    fn test_validate_question_valid() {
        assert!(Validator::validate_question("请分析这张图纸").is_ok());
    }

    #[test]
    fn test_validate_message_empty() {
        assert!(Validator::validate_message("").is_err());
    }

    #[test]
    fn test_validate_message_too_long() {
        assert!(Validator::validate_message(&"a".repeat(4001)).is_err());
    }

    #[test]
    fn test_validate_message_valid() {
        assert!(Validator::validate_message("你好，请帮我分析").is_ok());
    }

    #[test]
    fn test_validate_session_id_too_long() {
        assert!(Validator::validate_session_id(&"a".repeat(129)).is_err());
    }

    #[test]
    fn test_validate_session_id_invalid_chars() {
        assert!(Validator::validate_session_id("invalid@id!").is_err());
    }

    #[test]
    fn test_validate_session_id_valid() {
        assert!(Validator::validate_session_id("sess_1234567890").is_ok());
        assert!(Validator::validate_session_id("user-id_test").is_ok());
    }

    // ===== Validator Trait 测试 =====

    #[test]
    fn test_image_base64_validator_trait() {
        // 测试空字符串
        assert!(ImageBase64Validator::validate(&"".to_string()).is_err());
        
        // 测试非法字符
        assert!(ImageBase64Validator::validate(&"invalid!@#base64".to_string()).is_err());
        
        // 测试太小数据
        assert!(ImageBase64Validator::validate(&"SGVsbG8=".to_string()).is_err());
        
        // 测试有效数据
        let valid_base64 = "AAAA".repeat(768);
        assert!(ImageBase64Validator::validate(&valid_base64).is_ok());
    }

    #[test]
    fn test_drawing_type_validator_trait() {
        // 测试空字符串
        assert!(DrawingTypeValidator::validate(&"".to_string()).is_err());
        
        // 测试超长
        assert!(DrawingTypeValidator::validate(&"a".repeat(51)).is_err());
        
        // 测试有效
        assert!(DrawingTypeValidator::validate(&"建筑平面图".to_string()).is_ok());
        
        // 测试特殊字符
        assert!(DrawingTypeValidator::validate(&"类型<test>".to_string()).is_err());
    }

    #[test]
    fn test_question_validator_trait() {
        // 测试空字符串
        assert!(QuestionValidator::validate(&"".to_string()).is_err());
        
        // 测试超长
        assert!(QuestionValidator::validate(&"a".repeat(2001)).is_err());
        
        // 测试有效
        assert!(QuestionValidator::validate(&"请分析这张图纸".to_string()).is_ok());
    }

    #[test]
    fn test_message_validator_trait() {
        // 测试空字符串
        assert!(MessageValidator::validate(&"".to_string()).is_err());
        
        // 测试超长
        assert!(MessageValidator::validate(&"a".repeat(4001)).is_err());
        
        // 测试有效
        assert!(MessageValidator::validate(&"你好，请帮我分析".to_string()).is_ok());
    }

    #[test]
    fn test_session_id_validator_trait() {
        // 测试超长
        assert!(SessionIdValidator::validate(&"a".repeat(129)).is_err());
        
        // 测试非法字符
        assert!(SessionIdValidator::validate(&"invalid@id!".to_string()).is_err());
        
        // 测试有效
        assert!(SessionIdValidator::validate(&"sess_1234567890".to_string()).is_ok());
        assert!(SessionIdValidator::validate(&"user-id_test".to_string()).is_ok());
    }
}
