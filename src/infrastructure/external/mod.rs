//! API 客户端模块 - 支持 Ollama 本地和 Cloud 模式

use std::time::Duration;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{warn, error};

/// API 错误类型
#[derive(Debug, Error)]
pub enum ApiError {
    /// HTTP 错误 - 区分不同状态码
    #[error("HTTP 错误：{status}")]
    HttpError { 
        status: StatusCode, 
        message: String,
    },
    
    /// 认证失败
    #[error("认证失败：API Key 无效或已过期")]
    InvalidApiKey,
    
    /// 模型不存在
    #[error("模型不存在：{model}")]
    ModelNotFound { model: String },
    
    /// 请求超时
    #[error("请求超时：{0}")]
    Timeout(String),
    
    /// 超时错误消息（内部使用）
    #[error("请求超时")]
    TimeoutMsg(String),
    
    /// 网络错误（可重试）
    #[error("网络错误：{0}")]
    NetworkError(#[from] reqwest::Error),
    
    /// JSON 解析错误（不可重试）
    #[error("JSON 解析错误：{0}")]
    JsonError(#[from] serde_json::Error),
    
    /// IO 错误
    #[error("IO 错误：{0}")]
    IoError(#[from] std::io::Error),
    
    /// 速率限制
    #[error("请求过于频繁，请稍后再试")]
    RateLimitExceeded,
    
    /// 服务端错误（5xx）
    #[error("服务端错误：{status} - {message}")]
    ServerError { 
        status: StatusCode, 
        message: String,
    },
    
    /// 客户端错误（4xx，除认证外）
    #[error("客户端错误：{status} - {message}")]
    ClientError { 
        status: StatusCode, 
        message: String,
    },
}

impl ApiError {
    /// 从 HTTP 响应构建错误
    pub fn from_response(status: StatusCode, message: String) -> Self {
        match status {
            StatusCode::UNAUTHORIZED => ApiError::InvalidApiKey,
            StatusCode::NOT_FOUND => ApiError::ModelNotFound { 
                model: "unknown".to_string() 
            },
            StatusCode::TOO_MANY_REQUESTS => ApiError::RateLimitExceeded,
            StatusCode::REQUEST_TIMEOUT | StatusCode::GATEWAY_TIMEOUT => {
                ApiError::TimeoutMsg(message)
            }
            status if status.is_server_error() => ApiError::ServerError { status, message },
            status if status.is_client_error() => ApiError::ClientError { status, message },
            _ => ApiError::HttpError { status, message },
        }
    }
    
    /// 判断是否为可重试错误
    /// 
    /// 只有网络波动、超时、服务端临时错误才应该重试
    /// 认证错误、客户端错误、JSON 解析错误不应重试
    pub fn is_retryable(&self) -> bool {
        match self {
            // 网络错误通常可重试
            ApiError::NetworkError(e) => {
                e.is_timeout() || e.is_connect() || e.is_request()
            }
            // IO 错误通常可重试
            ApiError::IoError(_) => true,
            // 超时错误可重试
            ApiError::Timeout(_) | ApiError::TimeoutMsg(_) => true,
            // 服务端 5xx 错误可重试
            ApiError::ServerError { status, .. } => status.is_server_error(),
            // 速率限制可重试（带退避）
            ApiError::RateLimitExceeded => true,
            // 以下错误不应重试
            ApiError::InvalidApiKey => false,      // 认证错误重试无用
            ApiError::ModelNotFound { .. } => false,  // 模型不存在重试无用
            ApiError::ClientError { .. } => false,    // 客户端错误重试无用
            ApiError::JsonError(_) => false,          // JSON 解析错误重试无用
            ApiError::HttpError { status, .. } => {
                // 根据状态码判断
                status.is_server_error() || status.as_u16() == 429
            }
        }
    }

    /// 判断是否为认证错误
    pub fn is_auth_error(&self) -> bool {
        matches!(self, ApiError::InvalidApiKey)
    }

    /// 判断是否为模型不存在错误
    pub fn is_model_not_found(&self) -> bool {
        matches!(self, ApiError::ModelNotFound { .. })
    }
    
    /// 判断是否为用户错误（非服务端问题）
    pub fn is_client_error(&self) -> bool {
        matches!(
            self,
            ApiError::InvalidApiKey
            | ApiError::ModelNotFound { .. }
            | ApiError::ClientError { .. }
            | ApiError::JsonError(_)
        )
    }
    
    /// 获取错误码（用于监控和告警）
    pub fn error_code(&self) -> &'static str {
        match self {
            ApiError::HttpError { .. } => "HTTP_ERROR",
            ApiError::InvalidApiKey => "INVALID_API_KEY",
            ApiError::ModelNotFound { .. } => "MODEL_NOT_FOUND",
            ApiError::Timeout(_) | ApiError::TimeoutMsg(_) => "TIMEOUT",
            ApiError::NetworkError(_) => "NETWORK_ERROR",
            ApiError::JsonError(_) => "JSON_ERROR",
            ApiError::IoError(_) => "IO_ERROR",
            ApiError::RateLimitExceeded => "RATE_LIMIT_EXCEEDED",
            ApiError::ServerError { .. } => "SERVER_ERROR",
            ApiError::ClientError { .. } => "CLIENT_ERROR",
        }
    }
}

impl ApiError {
    /// 超时错误消息
    pub fn timeout(msg: impl Into<String>) -> Self {
        ApiError::TimeoutMsg(msg.into())
    }
}

/// 对话消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,
}

impl Message {
    pub fn user(content: String) -> Self {
        Self { role: "user".to_string(), content, images: None }
    }

    pub fn user_with_images(content: String, images: Vec<String>) -> Self {
        Self { role: "user".to_string(), content, images: Some(images) }
    }

    pub fn assistant(content: String) -> Self {
        Self { role: "assistant".to_string(), content, images: None }
    }
}

/// 聊天请求
#[derive(Debug, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
}

impl ChatRequest {
    pub fn new(model: String, messages: Vec<Message>) -> Self {
        Self { model, messages }
    }
}

/// API 客户端 - 支持 Ollama 本地和 Cloud 模式
#[derive(Clone)]
pub struct ApiClient {
    client: Client,
    base_url: String,
    model: String,
    api_key: Option<String>,
    max_retries: usize,
}

impl ApiClient {
    /// 创建本地 Ollama 客户端
    pub fn local(model: &str, max_retries: usize) -> Self {
        // HTTP client 创建失败概率极低，但如果失败说明系统资源有问题
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("Failed to create HTTP client: insufficient system resources or invalid configuration");
        Self {
            client,
            base_url: "http://localhost:11434".to_string(),
            model: model.to_string(),
            api_key: None,
            max_retries,
        }
    }

    /// 创建 Cloud 客户端
    pub fn cloud(model: &str, api_key: &str, max_retries: usize) -> Self {
        // HTTP client 创建失败概率极低，但如果失败说明系统资源有问题
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("Failed to create HTTP client: insufficient system resources or invalid configuration");
        Self {
            client,
            base_url: "https://ollama.com".to_string(),
            model: model.to_string(),
            api_key: Some(api_key.to_string()),
            max_retries,
        }
    }

    /// 发送聊天请求（非流式）
    pub async fn chat(&self, messages: &[Message]) -> Result<String, ApiError> {
        let url = format!("{}/api/chat", self.base_url.trim_end_matches('/'));

        let body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "stream": false,
        });

        let mut last_error = None;
        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                // 指数退避：100ms, 200ms, 400ms, ...
                let delay = Duration::from_millis(100 * (1 << attempt));
                warn!("请求失败，{}ms 后重试 ({}/{})", delay.as_millis(), attempt + 1, self.max_retries + 1);
                tokio::time::sleep(delay).await;
            }

            match self.do_request(&url, &body).await {
                Ok(content) => return Ok(content),
                Err(e) => {
                    if e.is_retryable() {
                        last_error = Some(e);
                        continue;
                    } else {
                        // 不可重试错误，直接返回
                        error!("不可重试的错误：{}", e);
                        return Err(e);
                    }
                }
            }
        }

        // 重试耗尽
        Err(last_error.unwrap_or_else(|| ApiError::HttpError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "重试耗尽".to_string(),
        }))
    }

    /// 发送聊天请求（流式）
    pub async fn chat_stream(&self, messages: &[Message]) -> Result<String, ApiError> {
        // 简化：流式和非流式使用相同实现
        self.chat(messages).await
    }

    async fn do_request(&self, url: &str, body: &serde_json::Value) -> Result<String, ApiError> {
        let mut request = self.client.post(url).json(body);

        if let Some(ref key) = self.api_key {
            request = request.header("Authorization", format!("Bearer {}", key));
        }

        let response = request.send().await?;
        let status = response.status();
        let text = response.text().await?;

        if status.is_success() {
            let json: serde_json::Value = serde_json::from_str(&text)?;
            json["message"]["content"]
                .as_str()
                .map(|s| s.to_string())
                .or_else(|| json["content"].as_str().map(|s| s.to_string()))
                .ok_or_else(|| ApiError::IoError(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "无法解析响应内容"
                )))
        } else {
            Err(ApiError::from_response(status, text))
        }
    }

    /// 获取客户端名称
    pub fn client_name(&self) -> &str {
        if self.api_key.is_some() { "Ollama Cloud" } else { "Ollama Local" }
    }

    /// 获取模型名称
    pub fn model(&self) -> &str {
        &self.model
    }

    /// 为会话克隆客户端（用于会话池）
    /// 
    /// reqwest::Client 内部使用 Arc，克隆是轻量级的
    pub fn clone_for_session(&self) -> Self {
        Self {
            client: self.client.clone(),
            base_url: self.base_url.clone(),
            model: self.model.clone(),
            api_key: self.api_key.clone(),
            max_retries: self.max_retries,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retryable_errors() {
        // 认证错误不应重试
        let err = ApiError::InvalidApiKey;
        assert!(!err.is_retryable());

        // 模型不存在不应重试
        let err = ApiError::ModelNotFound { model: "test".to_string() };
        assert!(!err.is_retryable());

        // 5xx 错误应该重试
        let err = ApiError::ServerError { 
            status: StatusCode::INTERNAL_SERVER_ERROR, 
            message: "test".to_string() 
        };
        assert!(err.is_retryable());

        // 4xx 错误（除速率限制外）不应重试
        let err = ApiError::ClientError { 
            status: StatusCode::BAD_REQUEST, 
            message: "test".to_string() 
        };
        assert!(!err.is_retryable());
        
        // 超时错误应该重试
        let err = ApiError::Timeout("test".to_string());
        assert!(err.is_retryable());
    }

    #[test]
    fn test_error_codes() {
        assert_eq!(ApiError::InvalidApiKey.error_code(), "INVALID_API_KEY");
        assert_eq!(ApiError::ModelNotFound { model: "test".to_string() }.error_code(), "MODEL_NOT_FOUND");
        assert_eq!(ApiError::Timeout("test".to_string()).error_code(), "TIMEOUT");
        assert_eq!(ApiError::RateLimitExceeded.error_code(), "RATE_LIMIT_EXCEEDED");
    }
}
