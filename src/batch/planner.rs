//! 批处理计划生成模块
//!
//! 功能：
//! - 扫描 PDF 目录
//! - 快速统计页数（不转换）
//! - 生成批次计划
//! - 配额预检

use crate::error::Result;
use crate::error::Error;
use std::path::{Path, PathBuf};
use std::fs;
use tracing::{info, warn};

/// 处理计划
#[derive(Debug, Clone)]
pub struct ProcessingPlan {
    /// 总 PDF 数量
    pub total_pdfs: usize,
    /// 总页数
    pub total_pages: usize,
    /// 预估 API 调用次数（通常等于页数）
    pub estimated_api_calls: usize,
    /// 所需配额
    pub required_quota: usize,
    /// 可用配额
    pub available_quota: u32,
    /// 是否可行
    pub is_feasible: bool,
    /// 批次计划
    pub batches: Vec<BatchPlan>,
    /// PDF 文件列表（带页数）
    pub pdf_files: Vec<PdfInfo>,
}

/// PDF 文件信息
#[derive(Debug, Clone)]
pub struct PdfInfo {
    pub path: PathBuf,
    pub page_count: usize,
}

/// 批次计划
#[derive(Debug, Clone)]
pub struct BatchPlan {
    pub batch_id: usize,
    pub pdfs: Vec<PdfInfo>,
    pub total_pages: usize,
}

/// 批处理配置
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// 每批 PDF 数量
    pub pdfs_per_batch: usize,
    /// 每 PDF 最大页数（0=无限制）
    pub max_pages_per_pdf: usize,
    /// 并发处理 PDF 数量
    pub concurrency: usize,
    /// 是否检查配额
    pub enable_quota_check: bool,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            pdfs_per_batch: 5,
            max_pages_per_pdf: 0,
            concurrency: 2,
            enable_quota_check: true,
        }
    }
}

/// 创建处理计划
pub async fn create_processing_plan(
    pdf_dir: &Path,
    available_quota: u32,
    config: &BatchConfig,
) -> Result<ProcessingPlan> {
    info!("扫描 PDF 目录：{}", pdf_dir.display());

    // 扫描所有 PDF 文件
    let pdf_files = scan_pdfs(pdf_dir, config.max_pages_per_pdf)?;

    if pdf_files.is_empty() {
        return Err(Error::Validation(format!(
            "目录中没有找到 PDF 文件：{}",
            pdf_dir.display()
        )));
    }

    info!("找到 {} 个 PDF 文件", pdf_files.len());

    // 计算总页数
    let total_pages: usize = pdf_files.iter().map(|p| p.page_count).sum();
    let total_pdfs = pdf_files.len();

    info!("总计 {} 页", total_pages);

    // 计算配额需求
    let required_quota = total_pages;
    let is_feasible = !config.enable_quota_check || (required_quota as u32 <= available_quota);

    if !is_feasible {
        warn!("配额不足：需要 {} 次，可用 {} 次", required_quota, available_quota);
    }

    // 生成批次计划
    let batches = create_batch_plan(&pdf_files, config.pdfs_per_batch);

    Ok(ProcessingPlan {
        total_pdfs,
        total_pages,
        estimated_api_calls: total_pages,
        required_quota,
        available_quota,
        is_feasible,
        batches,
        pdf_files,
    })
}

/// 扫描目录中的 PDF 文件
fn scan_pdfs(dir: &Path, max_pages_per_pdf: usize) -> Result<Vec<PdfInfo>> {
    let mut pdf_files = Vec::new();

    if !dir.exists() {
        return Err(Error::Validation(format!("目录不存在：{}", dir.display())));
    }

    for entry in fs::read_dir(dir).map_err(|e| {
        Error::Validation(format!("无法读取目录 {}: {}", dir.display(), e))
    })? {
        let entry = entry.map_err(|e| {
            Error::Internal(format!("读取目录条目失败：{}", e))
        })?;

        let path = entry.path();
        
        if path.is_file() && path.extension().map_or(false, |ext| ext.eq_ignore_ascii_case("pdf")) {
            match quick_count_pages(&path) {
                Ok(count) => {
                    let count = if max_pages_per_pdf > 0 && count > max_pages_per_pdf {
                        info!("PDF {} 有 {} 页，限制为 {} 页", path.display(), count, max_pages_per_pdf);
                        max_pages_per_pdf
                    } else {
                        count
                    };
                    
                    pdf_files.push(PdfInfo {
                        path,
                        page_count: count,
                    });
                }
                Err(e) => {
                    warn!("跳过无法读取的 PDF {}: {}", path.display(), e);
                }
            }
        }
    }

    // 按文件名排序，确保处理顺序一致
    pdf_files.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(pdf_files)
}

/// 快速统计 PDF 页数（不转换，只读取元数据）
fn quick_count_pages(pdf_path: &Path) -> Result<usize> {
    // 使用 pdfinfo 快速读取页数（poppler 工具）
    if let Ok(output) = std::process::Command::new("pdfinfo")
        .arg(pdf_path)
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.starts_with("Pages:") {
                    if let Some(pages_str) = line.split(':').nth(1) {
                        if let Ok(pages) = pages_str.trim().parse::<usize>() {
                            return Ok(pages);
                        }
                    }
                }
            }
        }
    }

    // 备用方法：尝试 pdftoppm -info（如果可用）
    if let Ok(output) = std::process::Command::new("pdftoppm")
        .arg("-info")
        .arg(pdf_path)
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.starts_with("Pages:") {
                    if let Some(pages_str) = line.split(':').nth(1) {
                        if let Ok(pages) = pages_str.trim().parse::<usize>() {
                            return Ok(pages);
                        }
                    }
                }
            }
        }
    }

    // 备用方法：尝试使用 qpdf
    if let Ok(output) = std::process::Command::new("qpdf")
        .arg("--show-npages")
        .arg(pdf_path)
        .output()
    {
        if output.status.success() {
            if let Ok(pages) = String::from_utf8_lossy(&output.stdout).trim().parse::<usize>() {
                return Ok(pages);
            }
        }
    }

    // 如果所有外部工具都失败，返回一个估计值（1 页）
    // 这允许批处理继续，但会在日志中警告
    tracing::warn!("无法读取 PDF {} 的页数，外部工具不可用。请安装 pdfinfo（poppler 工具）或 qpdf", pdf_path.display());
    tracing::warn!("假设 PDF 有 1 页，实际处理时可能会不准确");
    Ok(1)
}

/// 生成批次计划
fn create_batch_plan(pdf_files: &[PdfInfo], pdfs_per_batch: usize) -> Vec<BatchPlan> {
    let mut batches = Vec::new();
    let mut batch_id = 1;

    for chunk in pdf_files.chunks(pdfs_per_batch) {
        let total_pages: usize = chunk.iter().map(|p| p.page_count).sum();
        
        batches.push(BatchPlan {
            batch_id,
            pdfs: chunk.to_vec(),
            total_pages,
        });
        
        batch_id += 1;
    }

    info!("生成 {} 个批次计划", batches.len());

    batches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_plan_creation() {
        let pdf_files = vec![
            PdfInfo { path: PathBuf::from("a.pdf"), page_count: 5 },
            PdfInfo { path: PathBuf::from("b.pdf"), page_count: 3 },
            PdfInfo { path: PathBuf::from("c.pdf"), page_count: 7 },
            PdfInfo { path: PathBuf::from("d.pdf"), page_count: 2 },
            PdfInfo { path: PathBuf::from("e.pdf"), page_count: 4 },
        ];

        let batches = create_batch_plan(&pdf_files, 2);
        
        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0].pdfs.len(), 2);
        assert_eq!(batches[0].total_pages, 8);
        assert_eq!(batches[1].pdfs.len(), 2);
        assert_eq!(batches[1].total_pages, 9);
        assert_eq!(batches[2].pdfs.len(), 1);
        assert_eq!(batches[2].total_pages, 4);
    }
}
