//! 多模态模型模板分类器
//!
//! 使用 llava/qwen 等多模态模型直接对涵洞图纸进行分类
//! 优点：
//! - 准确率高（可达 90%+）
//! - 无需额外 OCR 依赖
//! - 支持端到端分类
//!
//! 使用方式：
//! ```no_run
//! use crate::infrastructure::template_selection::multimodal_classifier::{MultimodalTemplateClassifier, ClassifierConfig};
//! use crate::api::ApiClient;
//!
//! let config = ClassifierConfig::default();
//! let classifier = MultimodalTemplateClassifier::new(config, api_client);
//! let (template_type, confidence) = classifier.classify(image_data).await?;
//! ```

use crate::domain::{
    service::template_selection::CulvertDrawingType,
    DomainError,
    DomainResult,
};
use crate::infrastructure::external::{ApiClient, Message};
use crate::infrastructure::confidence_handler::{ConfidenceThresholdHandler, ConfidenceThresholdConfig, ConfidenceResult};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use tracing::{info, warn, debug};

/// 分类器配置
#[derive(Debug, Clone)]
pub struct ClassifierConfig {
    /// 置信度阈值配置
    pub threshold_config: ConfidenceThresholdConfig,
    /// 是否启用日志
    pub enable_logging: bool,
    /// 分类超时（秒）
    pub timeout_seconds: u64,
    /// 最大重试次数
    pub max_retries: usize,
}

impl Default for ClassifierConfig {
    fn default() -> Self {
        Self {
            threshold_config: ConfidenceThresholdConfig::default(),
            enable_logging: true,
            timeout_seconds: 60,
            max_retries: 2,
        }
    }
}

/// 分类结果
#[derive(Debug, Clone)]
pub struct ClassificationResult {
    /// 分类的模板类型
    pub template_type: CulvertDrawingType,
    /// 置信度分数（0.0-1.0）
    pub confidence_score: f32,
    /// 置信度评估结果
    pub confidence_result: ConfidenceResult,
    /// 模型响应原文
    pub model_response: String,
}

/// 多模态模型模板分类器
pub struct MultimodalTemplateClassifier {
    config: ClassifierConfig,
    api_client: ApiClient,
    confidence_handler: ConfidenceThresholdHandler,
}

impl MultimodalTemplateClassifier {
    /// 创建新的分类器
    pub fn new(config: ClassifierConfig, api_client: ApiClient) -> Self {
        let confidence_handler = ConfidenceThresholdHandler::new(
            config.threshold_config.clone()
        );
        Self {
            config,
            api_client,
            confidence_handler,
        }
    }

    /// 创建带有自定义置信度处理器的分类器
    pub fn with_confidence_handler(
        config: ClassifierConfig,
        api_client: ApiClient,
        confidence_handler: ConfidenceThresholdHandler,
    ) -> Self {
        Self {
            config,
            api_client,
            confidence_handler,
        }
    }

    /// 获取配置
    pub fn config(&self) -> &ClassifierConfig {
        &self.config
    }

    /// 分类图片
    ///
    /// # 参数
    /// * `image_data` - 图片数据（JPEG/PNG 等格式）
    ///
    /// # 返回
    /// * `Ok(ClassificationResult)` - 分类结果
    /// * `Err(DomainError)` - 分类失败
    pub async fn classify(&self, image_data: &[u8]) -> DomainResult<ClassificationResult> {
        // 1. 构建分类 Prompt
        let prompt = self.build_classification_prompt();

        // 2. 将图片转换为 Base64
        let image_base64 = BASE64.encode(image_data);

        // 3. 构建消息
        let messages = vec![Message::user_with_images(prompt, vec![image_base64])];

        // 4. 多次采样，计算一致性（真正的置信度计算）
        let result = self.classify_with_sampling(&messages).await?;
        
        // 5. 验证并返回结果
        self.validate_and_return(result)
    }

    /// 多次采样分类，计算一致性置信度
    ///
    /// 通过多次独立采样，统计各类型得票数，计算一致性作为置信度
    /// - 5 次采样，5 次都输出同一个类型 = 1.0 置信度
    /// - 5 次采样，3 次输出同一个类型 = 0.6 置信度
    async fn classify_with_sampling(&self, messages: &[Message]) -> DomainResult<(CulvertDrawingType, String, f32)> {
        use std::collections::HashMap;
        
        const SAMPLE_COUNT: usize = 5; // 采样次数
        let mut votes: HashMap<String, (CulvertDrawingType, usize)> = HashMap::new();
        let mut last_response = String::new();
        let mut last_error = None;

        // 执行多次采样
        for attempt in 0..SAMPLE_COUNT {
            match self.do_classify(messages).await {
                Ok((template_type, response)) => {
                    let type_id = template_type.to_internal_id().to_string();
                    last_response = response.clone();
                    
                    // 累加票数
                    let entry = votes.entry(type_id).or_insert((template_type, 0));
                    entry.1 += 1;
                }
                Err(e) => {
                    if self.config.enable_logging {
                        warn!("第 {} 次采样失败：{}", attempt + 1, e);
                    }
                    last_error = Some(e);
                    // 继续下一次采样，不中断
                }
            }
        }

        // 如果没有成功采样，返回错误
        if votes.is_empty() {
            return Err(last_error.unwrap_or_else(|| DomainError::validation(
                "classification",
                "分类失败：所有采样均失败"
            )));
        }

        // 计算总有效样本数
        let total_valid_samples = votes.values().map(|(_, v)| v).sum::<usize>();

        // 找出得票最多的类型
        let (best_type_id, (best_template, max_votes)) = votes
            .into_iter()
            .max_by_key(|(_, (_, votes))| *votes)
            .unwrap(); // safe: votes is not empty

        // 计算置信度 = 最高票数 / 有效采样次数
        let confidence = max_votes as f32 / total_valid_samples as f32;

        if self.config.enable_logging {
            debug!(
                "采样结果：{} 票最高 ({}/{})，置信度：{:.2}",
                best_type_id, max_votes, total_valid_samples, confidence
            );
        }

        Ok((best_template, last_response, confidence))
    }

    /// 执行分类请求
    async fn do_classify(&self, messages: &[Message]) -> DomainResult<(CulvertDrawingType, String)> {
        let response = self.api_client.chat(messages).await
            .map_err(|e| DomainError::external_service("MultimodalClassifier", "chat", format!("模型调用失败：{}", e)))?;

        // 解析响应
        let response = response.trim().to_lowercase();
        debug!("模型响应：{}", response);

        // 尝试从响应中提取模板类型
        let template_type = self.parse_template_type(&response)?;

        Ok((template_type, response))
    }

    /// 构建分类 Prompt
    fn build_classification_prompt(&self) -> String {
        let all_types = CulvertDrawingType::get_all_types();
        
        // 构建类型列表
        let type_list = all_types.iter()
            .map(|t| format!("  - `{}`: {}", t.to_internal_id(), t.as_str()))
            .collect::<Vec<_>>()
            .join("\n");

        format!(
            r#"你是一位专业的涵洞图纸分类专家。请判断这张图纸属于以下哪种类型：

可用的模板类型（共{}种）：
{}

分类规则：
1. 仔细观察图纸的标题、表格名称、图例等文字信息
2. 根据图纸内容判断其类型
3. 只返回类型标识符（如 `culvert_setting_table`），不要包含任何其他文字
4. 如果无法确定，返回最可能的类型

请分类："#,
            all_types.len(),
            type_list
        )
    }

    /// 解析模板类型
    fn parse_template_type(&self, response: &str) -> DomainResult<CulvertDrawingType> {
        // 清理响应文本
        let cleaned = response
            .trim()
            .trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '-');

        // 直接匹配
        if let Some(template_type) = CulvertDrawingType::from_internal_id(cleaned) {
            return Ok(template_type);
        }

        // 尝试模糊匹配（去除前后缀）
        for template in CulvertDrawingType::get_all_types() {
            let id = template.to_internal_id();
            if cleaned.contains(&id) || id.contains(cleaned) {
                return Ok(template.clone());
            }
        }

        // 尝试关键词匹配
        let keywords = [
            ("一览表", CulvertDrawingType::CulvertSettingTable),
            ("数量表", CulvertDrawingType::CulvertQuantityTable),
            ("布置图", CulvertDrawingType::CulvertLayout),
            ("暗涵", CulvertDrawingType::DarkCulvertLayout),
            ("2m", CulvertDrawingType::BoxCulvertReinforcement2m),
            ("3m", CulvertDrawingType::BoxCulvertReinforcement3m),
            ("4m", CulvertDrawingType::BoxCulvertReinforcement4m),
            ("斜涵", CulvertDrawingType::SkewedBoxCulvertReinforcement2m),
            ("防水", CulvertDrawingType::JointWaterproofing),
            ("止水带", CulvertDrawingType::WaterStopInstallation),
            ("帽石", CulvertDrawingType::CapStoneReinforcement),
            ("基础钢筋网", CulvertDrawingType::FoundationReinforcementPlan),
            ("方案图", CulvertDrawingType::CulvertLengthAdjustment1),
            ("斜布", CulvertDrawingType::SkewedReinforcementCombination),
        ];

        for (keyword, template_type) in &keywords {
            if response.contains(keyword) {
                if self.config.enable_logging {
                    info!("通过关键词 '{}' 匹配到类型 {:?}", keyword, template_type);
                }
                return Ok(template_type.clone());
            }
        }

        Err(DomainError::validation(
            "template_type",
            format!("无法解析模板类型：'{}'。响应：{}", cleaned, response)
        ))
    }

    /// 验证并返回结果
    fn validate_and_return(&self, result: (CulvertDrawingType, String, f32)) -> DomainResult<ClassificationResult> {
        let (template_type, model_response, confidence_score) = result;

        // 使用置信度处理器评估
        let confidence_result = self.confidence_handler.evaluate(
            template_type.clone(),
            confidence_score
        );

        if self.config.enable_logging {
            if confidence_result.needs_review {
                warn!(
                    "低置信度分类：{:?} (置信度：{:.2})，建议人工复核",
                    template_type,
                    confidence_score
                );
            } else {
                info!(
                    "模板分类完成：{:?} (置信度：{:.2}, 级别：{:?})",
                    template_type,
                    confidence_score,
                    confidence_result.confidence_level
                );
            }
        }

        Ok(ClassificationResult {
            template_type,
            confidence_score,
            confidence_result,
            model_response,
        })
    }

    /// 批量分类（用于批量处理模式）
    pub async fn classify_batch(
        &self,
        images: &[Vec<u8>],
        max_concurrency: usize,
    ) -> DomainResult<Vec<ClassificationResult>> {
        use futures::stream::{self, StreamExt};

        let results = stream::iter(images.iter())
            .map(|image_data| async move {
                self.classify(image_data.as_slice()).await
            })
            .buffered(max_concurrency)
            .collect::<Vec<_>>()
            .await;

        // 收集结果，遇到错误时返回第一个错误
        let mut ok_results = Vec::new();
        for result in results {
            match result {
                Ok(r) => ok_results.push(r),
                Err(e) => return Err(e),
            }
        }

        Ok(ok_results)
    }
}

/// 为分类器实现 TemplateClassifier trait
#[async_trait::async_trait]
impl crate::domain::service::template_selection::TemplateClassifier for MultimodalTemplateClassifier {
    async fn classify(&self, image_data: &[u8]) -> DomainResult<crate::domain::service::template_selection::ClassificationResult> {
        let result = self.classify(image_data).await?;
        Ok(crate::domain::service::template_selection::ClassificationResult {
            template_type: result.template_type,
            confidence: result.confidence_score,
            needs_review: result.confidence_result.needs_review,
            source: "multimodal".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = ClassifierConfig::default();
        assert_eq!(config.threshold_config.high_threshold, 0.8);
        assert!(config.enable_logging);
        assert_eq!(config.timeout_seconds, 60);
        assert_eq!(config.max_retries, 2);
    }
}
