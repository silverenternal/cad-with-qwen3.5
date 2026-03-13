//! 安全工具模块 - 防止常见安全漏洞
//!
//! 本模块提供：
//! - 路径遍历防护
//! - 文件类型嗅探（MIME 验证）
//! - API Key 脱敏
//! - 其他安全相关工具

pub mod path_middleware;

use std::path::{Path, PathBuf};
use thiserror::Error;

// 使用统一错误类型中的 PathSecurityError
use crate::error::PathSecurityError;

/// 验证并规范化用户提供的路径，防止路径遍历攻击
///
/// # 参数
/// * `root` - 允许的根目录
/// * `user_path` - 用户提供的路径（可能包含 `..` 等危险元素）
///
/// # 返回
/// * `Ok(PathBuf)` - 安全的全路径
/// * `Err(PathSecurityError)` - 检测到路径遍历攻击
///
/// # 示例
/// ```
/// use std::path::Path;
/// let root = Path::new("/var/data");
/// let user_path = "../../etc/passwd";
/// assert!(sanitize_path(root, user_path).is_err());
/// ```
pub fn sanitize_path<P: AsRef<Path>>(root: P, user_path: &str) -> Result<PathBuf, PathSecurityError> {
    let root = root.as_ref();

    // 规范化根目录（解析符号链接，转换为绝对路径）
    let canonical_root = root.canonicalize()
        .map_err(|e| PathSecurityError::InvalidPath(format!("根目录无效：{}", e)))?;

    // 拼接路径
    let full_path = root.join(user_path);

    // 尝试规范化完整路径
    // 如果文件不存在，使用手动规范化
    match full_path.canonicalize() {
        Ok(canonical_full) => {
            // 文件存在，检查是否在根目录内
            // 使用 Path::starts_with 而不是字符串比较，避免 C:\data 和 C:\data2 误判
            if canonical_full.starts_with(&canonical_root) {
                Ok(canonical_full)
            } else {
                Err(PathSecurityError::TraversalAttempt(
                    format!("尝试访问根目录外的路径：{} (root: {})",
                        canonical_full.display(), canonical_root.display())
                ))
            }
        }
        Err(_) => {
            // 文件不存在时，手动规范化路径并验证
            let normalized = normalize_path(&full_path);

            // 规范化路径也需要 canonicalize 以确保格式一致
            let canonical_normalized = normalized.canonicalize()
                .unwrap_or(normalized);

            // 检查规范化后的路径是否在根目录内
            // 使用 Path::starts_with 而不是字符串比较，避免路径前缀误判
            if canonical_normalized.starts_with(&canonical_root) {
                Ok(canonical_normalized)
            } else {
                Err(PathSecurityError::TraversalAttempt(
                    format!("尝试访问根目录外的路径：{} (root: {})",
                        canonical_normalized.display(), canonical_root.display())
                ))
            }
        }
    }
}

/// 验证并规范化路径（如果文件不存在则返回拼接后的路径）
///
/// 用于新文件的创建场景，此时文件还不存在
pub fn sanitize_path_for_create<P: AsRef<Path>>(root: P, user_path: &str) -> Result<PathBuf, PathSecurityError> {
    let root = root.as_ref();

    // 规范化根目录
    let canonical_root = root.canonicalize()
        .map_err(|e| PathSecurityError::InvalidPath(format!("根目录无效：{}", e)))?;

    // 拼接路径（不检查文件是否存在）
    let full_path = root.join(user_path);

    // 规范化路径（处理 `..` 和符号链接）
    let normalized_path = normalize_path(&full_path);

    // 确保规范化后的路径有绝对路径（如果 normalize_path 返回的是相对路径）
    let normalized_path = if normalized_path.is_absolute() {
        normalized_path
    } else {
        std::env::current_dir()
            .map_err(|e| PathSecurityError::InvalidPath(format!("无法获取当前目录：{}", e)))?
            .join(normalized_path)
    };

    // 检查是否在根目录内
    // 使用字符串比较来处理 Windows UNC 路径问题
    let root_str = canonical_root.to_string_lossy().replace(r"\\?\", "").to_lowercase();
    let path_str = normalized_path.to_string_lossy().replace(r"\\?\", "").to_lowercase();

    // 确保根目录字符串以分隔符结尾，避免 C:\data 和 C:\data2 误判
    let root_str = if root_str.ends_with('\\') || root_str.ends_with('/') {
        root_str
    } else {
        format!("{}\\", root_str)
    };

    // 路径必须等于根目录或以根目录开头且后面紧跟分隔符
    if path_str == root_str.trim_end_matches('\\').trim_end_matches('/') || path_str.starts_with(&root_str) {
        Ok(normalized_path)
    } else {
        Err(PathSecurityError::TraversalAttempt(
            format!("尝试在根目录外创建文件：{} (root: {})",
                normalized_path.display(), canonical_root.display())
        ))
    }
}

/// 规范化路径（不依赖文件系统）
/// 处理 `..` 和 `.` 组件
fn normalize_path(path: &Path) -> PathBuf {
    let mut components = path.components().peekable();
    let mut ret = if let Some(c @ std::path::Component::Prefix(..)) = components.peek().cloned() {
        components.next();
        PathBuf::from(c.as_os_str())
    } else {
        PathBuf::new()
    };

    for component in components {
        match component {
            std::path::Component::Prefix(..) => unreachable!(),
            std::path::Component::RootDir => {
                ret.push(component.as_os_str());
            }
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                ret.pop();
            }
            std::path::Component::Normal(c) => {
                ret.push(c);
            }
        }
    }

    // 尝试规范化路径（如果路径存在则使用 canonicalize）
    // 这样可以确保返回的路径格式与根目录一致（都是 UNC 或都不是）
    if ret.try_exists().unwrap_or(false) {
        ret.canonicalize().unwrap_or(ret)
    } else {
        ret
    }
}

/// MIME 类型验证错误
#[derive(Debug, Error)]
pub enum MimeTypeError {
    #[error("未知的文件类型")]
    UnknownType,
    #[error("不允许的文件类型：{0}")]
    DisallowedType(String),
    #[error("文件内容与实际类型不符")]
    TypeMismatch,
}

/// 文件类型白名单
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllowedImageType {
    Png,
    Jpeg,
    Gif,
    WebP,
    Bmp,
}

impl AllowedImageType {
    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Gif => "image/gif",
            Self::WebP => "image/webp",
            Self::Bmp => "image/bmp",
        }
    }
    
    pub fn all() -> &'static [Self] {
        &[Self::Png, Self::Jpeg, Self::Gif, Self::WebP, Self::Bmp]
    }
}

/// 嗅探文件的真实 MIME 类型（不依赖文件扩展名）
///
/// 使用文件魔数（magic numbers）进行识别
/// 
/// # 参数
/// * `bytes` - 文件内容的前若干字节（建议至少 8 字节）
///
/// # 返回
/// * `Some(&'static str)` - 检测到的 MIME 类型
/// * `None` - 无法识别的类型
pub fn sniff_mime_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() < 8 {
        return None;
    }

    // 检查常见图片格式的魔数
    if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some("image/png");
    }
    
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    
    if bytes.starts_with(&[0x52, 0x49, 0x46, 0x46]) && bytes.len() >= 12 && bytes[8..12] == [0x57, 0x45, 0x42, 0x50] {
        return Some("image/webp");
    }
    
    if bytes.starts_with(&[0x42, 0x4D]) {
        return Some("image/bmp");
    }

    None
}

/// 验证上传的文件是否为允许的图片类型
///
/// # 参数
/// * `bytes` - 完整的文件内容
/// * `allowed_types` - 允许的类型白名单
///
/// # 返回
/// * `Ok(AllowedImageType)` - 验证通过，返回检测到的类型
/// * `Err(MimeTypeError)` - 验证失败
pub fn validate_image_content(
    bytes: &[u8],
    allowed_types: &[AllowedImageType],
) -> Result<AllowedImageType, MimeTypeError> {
    let detected_mime = sniff_mime_type(bytes)
        .ok_or(MimeTypeError::UnknownType)?;
    
    // 检查是否在白名单内
    let allowed_mimes: Vec<&str> = allowed_types
        .iter()
        .map(|t| t.mime_type())
        .collect();
    
    if !allowed_mimes.contains(&detected_mime) {
        return Err(MimeTypeError::DisallowedType(detected_mime.to_string()));
    }
    
    // 返回对应的类型枚举
    match detected_mime {
        "image/png" => Ok(AllowedImageType::Png),
        "image/jpeg" => Ok(AllowedImageType::Jpeg),
        "image/gif" => Ok(AllowedImageType::Gif),
        "image/webp" => Ok(AllowedImageType::WebP),
        "image/bmp" => Ok(AllowedImageType::Bmp),
        _ => Err(MimeTypeError::UnknownType),
    }
}

/// API Key 脱敏工具
///
/// 只显示前 8 个字符和后 4 个字符，中间用 `***` 替代
/// 
/// # 示例
/// ```
/// let key = "sk_1234567890abcdef";
/// assert_eq!(mask_api_key(key), "sk_123***cdef");
/// ```
pub fn mask_api_key(api_key: &str) -> String {
    if api_key.len() <= 12 {
        // 太短的 key 全部隐藏
        "***".to_string()
    } else {
        let (prefix, suffix) = api_key.split_at(api_key.len() - 4);
        format!("{}***{}", &prefix[..8], suffix)
    }
}

/// 脱敏显示 API Key（仅显示 hash 前缀）
///
/// 用于日志记录，只显示 hash 的前 8 个字符
pub fn mask_api_key_for_log(api_key: &str) -> String {
    use sha2::{Sha256, Digest};
    
    let hash = Sha256::digest(api_key.as_bytes());
    format!("sha256:{:x}", hash)[..16].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_sanitize_path_normal() {
        let temp_dir = TempDir::new().unwrap();
        // 创建子目录和文件
        let subdir = temp_dir.path().join("subdir");
        std::fs::create_dir_all(&subdir).unwrap();
        std::fs::write(subdir.join("file.txt"), "test").unwrap();
        
        let result = sanitize_path(temp_dir.path(), "subdir/file.txt");
        assert!(result.is_ok(), "Failed: {:?}", result);
        let safe_path = result.unwrap();
        
        // 在 Windows 上，canonicalize 返回 UNC 路径，所以需要比较 canonical 形式
        let canonical_root = temp_dir.path().canonicalize().unwrap();
        assert!(safe_path.starts_with(&canonical_root), 
            "Path {:?} doesn't start with {:?}", safe_path, canonical_root);
    }

    #[test]
    fn test_sanitize_path_traversal_attack() {
        let temp_dir = TempDir::new().unwrap();
        let result = sanitize_path(temp_dir.path(), "../../etc/passwd");
        assert!(result.is_err());
        assert!(matches!(result, Err(PathSecurityError::TraversalAttempt(_))));
    }

    #[test]
    fn test_sanitize_path_dotdot_in_subdir() {
        let temp_dir = TempDir::new().unwrap();
        let subdir = temp_dir.path().join("subdir");
        std::fs::create_dir_all(&subdir).unwrap();
        // 文件在根目录下
        std::fs::write(temp_dir.path().join("file.txt"), "root test").unwrap();

        // 在子目录内使用 .. 是允许的，只要不超出根目录
        // subdir/../file.txt 应该解析为 <root>/file.txt
        let result = sanitize_path(temp_dir.path(), "subdir/../file.txt");

        assert!(result.is_ok(), "Failed: {:?}", result);

        // 验证规范化后的路径
        let safe_path = result.unwrap();
        let canonical_root = temp_dir.path().canonicalize().unwrap();

        // 验证路径在根目录内
        assert!(safe_path.starts_with(&canonical_root),
            "Path {:?} doesn't start with {:?}", safe_path, canonical_root);
        
        // 验证内容正确（应该是根目录下的 file.txt）
        let content = std::fs::read_to_string(&safe_path).unwrap();
        assert_eq!(content, "root test");
    }

    #[test]
    fn test_sniff_mime_type_png() {
        let png_header = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        assert_eq!(sniff_mime_type(&png_header), Some("image/png"));
    }

    #[test]
    fn test_sniff_mime_type_jpeg() {
        let jpeg_header = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46];
        assert_eq!(sniff_mime_type(&jpeg_header), Some("image/jpeg"));
    }

    #[test]
    fn test_mask_api_key() {
        let key = "sk_1234567890abcdef";
        // 显示前 8 个字符 + *** + 后 4 个字符
        assert_eq!(mask_api_key(key), "sk_12345***cdef");
    }

    #[test]
    fn test_mask_api_key_short() {
        let key = "short";
        assert_eq!(mask_api_key(key), "***");
    }
}
