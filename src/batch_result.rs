//! 批量处理结果数据结构

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

/// 单个文件的处理状态
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum FileStatus {
    /// 处理成功
    Success {
        /// 识别结果
        answer: String,
        /// 延迟（毫秒）
        latency_ms: u64,
    },
    /// 处理失败
    Failed {
        /// 错误信息
        error: String,
    },
}

/// 单个文件的处理结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileResult {
    /// 文件路径（相对路径）
    pub file: String,
    /// 图纸类型
    pub drawing_type: String,
    /// 问题
    pub question: String,
    /// 处理状态
    #[serde(flatten)]
    pub status: FileStatus,
}

impl FileResult {
    pub fn success(file: String, drawing_type: String, question: String, answer: String, latency_ms: u64) -> Self {
        Self {
            file,
            drawing_type,
            question,
            status: FileStatus::Success { answer, latency_ms },
        }
    }

    pub fn failed(file: String, drawing_type: String, question: String, error: String) -> Self {
        Self {
            file,
            drawing_type,
            question,
            status: FileStatus::Failed { error },
        }
    }

    /// 是否成功
    pub fn is_success(&self) -> bool {
        matches!(self.status, FileStatus::Success { .. })
    }

    /// 获取答案（如果成功）
    pub fn answer(&self) -> Option<&str> {
        match &self.status {
            FileStatus::Success { answer, .. } => Some(answer),
            _ => None,
        }
    }

    /// 获取错误信息（如果失败）
    pub fn error(&self) -> Option<&str> {
        match &self.status {
            FileStatus::Failed { error } => Some(error),
            _ => None,
        }
    }
}

/// 批量处理结果汇总
/// 
/// 注意：此结构体设计为在单线程中被修改（通过 channel 串行化结果）
/// 如需多线程安全访问，请使用 Arc<Mutex<BatchResult>>
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResult {
    /// 批量任务 ID
    pub batch_id: String,
    /// 开始时间
    pub started_at: DateTime<Utc>,
    /// 完成时间
    pub completed_at: DateTime<Utc>,
    /// 总文件数
    pub total: usize,
    /// 成功数量
    pub success: usize,
    /// 失败数量
    pub failed: usize,
    /// 详细结果列表
    pub results: Vec<FileResult>,
    /// 额外统计信息
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub stats: HashMap<String, String>,
    /// 进度文件路径（用于断点续传，不序列化）
    #[serde(skip)]
    pub progress_file: Option<PathBuf>,
}

/// 线程安全的批量结果包装器
pub struct SafeBatchResult {
    inner: Arc<Mutex<BatchResult>>,
}

impl SafeBatchResult {
    /// 创建新的安全批量结果
    pub fn new(result: BatchResult) -> Self {
        Self {
            inner: Arc::new(Mutex::new(result)),
        }
    }

    /// 获取内部 Arc（用于克隆传递给任务）
    pub fn clone_inner(&self) -> Arc<Mutex<BatchResult>> {
        Arc::clone(&self.inner)
    }

    /// 添加结果（线程安全）
    pub async fn add_result(&self, result: FileResult) {
        let mut guard = self.inner.lock().await;
        guard.add_result(result);
    }

    /// 完成处理（线程安全）
    pub async fn finish(&self) {
        let mut guard = self.inner.lock().await;
        guard.finish();
    }

    /// 获取最终结果（消耗 self）
    pub async fn into_inner(self) -> BatchResult {
        match Arc::try_unwrap(self.inner) {
            Ok(mutex) => mutex.into_inner(),
            Err(arc) => {
                tracing::warn!("SafeBatchResult: Arc has multiple owners, cloning instead of unwrapping");
                arc.lock().await.clone()
            }
        }
    }
}

impl Clone for SafeBatchResult {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl BatchResult {
    /// 创建新的批量结果
    pub fn new(batch_id: String, started_at: DateTime<Utc>) -> Self {
        Self {
            batch_id,
            started_at,
            completed_at: started_at,
            total: 0,
            success: 0,
            failed: 0,
            results: Vec::new(),
            stats: HashMap::new(),
            progress_file: None,
        }
    }

    /// 设置进度文件路径
    pub fn with_progress_file(mut self, path: PathBuf) -> Self {
        self.progress_file = Some(path);
        self
    }

    /// 添加单个结果（并保存到进度文件）
    pub fn add_result(&mut self, result: FileResult) {
        self.total += 1;
        if result.is_success() {
            self.success += 1;
        } else {
            self.failed += 1;
        }
        self.results.push(result);
        self.completed_at = Utc::now();
        
        // 增量保存到进度文件
        self.save_progress();
    }

    /// 保存进度到文件
    fn save_progress(&self) {
        if let Some(path) = &self.progress_file {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            match std::fs::write(path, serde_json::to_string_pretty(self).unwrap_or_default()) {
                Ok(_) => tracing::debug!("Progress saved: {}", path.display()),
                Err(e) => tracing::warn!("Failed to save progress: {} - {}", path.display(), e),
            }
        }
    }

    /// 从文件加载进度
    pub fn load_from_file(path: &std::path::Path) -> Option<Self> {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                match serde_json::from_str::<Self>(&content) {
                    Ok(mut result) => {
                        result.progress_file = Some(path.to_path_buf());
                        Some(result)
                    }
                    Err(e) => {
                        tracing::warn!("Failed to parse progress file: {} - {}", path.display(), e);
                        None
                    }
                }
            }
            Err(_) => None,
        }
    }

    /// 完成处理（移除进度文件）
    pub fn finish(&mut self) {
        self.completed_at = Utc::now();

        // 计算统计信息
        let total_latency: u64 = self.results.iter()
            .filter_map(|r| match &r.status {
                FileStatus::Success { latency_ms, .. } => Some(*latency_ms),
                _ => None,
            })
            .sum();

        let success_count = self.success as u64;
        if success_count > 0 {
            self.stats.insert(
                "avg_latency_ms".to_string(),
                (total_latency / success_count).to_string(),
            );
        }

        if self.total > 0 {
            self.stats.insert(
                "success_rate".to_string(),
                format!("{:.2}", (self.success as f64 / self.total as f64) * 100.0),
            );
        }

        self.save_progress();

        // 删除进度文件（已完成）
        if let Some(path) = &self.progress_file {
            let _ = std::fs::remove_file(path);
            tracing::debug!("Progress file removed: {}", path.display());
        }
    }

    /// 保存到文件（原子写入）
    pub fn save_to_file(&self, path: &Path) -> std::result::Result<(), std::io::Error> {
        use std::fs;
        use std::io::Write;

        // 原子写入：先写临时文件，再重命名
        let tmp_path = path.with_extension("tmp");
        
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        
        let mut file = fs::File::create(&tmp_path)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;  // 确保数据刷入磁盘
        
        fs::rename(&tmp_path, path)?;
        
        Ok(())
    }

    /// 生成新的批量 ID
    pub fn generate_id() -> String {
        Uuid::new_v4().to_string()
    }
}

/// 输出格式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    #[default]
    Json,
    Csv,
}

impl OutputFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            OutputFormat::Json => "json",
            OutputFormat::Csv => "csv",
        }
    }
}

impl std::str::FromStr for OutputFormat {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_lowercase().as_str() {
            "csv" => OutputFormat::Csv,
            _ => OutputFormat::Json,
        })
    }
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputFormat::Json => write!(f, "json"),
            OutputFormat::Csv => write!(f, "csv"),
        }
    }
}
