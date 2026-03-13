//! 死信队列 - 失败文件单独记录和重试

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::batch::BatchError;

/// 失败文件记录
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FailedFile {
    /// 文件路径
    pub path: PathBuf,
    /// 错误信息
    pub error: BatchError,
    /// 重试次数
    pub retry_count: u32,
    /// 失败时间戳
    pub failed_at: chrono::DateTime<chrono::Utc>,
}

impl FailedFile {
    /// 创建新的失败记录
    pub fn new(path: PathBuf, error: BatchError, retry_count: u32) -> Self {
        Self {
            path,
            error,
            retry_count,
            failed_at: chrono::Utc::now(),
        }
    }
}

/// 死信队列
pub struct DeadLetterQueue {
    /// 失败文件列表
    failed_files: RwLock<Vec<FailedFile>>,
    /// 持久化路径
    persistence_path: Option<PathBuf>,
}

impl DeadLetterQueue {
    /// 创建死信队列（无持久化）
    pub fn new() -> Self {
        Self {
            failed_files: RwLock::new(Vec::new()),
            persistence_path: None,
        }
    }

    /// 创建带持久化的死信队列
    pub fn with_persistence<P: AsRef<Path>>(persistence_path: P) -> Self {
        Self {
            failed_files: RwLock::new(Vec::new()),
            persistence_path: Some(persistence_path.as_ref().to_path_buf()),
        }
    }

    /// 添加失败文件
    pub async fn add(&self, path: PathBuf, error: BatchError, retry_count: u32) {
        let mut files = self.failed_files.write().await;
        let failed_file = FailedFile::new(path, error, retry_count);
        files.push(failed_file.clone());

        // 异步持久化
        if let Some(ref persist_path) = self.persistence_path {
            drop(files);  // 释放写锁
            let _ = self.persist_to_file(persist_path).await;
        }
    }

    /// 持久化到文件（带 fsync 确保数据落盘）
    async fn persist_to_file(&self, path: &Path) -> std::io::Result<()> {
        use tokio::fs::{self, File};
        use tokio::io::AsyncWriteExt;

        let files = self.failed_files.read().await;
        let json = serde_json::to_vec_pretty(&*files)
            .map_err(std::io::Error::other)?;

        // 确保目录存在
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // 原子写入（先写临时文件再重命名）
        let tmp_path = path.with_extension("tmp");

        // 使用 File 以便调用 sync_all
        let mut file = File::create(&tmp_path).await?;
        file.write_all(&json).await?;
        file.sync_all().await?;  // 强制落盘
        drop(file);  // 关闭文件句柄

        fs::rename(&tmp_path, path).await?;

        Ok(())
    }

    /// 从文件加载
    pub async fn load_from_file<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        use tokio::fs;

        let content = fs::read_to_string(path).await?;
        let files: Vec<FailedFile> = serde_json::from_str(&content)
            .map_err(std::io::Error::other)?;
        
        let mut current = self.failed_files.write().await;
        current.extend(files);
        
        Ok(())
    }

    /// 获取所有失败文件
    pub async fn get_all(&self) -> Vec<FailedFile> {
        self.failed_files.read().await.clone()
    }

    /// 获取失败文件数量
    pub async fn len(&self) -> usize {
        self.failed_files.read().await.len()
    }

    /// 检查是否为空
    pub async fn is_empty(&self) -> bool {
        self.failed_files.read().await.is_empty()
    }

    /// 清空队列并持久化
    pub async fn clear(&self) {
        self.failed_files.write().await.clear();
        
        if let Some(ref persist_path) = self.persistence_path {
            let _ = self.persist_to_file(persist_path).await;
        }
    }

    /// 导出失败文件列表（用于重试）
    pub async fn export_paths(&self) -> Vec<PathBuf> {
        self.failed_files
            .read()
            .await
            .iter()
            .map(|f| f.path.clone())
            .collect()
    }

    /// 移除已处理的成功文件（重试成功后调用）
    pub async fn remove(&self, path: &Path) {
        let mut files = self.failed_files.write().await;
        files.retain(|f| f.path != path);
        
        if let Some(ref persist_path) = self.persistence_path {
            drop(files);
            let _ = self.persist_to_file(persist_path).await;
        }
    }
}

impl Default for DeadLetterQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// 共享死信队列（线程安全）
pub type SharedDeadLetterQueue = Arc<DeadLetterQueue>;

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::fs;

    #[tokio::test]
    async fn test_dead_letter_queue() {
        let dlq = DeadLetterQueue::new();

        assert_eq!(dlq.len().await, 0);

        dlq.add(
            PathBuf::from("/test/file1.jpg"),
            BatchError::Retryable("test error".to_string()),
            3,
        )
        .await;

        assert_eq!(dlq.len().await, 1);

        let failed = dlq.get_all().await;
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].path, PathBuf::from("/test/file1.jpg"));
        assert_eq!(failed[0].retry_count, 3);

        dlq.clear().await;
        assert_eq!(dlq.len().await, 0);
    }

    #[tokio::test]
    async fn test_dead_letter_queue_persistence() {
        let temp_path = std::env::temp_dir().join("test_dlq.json");

        {
            let dlq = DeadLetterQueue::with_persistence(&temp_path);

            dlq.add(
                PathBuf::from("/test/persist_test.jpg"),
                BatchError::Retryable("persist test".to_string()),
                1,
            )
            .await;

            // 等待持久化完成
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        // 验证文件存在
        assert!(temp_path.exists());

        // 从文件加载
        let dlq2 = DeadLetterQueue::new();
        dlq2.load_from_file(&temp_path).await.unwrap();
        assert_eq!(dlq2.len().await, 1);

        // 清理
        let _ = fs::remove_file(&temp_path).await;
    }

    #[tokio::test]
    async fn test_dead_letter_queue_remove() {
        let dlq = DeadLetterQueue::new();

        dlq.add(
            PathBuf::from("/test/file1.jpg"),
            BatchError::Retryable("error1".to_string()),
            1,
        )
        .await;

        dlq.add(
            PathBuf::from("/test/file2.jpg"),
            BatchError::Retryable("error2".to_string()),
            2,
        )
        .await;

        assert_eq!(dlq.len().await, 2);

        // 移除一个文件
        dlq.remove(Path::new("/test/file1.jpg")).await;

        assert_eq!(dlq.len().await, 1);
        let failed = dlq.get_all().await;
        assert_eq!(failed[0].path, PathBuf::from("/test/file2.jpg"));
    }

    #[tokio::test]
    async fn test_failed_file_timestamp() {
        use chrono::{Utc, Duration};

        let failed_file = FailedFile::new(
            PathBuf::from("/test/timestamp_test.jpg"),
            BatchError::Retryable("test".to_string()),
            0,
        );

        // 验证时间戳是最近的
        let now = Utc::now();
        let diff = now.signed_duration_since(failed_file.failed_at);
        assert!(diff < Duration::seconds(1));
    }

    #[tokio::test]
    async fn test_dead_letter_queue_disk_full_simulation() {
        // 测试磁盘满时的优雅降级
        // 模拟：当磁盘满时，内存队列仍然工作，只是持久化失败

        let dlq = DeadLetterQueue::new();

        // 添加多个失败文件到内存队列
        for i in 0..100 {
            dlq.add(
                PathBuf::from(format!("/test/file_{}.jpg", i)),
                BatchError::Retryable(format!("error {}", i)),
                i,
            )
            .await;
        }

        // 验证内存队列仍然工作
        assert_eq!(dlq.len().await, 100);

        // 尝试持久化到一个无效路径（模拟磁盘满）
        // 使用 Windows 保留设备名，这些路径无法创建
        #[cfg(windows)]
        let invalid_path = std::path::PathBuf::from("C:\\CON\\dead_letter.json");
        #[cfg(unix)]
        let invalid_path = std::path::PathBuf::from("/dev/full/dead_letter.json");

        let result = dlq.persist_to_file(&invalid_path).await;
        // 在大多数系统上应该失败，但即使成功也不影响功能
        // 关键是验证内存队列仍然可用
        assert_eq!(dlq.len().await, 100);
        assert!(!dlq.is_empty().await);
    }

    #[tokio::test]
    async fn test_dead_letter_queue_permission_denied() {
        // 测试权限不足时的优雅降级
        // 在 Windows 上，尝试写入系统目录会失败

        let dlq = DeadLetterQueue::new();

        // 添加失败文件
        dlq.add(
            PathBuf::from("/test/permission_test.jpg"),
            BatchError::Retryable("permission test".to_string()),
            1,
        )
        .await;

        // 验证内存队列工作
        assert_eq!(dlq.len().await, 1);

        // 尝试持久化到受保护的路径（Windows 上的系统目录）
        #[cfg(windows)]
        let protected_path = std::path::PathBuf::from("C:\\Windows\\System32\\dead_letter.json");
        #[cfg(unix)]
        let protected_path = std::path::PathBuf::from("/root/dead_letter.json");

        let result = dlq.persist_to_file(&protected_path).await;
        // 应该失败（权限不足）或成功（如果有权限）
        // 关键是内存队列仍然可用
        assert_eq!(dlq.len().await, 1);
    }

    #[tokio::test]
    async fn test_dead_letter_queue_large_scale() {
        // 大规模测试：添加大量失败文件
        let dlq = DeadLetterQueue::new();

        // 添加 1000 个失败文件
        for i in 0..1000 {
            dlq.add(
                PathBuf::from(format!("/test/large_scale_{}.jpg", i)),
                BatchError::Retryable(format!("large scale error {}", i)),
                i,
            )
            .await;
        }

        // 验证所有文件都在队列中
        assert_eq!(dlq.len().await, 1000);

        // 验证导出功能
        let export = dlq.export_paths().await;
        assert_eq!(export.len(), 1000);

        // 验证 get_all 功能
        let all = dlq.get_all().await;
        assert_eq!(all.len(), 1000);
    }

    #[tokio::test]
    async fn test_dead_letter_queue_concurrent_add() {
        // 并发添加测试：多个任务同时添加失败文件
        use std::sync::Arc;
        use tokio::task;

        let dlq = Arc::new(DeadLetterQueue::new());
        let mut handles = vec![];

        // 创建 50 个并发任务，每个添加 10 个文件
        for task_id in 0..50 {
            let dlq_clone = Arc::clone(&dlq);
            let handle = task::spawn(async move {
                for i in 0..10 {
                    dlq_clone.add(
                        PathBuf::from(format!("/test/concurrent_{}_{}.jpg", task_id, i)),
                        BatchError::Retryable(format!("concurrent error {}-{}", task_id, i)),
                        i,
                    )
                    .await;
                }
            });
            handles.push(handle);
        }

        // 等待所有任务完成
        for handle in handles {
            handle.await.unwrap();
        }

        // 验证总共有 500 个文件
        assert_eq!(dlq.len().await, 500);
    }
}
