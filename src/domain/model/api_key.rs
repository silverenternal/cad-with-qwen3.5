//! API Key 领域模型

use chrono::{DateTime, Utc};

/// API Key 状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiKeyStatus {
    Active,
    Revoked,
    Expired,
}

/// API Key 前缀（用于日志和显示）
#[derive(Debug, Clone)]
pub struct ApiKeyPrefix(String);

impl ApiKeyPrefix {
    pub fn new(key: &str) -> Self {
        let prefix = if key.len() >= 8 {
            format!("{}...", &key[..8])
        } else {
            "***".to_string()
        };
        Self(prefix)
    }
    
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// API Key 实体
#[derive(Debug, Clone)]
pub struct ApiKey {
    pub key_hash: String,
    pub key_prefix: String,
    pub status: ApiKeyStatus,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
}

impl ApiKey {
    pub fn new(
        key_hash: impl Into<String>,
        key_prefix: impl Into<String>,
        description: Option<String>,
        expires_at: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            key_hash: key_hash.into(),
            key_prefix: key_prefix.into(),
            status: ApiKeyStatus::Active,
            description,
            created_at: Utc::now(),
            expires_at,
            last_used_at: None,
        }
    }
    
    /// 检查 API Key 是否有效
    pub fn is_valid(&self) -> bool {
        if self.status != ApiKeyStatus::Active {
            return false;
        }
        
        // 检查是否过期
        if let Some(expires) = self.expires_at {
            if Utc::now() > expires {
                return false;
            }
        }
        
        true
    }
    
    /// 撤销 API Key
    pub fn revoke(&mut self) {
        self.status = ApiKeyStatus::Revoked;
    }
    
    /// 更新最后使用时间
    pub fn touch(&mut self) {
        self.last_used_at = Some(Utc::now());
    }
    
    /// 检查是否过期并更新状态
    pub fn check_expiration(&mut self) {
        if let Some(expires) = self.expires_at {
            if Utc::now() > expires && self.status == ApiKeyStatus::Active {
                self.status = ApiKeyStatus::Expired;
            }
        }
    }
}

/// API Key 生成结果
#[derive(Debug, Clone)]
pub struct ApiKeyGenerationResult {
    pub key: String,           // 完整 Key（仅返回一次）
    pub key_prefix: String,    // Key 前缀（用于显示）
    pub key_hash: String,      // Hash 值（用于存储）
}

impl ApiKeyGenerationResult {
    pub fn new(key: String, key_hash: String) -> Self {
        let prefix = if key.len() >= 8 {
            format!("{}...", &key[..8])
        } else {
            "***".to_string()
        };
        
        Self {
            key,
            key_prefix: prefix,
            key_hash,
        }
    }
}
