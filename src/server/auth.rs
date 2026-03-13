//! Authentication module - API Key authentication

use axum::{
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tracing::{warn, info};

use crate::{db::{Database, hash_api_key_with_salt}, security};

/// API Key 哈希到前缀的映射（内存中只存哈希和前缀，不存明文）
pub type ApiKeyHashToPrefix = Arc<RwLock<HashMap<String, String>>>;

/// Authenticated user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUser {
    pub user_id: String,
    pub api_key_prefix: String,  // Only store prefix for logging
}

/// Auth state
pub struct AuthState {
    // 内存中存储：哈希 -> 前缀映射（不存明文，只存前缀用于显示）
    api_key_hash_to_prefix: ApiKeyHashToPrefix,
    db: Option<Arc<dyn Database>>,
}

impl AuthState {
    pub fn new() -> Self {
        Self {
            api_key_hash_to_prefix: Arc::new(RwLock::new(HashMap::new())),
            db: None,
        }
    }

    pub fn with_db(mut self, db: Arc<dyn Database>) -> Self {
        self.db = Some(db);
        self
    }

    pub async fn add_api_key(&self, key: String) {
        // 计算加盐哈希和前缀
        let key_hash = hash_api_key_with_salt(&key);
        let key_prefix = crate::db::get_key_prefix(&key);

        // 存储哈希 -> 前缀映射到内存（不存明文）
        self.api_key_hash_to_prefix.write().await.insert(key_hash.clone(), key_prefix.clone());

        // 持久化到数据库
        if let Some(ref db) = self.db {
            if let Err(e) = db
                .save_api_key(&key_hash, &key_prefix, None, None)
                .await
            {
                warn!("Failed to save API Key to database: {}", e);
            }
        }
    }

    pub async fn remove_api_key(&self, key: &str) {
        let key_hash = hash_api_key_with_salt(key);
        // 从内存移除哈希 -> 前缀映射
        self.api_key_hash_to_prefix.write().await.remove(&key_hash);

        // 从数据库撤销
        if let Some(ref db) = self.db {
            if let Err(e) = db.revoke_api_key(&key_hash).await {
                warn!("Failed to revoke API Key: {}", e);
            }
        }
    }

    pub async fn contains_key(&self, key: &str) -> bool {
        // 计算加盐哈希并检查
        let key_hash = hash_api_key_with_salt(key);
        self.api_key_hash_to_prefix.read().await.contains_key(&key_hash)
    }

    /// 获取所有 API Key 前缀列表
    pub async fn get_api_key_prefixes(&self) -> Vec<String> {
        self.api_key_hash_to_prefix
            .read()
            .await
            .values()
            .cloned()
            .collect()
    }

    /// 获取 API Key 数量
    pub async fn get_api_key_count(&self) -> usize {
        self.api_key_hash_to_prefix.read().await.len()
    }
}

impl Default for AuthState {
    fn default() -> Self {
        Self::new()
    }
}

/// API Key authentication middleware
pub async fn api_key_auth(
    State(state): State<Arc<AuthState>>,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Get API Key from header
    let auth_header = request
        .headers()
        .get("Authorization")
        .and_then(|value| value.to_str().ok());

    let api_key = match auth_header {
        Some(header) if header.starts_with("Bearer ") => {
            &header[7..]
        }
        _ => {
            warn!("Missing Authorization header");
            return Err(StatusCode::UNAUTHORIZED);
        }
    };

    // Verify API Key
    if !state.contains_key(api_key).await {
        // 安全脱敏：不显示真实 API Key，只显示 hash 前缀
        let masked = security::mask_api_key_for_log(api_key);
        warn!("Invalid API Key: {}...", masked);
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Inject user info into request extensions
    let user = AuthUser {
        user_id: crate::server::user_id::extract_user_id(&request)
            .unwrap_or_else(|| "user_unknown".to_string()),
        // 安全脱敏：不存储完整 API Key，只存储脱敏后的前缀
        api_key_prefix: security::mask_api_key(api_key),
    };

    info!("User {} authenticated", user.user_id);
    request.extensions_mut().insert(user);

    Ok(next.run(request).await)
}

/// Load API Keys from environment variables
pub async fn load_api_keys() -> Vec<String> {
    use std::collections::HashSet;
    
    let mut keys = HashSet::new();

    // Load main API Key from .env
    if let Ok(key) = std::env::var("OLLAMA_API_KEY") {
        if !key.is_empty() && key != "your_new_api_key_here" {
            keys.insert(key);
        }
    }

    // Support multiple API Keys (comma separated)
    if let Ok(extra_keys) = std::env::var("API_KEYS") {
        for key in extra_keys.split(',').map(|s| s.trim()) {
            if !key.is_empty() {
                keys.insert(key.to_string());
            }
        }
    }

    info!("Loaded {} API Key(s)", keys.len());
    keys.into_iter().collect()
}

/// Get authenticated user from request
pub fn get_auth_user(request: &Request<axum::body::Body>) -> Option<&AuthUser> {
    request.extensions().get::<AuthUser>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_auth_state() {
        let state = AuthState::new();

        state.add_api_key("test_key_123".to_string()).await;

        assert!(state.contains_key("test_key_123").await);
        assert!(!state.contains_key("invalid_key").await);

        let prefixes = state.get_api_key_prefixes().await;
        assert_eq!(prefixes.len(), 1);
        assert!(prefixes[0].starts_with("test_key"));

        state.remove_api_key("test_key_123").await;
        assert!(!state.contains_key("test_key_123").await);
        assert!(state.get_api_key_prefixes().await.is_empty());
    }
}
