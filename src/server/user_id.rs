//! 统一的用户 ID 提取逻辑
//!
//! 所有中间件应该使用这个模块来提取用户 ID，保证行为一致。

use axum::http::Request;
use axum::body::Body;

/// 从请求中提取用户 ID
/// 
/// 提取逻辑：
/// 1. 从 Authorization header 获取 API Key
/// 2. 使用 API Key 前 8 个字符作为用户标识
/// 3. 如果没有 API Key，返回 None
/// 
/// 返回值格式：`user_xxxxxxxx` (API Key 前 8 个字符)
pub fn extract_user_id(request: &Request<Body>) -> Option<String> {
    // 从 Authorization header 获取 API Key
    let api_key = request.headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))?;
    
    // 使用 API Key 前 8 个字符作为用户标识
    let key_prefix = if api_key.len() >= 8 {
        &api_key[..8]
    } else {
        api_key
    };
    
    Some(format!("user_{}", key_prefix))
}

/// 从请求中提取 API Key
pub fn extract_api_key(request: &Request<Body>) -> Option<&str> {
    request.headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
}

/// 从请求中提取用户 ID 或降级到 IP
///
/// 如果没有 API Key，使用客户端 IP 作为用户标识
///
/// 注意：ConnectInfo 需要服务器配置才能获取，如果未配置，返回 "unknown"
pub fn extract_user_id_or_ip(request: &Request<Body>) -> String {
    if let Some(user_id) = extract_user_id(request) {
        user_id
    } else {
        // 降级到客户端 IP
        // 方法 1: 尝试从 ConnectInfo 扩展获取（如果服务器配置了 ConnectInfoLayer）
        let ip = request.extensions()
            .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
            .map(|ci| ci.0.ip().to_string());

        // 方法 2: 如果 ConnectInfo 不可用，使用 "unknown" 作为 fallback
        // 注意：要获取真实 IP，需要在服务器上进行以下配置之一：
        // 1. 使用 axum-server  crate 配合 TLS
        // 2. 使用自定义中间件从 X-Forwarded-For header 提取
        // 3. 使用 tower-http 的 TraceLayer 记录请求信息
        let ip = ip.unwrap_or_else(|| {
            // 尝试从 X-Forwarded-For header 获取（如果有的话）
            request.headers()
                .get("x-forwarded-for")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.split(',').next())
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| "unknown".to_string())
        });

        format!("ip_{}", ip)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn test_extract_user_id_with_valid_key() {
        let mut req = Request::new(Body::empty());
        req.headers_mut().insert(
            "Authorization",
            HeaderValue::from_str("Bearer abcdefgh123456").unwrap(),
        );
        
        let user_id = extract_user_id(&req);
        assert_eq!(user_id, Some("user_abcdefgh".to_string()));
    }

    #[test]
    fn test_extract_user_id_with_short_key() {
        let mut req = Request::new(Body::empty());
        req.headers_mut().insert(
            "Authorization",
            HeaderValue::from_str("Bearer short").unwrap(),
        );
        
        let user_id = extract_user_id(&req);
        assert_eq!(user_id, Some("user_short".to_string()));
    }

    #[test]
    fn test_extract_user_id_without_key() {
        let req = Request::new(Body::empty());
        let user_id = extract_user_id(&req);
        assert_eq!(user_id, None);
    }

    #[test]
    fn test_extract_api_key() {
        let mut req = Request::new(Body::empty());
        req.headers_mut().insert(
            "Authorization",
            HeaderValue::from_str("Bearer sk-test-key").unwrap(),
        );
        
        let key = extract_api_key(&req);
        assert_eq!(key, Some("sk-test-key"));
    }
}
