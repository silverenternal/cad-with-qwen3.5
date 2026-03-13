//! 图片缓存模块 - 支持压缩和 PDF 转换

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use lru::LruCache;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{info, warn};

// 使用统一错误类型中的 CacheError
use crate::error::CacheError;

pub type Result<T> = std::result::Result<T, CacheError>;

/// 图片缓存管理器
pub struct ImageCache {
    cache: LruCache<String, String>,
    max_dimension: u32,
    jpeg_quality: u8,
    root_dir: PathBuf,
}

impl ImageCache {
    /// 创建新缓存
    pub fn new(max_entries: usize, _max_memory_mb: usize, max_dimension: u32, jpeg_quality: u8, root_dir: PathBuf) -> Result<Self> {
        let max_entries = NonZeroUsize::new(max_entries)
            .ok_or(CacheError::InvalidSize(max_entries))?;

        Ok(Self {
            cache: LruCache::new(max_entries),
            max_dimension,
            jpeg_quality,
            root_dir,
        })
    }

    /// 获取或加载图片（自动压缩，支持 PDF 转换）
    pub async fn get_or_load(&mut self, path: &str) -> Result<String> {
        if let Some(data) = self.cache.get(path) {
            return Ok(data.clone());
        }

        // 安全校验路径，防止路径遍历攻击
        let safe_path = crate::security::sanitize_path(&self.root_dir, path)?;

        // 检测是否为 PDF 文件
        let base64_data = if is_pdf_file(&safe_path) {
            info!("检测到 PDF 文件，正在转换：{}", path);
            load_and_convert_pdf(&safe_path, self.max_dimension).await?
        } else {
            load_and_compress_image(&safe_path, self.max_dimension, 85).await?
        };

        self.cache.put(path.to_string(), base64_data.clone());
        Ok(base64_data)
    }

    /// 获取或加载 PDF 所有页（返回多张图片）
    pub async fn get_or_load_pdf(&mut self, path: &str) -> Result<Vec<String>> {
        // 安全校验路径
        let safe_path = crate::security::sanitize_path(&self.root_dir, path)?;

        if !is_pdf_file(&safe_path) {
            return Err(CacheError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "文件不是 PDF 格式",
            )));
        }

        info!("加载 PDF 所有页：{}", path);
        load_and_convert_pdf_all_pages(&safe_path, self.max_dimension).await
    }

    /// 清空缓存
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    /// 获取缓存统计信息
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            entry_count: self.cache.len(),
            max_entries: self.cache.cap().get(),
        }
    }

    /// 获取根目录
    pub fn root_dir(&self) -> &PathBuf {
        &self.root_dir
    }
}

/// 缓存统计信息
pub struct CacheStats {
    pub entry_count: usize,
    pub max_entries: usize,
}

impl std::fmt::Display for CacheStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{} 张图片", self.entry_count, self.max_entries)
    }
}

/// 加载图片并压缩为 JPEG（与 batch.rs 逻辑一致）
/// - 限制最大边长（默认 2048px）
/// - 减少内存占用约 60-80%
pub async fn load_and_compress_image(image_path: &Path, max_dimension: u32, _jpeg_quality: u8) -> Result<String> {
    if !image_path.exists() {
        return Err(CacheError::NotFound(image_path.display().to_string()));
    }

    // 读取图片
    let img = image::open(image_path)
        .map_err(|e| CacheError::ImageError(e.to_string()))?;

    // 限制最大尺寸（减少内存和带宽）
    let img = if img.width() > max_dimension || img.height() > max_dimension {
        img.thumbnail(max_dimension, max_dimension)
    } else {
        img
    };

    // 压缩为 JPEG
    let mut jpeg_buffer = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut jpeg_buffer);

    // 使用 image 库的 JPEG 编码
    img.write_to(&mut cursor, image::ImageFormat::Jpeg)
        .map_err(|e| CacheError::ImageError(e.to_string()))?;

    const MAX_FILE_SIZE: usize = 10 * 1024 * 1024;
    if jpeg_buffer.len() > MAX_FILE_SIZE {
        return Err(CacheError::IoError(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("图片文件过大（最大 {}MB）", MAX_FILE_SIZE / 1024 / 1024),
        )));
    }

    Ok(BASE64.encode(&jpeg_buffer))
}

/// 检测文件是否为 PDF
fn is_pdf_file(path: &Path) -> bool {
    if path.extension().map_or(false, |ext| ext.to_ascii_lowercase() == "pdf") {
        // 进一步检查文件头魔数
        if let Ok(data) = std::fs::read(path) {
            return data.len() >= 4 && &data[0..4] == b"%PDF";
        }
    }
    false
}

/// 使用 pdftoppm 转换 PDF 为 JPEG 图片（只返回第一页）
async fn load_and_convert_pdf(pdf_path: &Path, max_dimension: u32) -> Result<String> {
    if !pdf_path.exists() {
        return Err(CacheError::NotFound(pdf_path.display().to_string()));
    }

    // 尝试使用 pdftoppm 转换
    match convert_pdf_with_pdftoppm(pdf_path, max_dimension).await {
        Ok(base64_data) => return Ok(base64_data),
        Err(e) => {
            warn!("pdftoppm 转换失败：{}，尝试备选方案", e);
            // 继续尝试备选方案
        }
    }

    // 备选方案：使用 PDF 渲染库（需要 pdf2image 特性）
    #[cfg(feature = "pdf2image")]
    {
        match convert_pdf_with_pdf2image(pdf_path, max_dimension).await {
            Ok(base64_data) => return Ok(base64_data),
            Err(e) => {
                warn!("pdf2image 转换失败：{}", e);
            }
        }
    }

    // 所有方案都失败
    Err(CacheError::IoError(std::io::Error::new(
        std::io::ErrorKind::Other,
        "PDF 转换失败：pdftoppm 不可用且 pdf2image 特性未启用。请安装 poppler-utils (pdftoppm) 或启用 pdf2image 特性。",
    )))
}

/// 使用 pdftoppm 转换 PDF 所有页为 JPEG 图片
async fn load_and_convert_pdf_all_pages(pdf_path: &Path, max_dimension: u32) -> Result<Vec<String>> {
    if !pdf_path.exists() {
        return Err(CacheError::NotFound(pdf_path.display().to_string()));
    }

    // 尝试使用 pdftoppm 转换所有页
    match convert_pdf_all_pages_with_pdftoppm(pdf_path, max_dimension).await {
        Ok(base64_images) => return Ok(base64_images),
        Err(e) => {
            warn!("pdftoppm 多页转换失败：{}", e);
        }
    }

    // 备选方案：使用 PDF 渲染库（需要 pdf2image 特性）
    #[cfg(feature = "pdf2image")]
    {
        match convert_pdf_all_pages_with_pdf2image(pdf_path, max_dimension).await {
            Ok(base64_images) => return Ok(base64_images),
            Err(e) => {
                warn!("pdf2image 多页转换失败：{}", e);
            }
        }
    }

    // 所有方案都失败
    Err(CacheError::IoError(std::io::Error::new(
        std::io::ErrorKind::Other,
        "PDF 多页转换失败：pdftoppm 不可用且 pdf2image 特性未启用。请安装 poppler-utils (pdftoppm) 或启用 pdf2image 特性。",
    )))
}

/// 使用 pdftoppm 转换 PDF
async fn convert_pdf_with_pdftoppm(pdf_path: &Path, max_dimension: u32) -> Result<String> {
    use tempfile::TempDir;

    // 创建临时目录
    let temp_dir = TempDir::new()
        .map_err(|e| CacheError::IoError(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("创建临时目录失败：{}", e),
        )))?;

    let output_prefix = temp_dir.path().join("page");
    let pdf_path_str = pdf_path.to_str()
        .ok_or_else(|| CacheError::IoError(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "PDF 路径包含无效的 UTF-8 字符",
        )))?;
    let output_prefix_str = output_prefix.to_str()
        .ok_or_else(|| CacheError::IoError(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "输出路径包含无效的 UTF-8 字符",
        )))?;

    // 调用 pdftoppm 转换（只取第一页）
    let result = Command::new("D:\\poppler-25.12.0\\Library\\bin\\pdftoppm.exe")
        .args([
            "-jpeg",
            "-f", "1",        // 只转换第 1 页
            "-l", "1",        // 只转换第 1 页
            "-r", "150",      // 150 DPI
            pdf_path_str,
            output_prefix_str,
        ])
        .output();

    match result {
        Ok(output) if output.status.success() => {
            // 调试：列出临时目录中的所有文件
            use std::fs;
            let files: Vec<_> = fs::read_dir(temp_dir.path())
                .map(|dir| dir.filter_map(|e| e.ok()).collect())
                .unwrap_or_default();
            let file_names: Vec<String> = files.iter().map(|f| f.file_name().to_string_lossy().to_string()).collect();
            info!("pdftoppm 输出文件：{:?}", file_names);

            // 尝试多种可能的文件名格式（Windows 上可能是 page-1.jpg 或 page-0001.jpg）
            let possible_paths = [
                format!("{}-1.jpg", output_prefix_str),      // 标准格式
                format!("{}-0001.jpg", output_prefix_str),   // 带前导零
                format!("{}-1.jpeg", output_prefix_str),     // .jpeg 扩展名
                format!("{}-0001.jpeg", output_prefix_str),  // 带前导零 + .jpeg
            ];

            for image_path in &possible_paths {
                if std::path::Path::new(image_path).exists() {
                    // 读取并压缩图片
                    let base64_data = load_and_compress_image(
                        std::path::Path::new(image_path),
                        max_dimension,
                        85,
                    ).await?;

                    info!("PDF 转换成功：{} -> {}", pdf_path.display(), image_path);
                    return Ok(base64_data);
                }
            }

            // 所有可能的路径都不存在
            warn!("pdftoppm 执行成功但未找到输出文件，临时目录内容：{:?}", file_names);
            Err(CacheError::IoError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "PDF 转换后未找到生成的图片文件（已尝试：page-1.jpg, page-0001.jpg 等）",
            )))
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(CacheError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("pdftoppm 转换失败：{}", stderr),
            )))
        }
        Err(e) => {
            Err(CacheError::IoError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("pdftoppm 命令未找到：{}，请安装 poppler-utils", e),
            )))
        }
    }
}

/// 使用 pdf2image 转换 PDF（备选方案）
#[cfg(feature = "pdf2image")]
async fn convert_pdf_with_pdf2image(pdf_path: &Path, max_dimension: u32) -> Result<String> {
    use pdf2image::convert_pdf_to_images;
    use tempfile::TempDir;

    // 读取 PDF 数据
    let pdf_data = std::fs::read(pdf_path)
        .map_err(|e| CacheError::IoError(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("读取 PDF 文件失败：{}", e),
        )))?;

    // 使用 pdf2image 转换
    let images = convert_pdf_to_images(&pdf_data, 150)
        .map_err(|e| CacheError::IoError(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("pdf2image 转换失败：{}", e),
        )))?;

    if images.is_empty() {
        return Err(CacheError::IoError(std::io::Error::new(
            std::io::ErrorKind::Other,
            "PDF 转换后未生成任何图片",
        )));
    }

    // 取第一页
    let first_page = &images[0];

    // 创建临时文件保存转换结果
    let temp_dir = TempDir::new()
        .map_err(|e| CacheError::IoError(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("创建临时目录失败：{}", e),
        )))?;
    let temp_path = temp_dir.path().join("page-1.jpg");

    std::fs::write(&temp_path, first_page)
        .map_err(|e| CacheError::IoError(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("写入临时图片失败：{}", e),
        )))?;

    // 压缩并返回
    load_and_compress_image(&temp_path, max_dimension, 85).await
}

/// 使用 pdftoppm 转换 PDF 所有页为 JPEG 图片（带诊断和重试）
async fn convert_pdf_all_pages_with_pdftoppm(pdf_path: &Path, max_dimension: u32) -> Result<Vec<String>> {
    use crate::pdf_utils::{robust_pdf_convert, PdfConvertConfig};

    let config = PdfConvertConfig::default();
    
    match robust_pdf_convert(pdf_path, max_dimension, &config).await {
        Ok(images) => Ok(images),
        Err(e) => Err(CacheError::IoError(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("PDF 转换失败：{}", e),
        ))),
    }
}

/// 使用 pdf2image 转换 PDF 所有页（备选方案）
#[cfg(feature = "pdf2image")]
async fn convert_pdf_all_pages_with_pdf2image(pdf_path: &Path, max_dimension: u32) -> Result<Vec<String>> {
    use pdf2image::convert_pdf_to_images;
    use tempfile::TempDir;

    // 读取 PDF 数据
    let pdf_data = std::fs::read(pdf_path)
        .map_err(|e| CacheError::IoError(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("读取 PDF 文件失败：{}", e),
        )))?;

    // 使用 pdf2image 转换
    let images = convert_pdf_to_images(&pdf_data, 150)
        .map_err(|e| CacheError::IoError(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("pdf2image 转换失败：{}", e),
        )))?;

    if images.is_empty() {
        return Err(CacheError::IoError(std::io::Error::new(
            std::io::ErrorKind::Other,
            "PDF 转换后未生成任何图片",
        )));
    }

    // 转换所有页
    let mut base64_images: Vec<String> = Vec::new();
    for (idx, page_image) in images.iter().enumerate() {
        // 创建临时文件
        let temp_dir = TempDir::new()
            .map_err(|e| CacheError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("创建临时目录失败：{}", e),
            )))?;
        let temp_path = temp_dir.path().join(format!("page-{}.jpg", idx + 1));

        std::fs::write(&temp_path, page_image)
            .map_err(|e| CacheError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("写入临时图片失败：{}", e),
            )))?;

        // 压缩
        let base64_data = load_and_compress_image(&temp_path, max_dimension, 85).await?;
        base64_images.push(base64_data);
    }

    Ok(base64_images)
}
