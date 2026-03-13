//! 认证服务 trait

use crate::domain::model::ApiKey;
use crate::domain::model::api_key::ApiKeyGenerationResult;
use crate::domain::DomainResult;

/// 认证服务 trait - 定义 API Key 认证的核心业务逻辑
#[async_trait::async_trait]
pub trait AuthService: Send + Sync {
    /// 验证 API Key
    async fn verify_api_key(&self, key: &str) -> DomainResult<bool>;

    /// 生成新的 API Key
    async fn generate_api_key(
        &self,
        description: Option<String>,
        expires_in_days: Option<u32>,
    ) -> DomainResult<ApiKeyGenerationResult>;

    /// 撤销 API Key
    async fn revoke_api_key(&self, key_hash: &str) -> DomainResult<bool>;

    /// 列出所有 API Key
    async fn list_api_keys(&self) -> DomainResult<Vec<ApiKey>>;

    /// 轮换 API Key
    ///
    /// # Arguments
    /// * `old_key_hash` - 旧 Key 的 hash
    /// * `revoke_old` - 是否立即撤销旧 Key
    /// * `expires_in_days` - 新 Key 的有效期（天）
    ///
    /// # Returns
    /// 返回新生成的 API Key
    async fn rotate_api_key(
        &self,
        old_key_hash: &str,
        revoke_old: bool,
        expires_in_days: Option<u32>,
    ) -> DomainResult<ApiKeyGenerationResult>;
}
