//! 路径遍历攻击测试
//!
//! 验证 PathGuard 中间件能否有效防止路径遍历攻击

use crate::security::path_middleware::{PathGuard, AsyncPathGuard};
use tempfile::TempDir;
use std::fs;

#[test]
fn test_traversal_attack_with_dotdot() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    
    let guard = PathGuard::new(root);
    
    // 尝试访问根目录外的文件
    let result = guard.read_to_string("../sensitive.txt");
    
    assert!(result.is_err(), "应该阻止路径遍历攻击");
}

#[test]
fn test_traversal_attack_with_absolute_path() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    
    let guard = PathGuard::new(root);
    let result = guard.read_to_string("/etc/passwd");
    
    // 应该被阻止（路径不在根目录内）
    assert!(result.is_err());
}

#[test]
fn test_traversal_attack_with_encoded_path() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    
    // 创建测试文件
    fs::write(root.join("test.txt"), "test data").unwrap();
    
    let guard = PathGuard::new(root);
    
    // 正常访问应该成功
    let result = guard.read_to_string("test.txt");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "test data");
    
    // 尝试使用编码路径遍历（应该被当作普通路径处理）
    let result = guard.read_to_string("..%2F..%2Fetc%2Fpasswd");
    assert!(result.is_err());
}

#[test]
fn test_traversal_attack_in_subdir() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // 创建子目录
    let subdir = root.join("subdir");
    fs::create_dir_all(&subdir).unwrap();
    fs::write(subdir.join("file.txt"), "subdir data").unwrap();

    // 在根目录内创建文件（应该可以访问）
    fs::write(root.join("secret.txt"), "secret").unwrap();

    let guard = PathGuard::new(root);

    // 在子目录内使用 .. 但不超出根目录应该成功
    let result = guard.read_to_string("subdir/../secret.txt");
    println!("Result for subdir/../secret.txt: {:?}", result);
    assert!(result.is_ok(), "应该允许在根目录内使用 ..: {:?}", result.err());

    // 尝试超出根目录应该失败
    let result = guard.read_to_string("subdir/../../etc/passwd");
    assert!(result.is_err(), "应该阻止超出根目录的访问");
}

#[test]
fn test_read_dir_traversal() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    
    // 创建目录结构
    fs::create_dir_all(root.join("subdir1")).unwrap();
    fs::create_dir_all(root.join("subdir2")).unwrap();
    fs::write(root.join("subdir1/file1.txt"), "1").unwrap();
    fs::write(root.join("subdir2/file2.txt"), "2").unwrap();
    
    let guard = PathGuard::new(root);
    let files = guard.read_dir("").unwrap();
    
    // 应该包含根目录内的所有文件
    assert_eq!(files.len(), 2);
    for file in &files {
        // 验证文件在根目录内
        let canonical_file = std::fs::canonicalize(file).unwrap();
        let canonical_root = std::fs::canonicalize(root).unwrap();
        assert!(canonical_file.starts_with(&canonical_root), "文件应该在根目录内");
    }
}

#[tokio::test]
async fn test_async_traversal_attack() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    
    let guard = AsyncPathGuard::new(root);
    
    // 尝试路径遍历
    let result = guard.read_to_string("../../etc/passwd").await;
    assert!(result.is_err(), "异步版本也应该阻止路径遍历");
}

#[test]
fn test_create_file_traversal() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    let guard = PathGuard::new(root);

    // 尝试在根目录外创建文件
    let result = guard.write("../malicious.txt", b"malicious content");
    println!("Result for ../malicious.txt: {:?}", result);
    assert!(result.is_err(), "应该阻止在根目录外创建文件");

    // 正常创建应该成功
    let result = guard.write("normal.txt", b"normal content");
    println!("Result for normal.txt: {:?}", result);
    assert!(result.is_ok(), "正常创建应该成功：{:?}", result.err());
    assert!(root.join("normal.txt").exists());
}
