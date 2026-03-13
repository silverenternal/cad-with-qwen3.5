//! 批处理进度管理模块
//!
//! 支持：
//! - 进度持久化到 JSON 文件
//! - 断点续传
//! - 批次状态跟踪

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::fs;
use std::io::Write;
use chrono::{DateTime, Utc};
use tracing::{info, warn, error};

/// 批处理进度记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchProgress {
    /// 版本号，用于格式升级
    pub version: u32,
    /// 开始时间
    pub started_at: DateTime<Utc>,
    /// 最后更新时间
    pub updated_at: DateTime<Utc>,
    /// 总 PDF 数量
    pub total_pdfs: usize,
    /// 总页数
    pub total_pages: usize,
    /// 已处理 PDF 数量
    pub processed_pdfs: usize,
    /// 已处理页数
    pub processed_pages: usize,
    /// 当前批次号
    pub current_batch: usize,
    /// 批次计划
    pub batches: Vec<BatchPlan>,
    /// 输出文件路径
    pub output_path: Option<PathBuf>,
}

/// 批次计划
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchPlan {
    /// 批次 ID
    pub batch_id: usize,
    /// 包含的 PDF 文件
    pub pdfs: Vec<PathBuf>,
    /// 批次状态
    pub status: BatchStatus,
    /// 开始时间
    pub started_at: Option<DateTime<Utc>>,
    /// 完成时间
    pub completed_at: Option<DateTime<Utc>>,
    /// 结果文件路径
    pub results_file: Option<PathBuf>,
    /// 失败信息（如果有）
    pub error: Option<String>,
}

/// 批次状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "state", content = "details")]
pub enum BatchStatus {
    /// 待处理
    Pending,
    /// 处理中
    Processing { current_pdf: String },
    /// 已完成
    Completed { result_file: PathBuf },
    /// 失败
    Failed { error: String, failed_pdf: String },
    /// 跳过（配额不足等）
    Skipped { reason: String },
}

impl BatchProgress {
    /// 创建新的进度记录
    pub fn new(total_pdfs: usize, total_pages: usize, batches: Vec<BatchPlan>) -> Self {
        let now = Utc::now();
        Self {
            version: 1,
            started_at: now,
            updated_at: now,
            total_pdfs,
            total_pages,
            processed_pdfs: 0,
            processed_pages: 0,
            current_batch: 0,
            batches,
            output_path: None,
        }
    }

    /// 从文件加载进度
    pub fn load_from_file(path: &Path) -> Option<Self> {
        if !path.exists() {
            return None;
        }

        match fs::read_to_string(path) {
            Ok(content) => {
                match serde_json::from_str::<Self>(&content) {
                    Ok(progress) => {
                        info!("成功加载进度文件：{} (已处理 {}/{} PDF)", 
                              path.display(), progress.processed_pdfs, progress.total_pdfs);
                        Some(progress)
                    }
                    Err(e) => {
                        warn!("解析进度文件失败 {}: {}", path.display(), e);
                        None
                    }
                }
            }
            Err(e) => {
                warn!("读取进度文件失败 {}: {}", path.display(), e);
                None
            }
        }
    }

    /// 保存到文件（原子写入）
    pub fn save_to_file(&self, path: &Path) -> std::result::Result<(), std::io::Error> {
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

    /// 更新进度
    pub fn update(&mut self) {
        self.updated_at = Utc::now();
    }

    /// 标记批次开始
    pub fn mark_batch_started(&mut self, batch_id: usize, current_pdf: &str) {
        if let Some(batch) = self.batches.iter_mut().find(|b| b.batch_id == batch_id) {
            batch.status = BatchStatus::Processing {
                current_pdf: current_pdf.to_string(),
            };
            batch.started_at = Some(Utc::now());
            self.current_batch = batch_id;
            self.update();
        }
    }

    /// 标记批次完成
    pub fn mark_batch_completed(&mut self, batch_id: usize, result_file: PathBuf, pdfs_processed: usize, pages_processed: usize) {
        if let Some(batch) = self.batches.iter_mut().find(|b| b.batch_id == batch_id) {
            batch.status = BatchStatus::Completed {
                result_file: result_file.clone(),
            };
            batch.completed_at = Some(Utc::now());
            batch.results_file = Some(result_file);
            self.processed_pdfs += pdfs_processed;
            self.processed_pages += pages_processed;
            self.update();
        }
    }

    /// 标记批次失败
    pub fn mark_batch_failed(&mut self, batch_id: usize, error: String, failed_pdf: String) {
        if let Some(batch) = self.batches.iter_mut().find(|b| b.batch_id == batch_id) {
            batch.status = BatchStatus::Failed {
                error: error.clone(),
                failed_pdf: failed_pdf.clone(),
            };
            batch.error = Some(error);
            self.update();
        }
    }

    /// 获取下一个待处理的批次
    pub fn next_pending_batch(&self) -> Option<&BatchPlan> {
        self.batches.iter()
            .find(|b| matches!(b.status, BatchStatus::Pending))
    }

    /// 获取下一个待处理或处理中的批次（用于恢复）
    pub fn next_incomplete_batch(&mut self) -> Option<&mut BatchPlan> {
        self.batches.iter_mut()
            .find(|b| matches!(b.status, BatchStatus::Pending | BatchStatus::Processing { .. }))
    }

    /// 检查是否全部完成
    pub fn is_complete(&self) -> bool {
        self.batches.iter().all(|b| matches!(b.status, BatchStatus::Completed { .. }))
    }

    /// 获取处理百分比
    pub fn progress_percent(&self) -> f64 {
        if self.total_pdfs == 0 {
            return 0.0;
        }
        (self.processed_pdfs as f64 / self.total_pdfs as f64) * 100.0
    }
}

/// 进度文件守卫（RAII 模式，自动保存）
pub struct ProgressGuard {
    progress: BatchProgress,
    path: PathBuf,
}

impl ProgressGuard {
    pub fn new(progress: BatchProgress, path: PathBuf) -> Self {
        Self { progress, path }
    }

    pub fn progress(&self) -> &BatchProgress {
        &self.progress
    }

    pub fn progress_mut(&mut self) -> &mut BatchProgress {
        &mut self.progress
    }

    pub fn save(&self) -> std::result::Result<(), std::io::Error> {
        self.progress.save_to_file(&self.path)
    }
}

impl Drop for ProgressGuard {
    fn drop(&mut self) {
        // 自动保存进度
        if let Err(e) = self.save() {
            error!("自动保存进度失败 {}: {}", self.path.display(), e);
        } else {
            info!("进度已自动保存到 {}", self.path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_progress_save_load() {
        let temp_dir = std::env::temp_dir();
        let test_path = temp_dir.join("test_progress.json");

        let batches = vec![
            BatchPlan {
                batch_id: 1,
                pdfs: vec![PathBuf::from("a.pdf"), PathBuf::from("b.pdf")],
                status: BatchStatus::Pending,
                started_at: None,
                completed_at: None,
                results_file: None,
                error: None,
            }
        ];

        let progress = BatchProgress::new(2, 10, batches);
        
        // 保存
        progress.save_to_file(&test_path).unwrap();
        
        // 加载
        let loaded = BatchProgress::load_from_file(&test_path).unwrap();
        
        assert_eq!(loaded.total_pdfs, 2);
        assert_eq!(loaded.total_pages, 10);
        assert_eq!(loaded.batches.len(), 1);

        // 清理
        let _ = fs::remove_file(&test_path);
    }
}
