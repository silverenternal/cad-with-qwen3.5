//! 批处理结果合并模块
//!
//! 功能：
//! - 合并多个批次结果
//! - 生成最终 JSON
//! - 原子写入

use crate::batch_result::{BatchResult, FileResult, FileStatus};
use crate::error::{Result, Error};
use std::path::{Path, PathBuf};
use std::fs;
use std::io::Write;
use chrono::{DateTime, Utc};
use tracing::{info, warn};

/// 最终结果
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FinalResult {
    /// 元数据
    pub metadata: FinalMetadata,
    /// 所有结果
    pub results: Vec<FileResult>,
}

/// 最终结果元数据
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FinalMetadata {
    /// 生成时间
    pub generated_at: DateTime<Utc>,
    /// 总 PDF 数量
    pub total_pdfs: usize,
    /// 总页数
    pub total_pages: usize,
    /// 处理时间（秒）
    pub processing_time_seconds: f64,
    /// 成功数量
    pub success_count: usize,
    /// 失败数量
    pub failed_count: usize,
    /// 批次数量
    pub batch_count: usize,
}

impl FinalResult {
    /// 创建新的最终结果
    pub fn new() -> Self {
        Self {
            metadata: FinalMetadata {
                generated_at: Utc::now(),
                total_pdfs: 0,
                total_pages: 0,
                processing_time_seconds: 0.0,
                success_count: 0,
                failed_count: 0,
                batch_count: 0,
            },
            results: Vec::new(),
        }
    }

    /// 从批次结果加载
    pub fn from_batch_results(
        batch_results: &[PathBuf],
        started_at: DateTime<Utc>,
    ) -> Result<Self> {
        let mut final_result = Self::new();
        let mut unique_files: std::collections::HashSet<String> = std::collections::HashSet::new();

        for batch_file in batch_results {
            if !batch_file.exists() {
                warn!("批次结果文件不存在：{}", batch_file.display());
                continue;
            }

            match Self::load_batch_result(batch_file) {
                Ok(batch) => {
                    // 合并结果（去重）
                    for item in batch.results {
                        if !unique_files.contains(&item.file) {
                            unique_files.insert(item.file.clone());
                            final_result.results.push(item);
                        } else {
                            info!("跳过重复结果：{}", item.file);
                        }
                    }

                    // 更新统计
                    final_result.metadata.batch_count += 1;
                }
                Err(e) => {
                    warn!("加载批次结果失败 {}: {}", batch_file.display(), e);
                }
            }
        }

        // 计算统计信息
        final_result.metadata.total_pages = final_result.results.len();
        final_result.metadata.processing_time_seconds = 
            (Utc::now() - started_at).num_seconds() as f64;
        final_result.metadata.success_count = final_result.results
            .iter()
            .filter(|r| matches!(r.status, FileStatus::Success { .. }))
            .count();
        final_result.metadata.failed_count = final_result.results
            .iter()
            .filter(|r| matches!(r.status, FileStatus::Failed { .. }))
            .count();

        // 估算 PDF 数量（假设每 PDF 平均 5 页）
        final_result.metadata.total_pdfs = (final_result.metadata.total_pages as f64 / 5.0).ceil() as usize;

        Ok(final_result)
    }

    /// 加载单个批次结果
    fn load_batch_result(path: &Path) -> Result<BatchResult> {
        let content = fs::read_to_string(path)
            .map_err(|e| Error::Internal(e.to_string()))?;

        let batch: BatchResult = serde_json::from_str(&content)
            .map_err(|e| Error::Internal(format!("JSON 解析错误：{}", e)))?;

        Ok(batch)
    }

    /// 保存到文件（原子写入）
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        // 原子写入：先写临时文件，再重命名
        let tmp_path = path.with_extension("tmp");

        let json = serde_json::to_string_pretty(self)
            .map_err(|e| Error::Internal(format!("JSON 序列化错误：{}", e)))?;

        let mut file = fs::File::create(&tmp_path)
            .map_err(|e| Error::Internal(e.to_string()))?;
        file.write_all(json.as_bytes())
            .map_err(|e| Error::Internal(e.to_string()))?;
        file.sync_all()
            .map_err(|e| Error::Internal(e.to_string()))?;

        fs::rename(&tmp_path, path)
            .map_err(|e| Error::Internal(e.to_string()))?;

        info!("最终结果已保存到 {}", path.display());

        Ok(())
    }
}

impl Default for FinalResult {
    fn default() -> Self {
        Self::new()
    }
}

/// 临时目录守卫（RAII 模式，自动清理）
pub struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        if let Err(e) = fs::remove_dir_all(&self.path) {
            warn!("清理临时目录失败 {}: {}", self.path.display(), e);
        } else {
            info!("临时目录已清理：{}", self.path.display());
        }
    }
}

/// 创建临时目录
pub fn create_temp_dir(prefix: &str) -> Result<PathBuf> {
    let temp_dir = std::env::temp_dir().join(format!("{}_{}", prefix, Utc::now().timestamp()));
    fs::create_dir_all(&temp_dir)
        .map_err(|e| Error::Internal(e.to_string()))?;
    Ok(temp_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_final_result_save_load() {
        let temp_dir = std::env::temp_dir();
        let test_path = temp_dir.join("test_final_result.json");

        let mut final_result = FinalResult::new();
        final_result.results.push(FileResult::success(
            "test.pdf".to_string(),
            "building_plan".to_string(),
            "分析这张图纸".to_string(),
            "测试分析".to_string(),
            100,
        ));
        final_result.metadata.total_pages = 1;
        final_result.metadata.success_count = 1;

        // 保存
        final_result.save_to_file(&test_path).unwrap();

        // 加载验证
        let content = fs::read_to_string(&test_path).unwrap();
        let loaded: FinalResult = serde_json::from_str(&content).unwrap();

        assert_eq!(loaded.results.len(), 1);
        assert_eq!(loaded.results[0].file, "test.pdf");

        // 清理
        let _ = fs::remove_file(&test_path);
    }
}
