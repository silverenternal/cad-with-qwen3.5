//! 分析服务 trait

use crate::domain::model::{Drawing, DrawingAnalysis, DrawingType};
use crate::domain::DomainResult;

/// 分析服务 trait - 定义图纸分析的核心业务逻辑
#[async_trait::async_trait]
pub trait AnalysisService: Send + Sync {
    /// 分析图纸
    async fn analyze_drawing(
        &self,
        drawing: Drawing,
        question: Option<&str>,
    ) -> DomainResult<DrawingAnalysis>;
    
    /// 验证图纸类型
    fn validate_drawing_type(&self, drawing_type: &str) -> DomainResult<DrawingType>;
    
    /// 验证图片数据
    fn validate_image_data(&self, data: &[u8]) -> DomainResult<()>;
}
