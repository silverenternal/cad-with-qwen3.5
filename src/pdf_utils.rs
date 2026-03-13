//! PDF 工具模块 - 提供诊断和修复功能

use std::path::Path;
use std::process::Command;
use tracing::{info, warn};

/// PDF 诊断结果
#[derive(Debug, Clone)]
pub struct PdfDiagnostic {
    /// 文件路径
    pub path: String,
    /// 是否有效的 PDF
    pub is_valid_pdf: bool,
    /// 是否加密
    pub is_encrypted: bool,
    /// 页数
    pub page_count: Option<usize>,
    /// PDF 版本
    pub pdf_version: Option<String>,
    /// 错误信息
    pub errors: Vec<String>,
    /// 警告信息
    pub warnings: Vec<String>,
}

/// 诊断 PDF 文件
pub fn diagnose_pdf(pdf_path: &Path) -> PdfDiagnostic {
    let mut result = PdfDiagnostic {
        path: pdf_path.display().to_string(),
        is_valid_pdf: false,
        is_encrypted: false,
        page_count: None,
        pdf_version: None,
        errors: Vec::new(),
        warnings: Vec::new(),
    };

    // 检查文件是否存在
    if !pdf_path.exists() {
        result.errors.push("文件不存在".to_string());
        return result;
    }

    // 检查文件扩展名
    if pdf_path.extension().map_or(true, |ext| ext.to_ascii_lowercase() != "pdf") {
        result.warnings.push("文件扩展名不是 .pdf".to_string());
    }

    // 检查文件头魔数
    match std::fs::read(pdf_path) {
        Ok(data) => {
            if data.len() < 4 || &data[0..4] != b"%PDF" {
                result.errors.push("不是有效的 PDF 文件（缺少 %PDF 头）".to_string());
                return result;
            }

            // 提取 PDF 版本
            if data.len() >= 8 {
                let version_str = String::from_utf8_lossy(&data[4..8]);
                if version_str.starts_with('-') {
                    result.pdf_version = Some(version_str.trim().to_string());
                }
            }

            // 检查是否加密
            if data.windows(5).any(|w| w == b"ENCRYPT") || 
               data.windows(4).any(|w| w == b"/R ") ||
               data.windows(8).any(|w| w == b"/Encrypt") {
                result.is_encrypted = true;
                result.errors.push("PDF 已加密，无法转换".to_string());
            }
        }
        Err(e) => {
            result.errors.push(format!("无法读取文件：{}", e));
            return result;
        }
    }

    // 使用 pdftoppm 获取页数
    let temp_prefix = std::env::temp_dir().join(format!("pdf_diag_{}", uuid::Uuid::new_v4()));
    let pdf_path_str = pdf_path.to_str().unwrap_or("");
    let temp_prefix_str = temp_prefix.to_str().unwrap_or("");

    match Command::new("D:\\poppler-25.12.0\\Library\\bin\\pdftoppm.exe")
        .args(["-l", "1", "-r", "1", pdf_path_str, temp_prefix_str])
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                result.is_valid_pdf = true;

                // 尝试获取页数（通过 pdffonts 或 pdfinfo）
                result.page_count = get_pdf_page_count(pdf_path);

                // 检查是否有警告
                let stderr = String::from_utf8_lossy(&output.stderr);
                if !stderr.is_empty() {
                    result.warnings.push(format!("pdftoppm 警告：{}", stderr.trim()));
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                result.errors.push(format!("pdftoppm 失败：{}", stderr.trim()));

                // 分析错误信息
                if stderr.contains("encrypted") {
                    result.is_encrypted = true;
                } else if stderr.contains("damaged") || stderr.contains("corrupt") {
                    result.errors.push("PDF 文件可能已损坏".to_string());
                } else if stderr.contains("missing") {
                    result.errors.push("PDF 缺少必要的数据".to_string());
                }
            }
        }
        Err(e) => {
            result.errors.push(format!("无法执行 pdftoppm: {}", e));
        }
    }

    // 清理临时文件
    let _ = std::fs::remove_dir_all(temp_prefix.parent().unwrap_or(&std::env::temp_dir()));

    result
}

/// 获取 PDF 页数
fn get_pdf_page_count(pdf_path: &Path) -> Option<usize> {
    // 尝试使用 pdfinfo
    let pdf_path_str = pdf_path.to_str()?;
    
    if let Ok(output) = Command::new("D:\\poppler-25.12.0\\Library\\bin\\pdfinfo.exe")
        .arg(pdf_path_str)
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.starts_with("Pages:") {
                    if let Some(count_str) = line.split(':').nth(1) {
                        if let Ok(count) = count_str.trim().parse::<usize>() {
                            return Some(count);
                        }
                    }
                }
            }
        }
    }

    // 备用方法：尝试 pdftoppm 输出
    None
}

/// PDF 转换配置
#[derive(Debug, Clone)]
pub struct PdfConvertConfig {
    /// DPI（默认 150）
    pub dpi: u32,
    /// 输出格式（jpeg 或 png）
    pub format: String,
    /// 最大重试次数
    pub max_retries: u32,
    /// 是否尝试备选方案
    pub try_fallback: bool,
}

impl Default for PdfConvertConfig {
    fn default() -> Self {
        Self {
            dpi: 150,
            format: "jpeg".to_string(),
            max_retries: 2,
            try_fallback: true,
        }
    }
}

/// 健壮的 PDF 转换（带重试和备选方案）
pub async fn robust_pdf_convert(
    pdf_path: &Path,
    max_dimension: u32,
    config: &PdfConvertConfig,
) -> Result<Vec<String>, String> {
    // 先诊断
    let diagnostic = diagnose_pdf(pdf_path);
    
    if !diagnostic.is_valid_pdf {
        return Err(format!("无效的 PDF 文件：{}", diagnostic.errors.join(", ")));
    }

    if diagnostic.is_encrypted {
        return Err("PDF 已加密，无法转换".to_string());
    }

    // 尝试转换（带重试）
    let mut last_error = None;
    
    for attempt in 0..config.max_retries {
        // 尝试当前配置
        match try_convert_with_config(pdf_path, max_dimension, config).await {
            Ok(images) => {
                if images.is_empty() {
                    last_error = Some("转换后未生成任何图片".to_string());
                    continue;
                }
                info!("PDF 转换成功（尝试 {}/{}）: {} 页", attempt + 1, config.max_retries, images.len());
                return Ok(images);
            }
            Err(e) => {
                warn!("PDF 转换失败（尝试 {}/{}）: {}", attempt + 1, config.max_retries, e);
                last_error = Some(e);
            }
        }

        // 如果失败且允许备选方案，尝试不同的配置
        if config.try_fallback {
            let fallback_config = get_fallback_config(config, attempt);
            match try_convert_with_config(pdf_path, max_dimension, &fallback_config).await {
                Ok(images) => {
                    if !images.is_empty() {
                        info!("PDF 转换成功（备选方案）: {} 页", images.len());
                        return Ok(images);
                    }
                }
                Err(e) => {
                    warn!("备选方案失败：{}", e);
                }
            }
        }
    }

    Err(format!("所有尝试都失败：{}", last_error.unwrap_or_default()))
}

/// 尝试使用指定配置转换
async fn try_convert_with_config(
    pdf_path: &Path,
    max_dimension: u32,
    config: &PdfConvertConfig,
) -> Result<Vec<String>, String> {
    use tempfile::TempDir;
    use std::fs;

    let temp_dir = TempDir::new()
        .map_err(|e| format!("创建临时目录失败：{}", e))?;

    let output_prefix = temp_dir.path().join("page");
    let pdf_path_str = pdf_path.to_str()
        .ok_or("PDF 路径包含无效的 UTF-8 字符")?;
    let output_prefix_str = output_prefix.to_str()
        .ok_or("输出路径包含无效的 UTF-8 字符")?;

    // 构建命令
    let format_arg = match config.format.as_str() {
        "png" => "-png",
        _ => "-jpeg",
    };

    let result = Command::new("D:\\poppler-25.12.0\\Library\\bin\\pdftoppm.exe")
        .args(&[
            format_arg,
            "-r", &config.dpi.to_string(),
            pdf_path_str,
            output_prefix_str,
        ])
        .output();

    match result {
        Ok(output) if output.status.success() => {
            // 收集所有生成的文件
            let files: Vec<_> = fs::read_dir(temp_dir.path())
                .map(|dir| dir.filter_map(|e| e.ok()).collect())
                .unwrap_or_default();
            
            if files.is_empty() {
                return Err("未生成任何图片".to_string());
            }

            // 加载并压缩所有图片
            let mut images = Vec::new();
            for file_entry in &files {
                let file_path = file_entry.path();
                
                // 使用 cache.rs 中的函数压缩
                match super::cache::load_and_compress_image(&file_path, max_dimension, 85).await {
                    Ok(base64_data) => images.push(base64_data),
                    Err(e) => {
                        warn!("压缩图片失败 {}: {}", file_path.display(), e);
                        // 继续处理其他图片
                    }
                }
            }

            // 按文件名排序
            images.sort_by(|a, b| a.len().cmp(&b.len()));

            Ok(images)
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("pdftoppm 失败：{}", stderr))
        }
        Err(e) => {
            Err(format!("执行 pdftoppm 失败：{}", e))
        }
    }
}

/// 获取备选配置
fn get_fallback_config(base: &PdfConvertConfig, attempt: u32) -> PdfConvertConfig {
    match attempt {
        0 => PdfConvertConfig {
            format: "png".to_string(),  // 尝试 PNG 格式
            ..base.clone()
        },
        1 => PdfConvertConfig {
            dpi: 200,  // 提高 DPI
            ..base.clone()
        },
        2 => PdfConvertConfig {
            dpi: 100,  // 降低 DPI
            ..base.clone()
        },
        _ => base.clone(),
    }
}

/// 打印 PDF 诊断报告
pub fn print_diagnostic_report(diagnostic: &PdfDiagnostic) {
    println!("\n═══════════════════════════════════════════════════════════");
    println!("  📋 PDF 诊断报告");
    println!("═══════════════════════════════════════════════════════════");
    println!("文件：{}", diagnostic.path);
    println!();

    // 状态
    if diagnostic.is_valid_pdf {
        println!("✅ 有效的 PDF 文件");
    } else {
        println!("❌ 无效的 PDF 文件");
    }

    if diagnostic.is_encrypted {
        println!("⚠️  PDF 已加密");
    }

    // 页数
    if let Some(count) = diagnostic.page_count {
        println!("📄 页数：{}", count);
    }

    // 版本
    if let Some(version) = &diagnostic.pdf_version {
        println!("📝 PDF 版本：{}", version);
    }

    // 错误
    if !diagnostic.errors.is_empty() {
        println!("\n❌ 错误：");
        for error in &diagnostic.errors {
            println!("  - {}", error);
        }
    }

    // 警告
    if !diagnostic.warnings.is_empty() {
        println!("\n⚠️  警告：");
        for warning in &diagnostic.warnings {
            println!("  - {}", warning);
        }
    }

    println!("\n═══════════════════════════════════════════════════════════\n");
}
