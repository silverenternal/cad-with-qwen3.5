//! PDF 转换服务 trait

use crate::domain::DomainResult;

/// PDF 转换服务 trait - 定义 PDF 转图片的核心业务逻辑
#[async_trait::async_trait]
pub trait PdfConversionService: Send + Sync {
    /// 将 PDF 数据转换为图片
    /// 
    /// # 参数
    /// * `pdf_data` - PDF 文件的二进制数据
    /// * `dpi` - 转换 DPI（分辨率）
    /// 
    /// # 返回
    /// 返回转换后的图片数据列表（每页一张图片）
    async fn convert_pdf_to_images(
        &self,
        pdf_data: &[u8],
        dpi: u32,
    ) -> DomainResult<Vec<Vec<u8>>>;

    /// 判断数据是否为 PDF 格式
    fn is_pdf_data(&self, data: &[u8]) -> bool;
}
