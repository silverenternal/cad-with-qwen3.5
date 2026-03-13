//! 批量处理会话池 - 真正的连接池/会话池实现
//!
//! 每个会话包含独立的 ApiClient、配置和重试策略

use crate::infrastructure::external::{ApiClient, Message};
use crate::error::{Result, Error};
use crate::infrastructure::template_selection::{
    HybridTemplateClassifier, HybridClassifierConfig,
};
use std::sync::atomic::AtomicU32;
use std::sync::Arc;
use std::time::Duration;
use tracing::{warn, info};

/// 批量处理会话 - 每个会话独立配置
/// 
/// 支持两种模式：
/// 1. 固定类型模式：所有图片使用相同的图纸类型（向后兼容）
/// 2. 每图识别模式：每张图片独立识别模板类型
#[derive(Clone)]
pub struct BatchSession {
    /// 独立的 API 客户端
    client: ApiClient,
    /// 会话 ID（用于追踪）
    session_id: uuid::Uuid,
    /// 图纸类型（固定类型模式使用）
    pub drawing_type: String,
    /// 问题
    pub question: String,
    /// 最大图片尺寸
    max_image_dimension: u32,
    /// 最大重试次数
    pub max_retries: u32,
    /// 基础延迟 (ms)
    base_delay_ms: u64,
    /// 会话级配额剩余（独立追踪）
    quota_remaining: Option<Arc<AtomicU32>>,
    /// 会话级超时配置
    session_timeout: Duration,
    // ========== 每图识别模式配置 ==========
    /// 是否启用每图识别
    enable_per_image_classification: bool,
    /// 模板分类器（每图识别时使用）
    template_classifier: Option<Arc<HybridTemplateClassifier>>,
    /// 基础问题（用于构建动态 prompt）
    base_question: String,
}

impl BatchSession {
    /// 创建新会话（固定类型模式，向后兼容）
    pub fn new(
        client: ApiClient,
        drawing_type: String,
        question: String,
        max_image_dimension: u32,
        max_retries: u32,
        base_delay_ms: u64,
    ) -> Self {
        Self {
            client,
            session_id: uuid::Uuid::new_v4(),
            drawing_type,
            question: question.clone(),
            max_image_dimension,
            max_retries,
            base_delay_ms,
            quota_remaining: None,
            session_timeout: Duration::from_secs(300),  // 默认 5 分钟
            enable_per_image_classification: false,
            template_classifier: None,
            base_question: question.clone(),
        }
    }

    /// 创建带配额追踪的会话（固定类型模式）
    pub fn with_quota(
        client: ApiClient,
        drawing_type: String,
        question: String,
        max_image_dimension: u32,
        max_retries: u32,
        base_delay_ms: u64,
        initial_quota: u32,
    ) -> Self {
        Self {
            client,
            session_id: uuid::Uuid::new_v4(),
            drawing_type,
            question: question.clone(),
            max_image_dimension,
            max_retries,
            base_delay_ms,
            quota_remaining: Some(Arc::new(AtomicU32::new(initial_quota))),
            session_timeout: Duration::from_secs(300),
            enable_per_image_classification: false,
            template_classifier: None,
            base_question: question.clone(),
        }
    }

    /// 创建带自定义超时的会话（固定类型模式）
    pub fn with_timeout(
        client: ApiClient,
        drawing_type: String,
        question: String,
        max_image_dimension: u32,
        max_retries: u32,
        base_delay_ms: u64,
        timeout: Duration,
    ) -> Self {
        Self {
            client,
            session_id: uuid::Uuid::new_v4(),
            drawing_type,
            question: question.clone(),
            max_image_dimension,
            max_retries,
            base_delay_ms,
            quota_remaining: None,
            session_timeout: timeout,
            enable_per_image_classification: false,
            template_classifier: None,
            base_question: question.clone(),
        }
    }

    /// 创建支持每图识别的会话（新函数）
    #[allow(clippy::too_many_arguments)]
    pub fn with_per_image_classification(
        client: ApiClient,
        base_question: String,
        max_image_dimension: u32,
        max_retries: u32,
        base_delay_ms: u64,
        classification_config: HybridClassifierConfig,
    ) -> Self {
        let classifier = HybridTemplateClassifier::with_api_client(
            classification_config,
            client.clone_for_session(),
        );

        Self {
            client,
            session_id: uuid::Uuid::new_v4(),
            drawing_type: "auto_detected".to_string(), // 占位符，实际类型由每图识别决定
            question: base_question.clone(),
            max_image_dimension,
            max_retries,
            base_delay_ms,
            quota_remaining: None,
            session_timeout: Duration::from_secs(300),
            enable_per_image_classification: true,
            template_classifier: Some(Arc::new(classifier)),
            base_question,
        }
    }

    /// 创建支持每图识别的会话（共享分类器实例）
    #[allow(clippy::too_many_arguments)]
    pub fn with_per_image_classification_shared_classifier(
        client: ApiClient,
        base_question: String,
        max_image_dimension: u32,
        max_retries: u32,
        base_delay_ms: u64,
        _classification_config: HybridClassifierConfig,
        classifier: Arc<HybridTemplateClassifier>,
    ) -> Self {
        Self {
            client,
            session_id: uuid::Uuid::new_v4(),
            drawing_type: "auto_detected".to_string(), // 占位符，实际类型由每图识别决定
            question: base_question.clone(),
            max_image_dimension,
            max_retries,
            base_delay_ms,
            quota_remaining: None,
            session_timeout: Duration::from_secs(300),
            enable_per_image_classification: true,
            template_classifier: Some(classifier),
            base_question,
        }
    }

    /// 获取会话 ID
    pub fn session_id(&self) -> &uuid::Uuid {
        &self.session_id
    }

    /// 获取会话超时配置
    pub fn timeout(&self) -> Duration {
        self.session_timeout
    }

    /// 获取剩余配额
    pub fn remaining_quota(&self) -> Option<u32> {
        self.quota_remaining.as_ref().map(|q| q.load(std::sync::atomic::Ordering::SeqCst))
    }

    /// 扣减配额（原子操作）
    pub fn consume_quota(&self, count: u32) -> bool {
        if let Some(ref quota) = self.quota_remaining {
            loop {
                let current = quota.load(std::sync::atomic::Ordering::SeqCst);
                if current < count {
                    return false;
                }
                if quota.compare_exchange_weak(
                    current,
                    current - count,
                    std::sync::atomic::Ordering::SeqCst,
                    std::sync::atomic::Ordering::SeqCst,
                )
                .is_ok()
                {
                    return true;
                }
            }
        }
        true  // 没有配额限制时返回 true
    }

    /// 处理单张图片（带重试）
    pub async fn process_with_retry(&self, path: &std::path::Path, file_name: &str) -> Result<String> {
        let mut last_error = None;

        for attempt in 0..=self.max_retries {
            match self.process_single(path).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    let batch_error = crate::batch::BatchError::from_app_error(&e);

                    // 不可重试错误直接返回
                    if !batch_error.is_retryable() {
                        return Err(e);
                    }

                    // 记录重试日志
                    if attempt < self.max_retries {
                        let delay_ms = self.base_delay_ms * 2u64.pow(attempt);
                        warn!(
                            "File {} failed (attempt {}/{}), retrying in {}ms: {}",
                            file_name,
                            attempt + 1,
                            self.max_retries + 1,
                            delay_ms,
                            e
                        );
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    }

                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| Error::External("Unknown error".to_string())))
    }

    /// 处理单张图片（内部实现）
    async fn process_single(&self, path: &std::path::Path) -> Result<String> {
        use std::time::Instant;

        // 先读取原始图片数据（用于分类和编码）
        let encoding_start = Instant::now();
        let image_data = tokio::fs::read(path).await
            .map_err(|e| Error::Internal(e.to_string()))?;

        // 构建提示词（根据是否启用每图识别）
        // 注意：在编码前先构建 prompt，避免重复读文件
        let prompt = if self.enable_per_image_classification {
            // 每图识别模式：先识别类型，再构建动态 prompt
            self.build_prompt_with_classification(&image_data, path).await?
        } else {
            // 固定类型模式：使用配置的 drawing_type
            format!(
                "这是一张{}。{}\n\n请分析图片并回答。",
                self.drawing_type, self.question
            )
        };

        // 图片编码（使用 spawn_blocking）+ 超时保护
        let base64_image = tokio::time::timeout(
            Duration::from_secs(60),  // 编码超时 60 秒
            tokio::task::spawn_blocking({
                let image_data = image_data.clone();  // 克隆数据用于编码
                let _path = path.to_path_buf();
                let max_dim = self.max_image_dimension;
                move || {
                    // 从内存数据编码图片，避免再次读文件
                    encode_and_compress_image_from_data(&image_data, max_dim)
                }
            })
        )
        .await
        .map_err(|_| {
            Error::Internal("Image encoding timeout (exceeded 60s)".to_string())
        })?
        .map_err(|e| Error::Internal(format!("Image encoding task failed: {}", e)))??;

        // 记录编码延迟
        let encoding_duration = encoding_start.elapsed().as_secs_f64();
        tracing::debug!("Image encoding completed in {:.3}s", encoding_duration);

        // 构建消息
        let messages = vec![Message::user_with_images(prompt, vec![base64_image])];

        // 调用 API
        let answer = self.client.chat(&messages).await
            .map_err(|e| Error::External(e.to_string()))?;

        Ok(answer)
    }

    /// 使用模板分类器构建动态 prompt（每图识别模式）
    ///
    /// ## 性能优化
    /// 直接接收图片数据，避免重复读文件
    async fn build_prompt_with_classification(
        &self,
        image_data: &[u8],
        path: &std::path::Path,
    ) -> Result<String> {
        // 检查是否配置了分类器
        let classifier = self.template_classifier.as_ref()
            .ok_or_else(|| Error::Internal("Template classifier not configured for per-image classification".to_string()))?;

        // 调用分类器识别类型（直接使用传入的图片数据）
        let classification_result: crate::domain::service::template_selection::ClassificationResult = classifier.classify(image_data).await
            .map_err(|e| Error::Internal(format!("Template classification failed: {}", e)))?;

        let template_type = classification_result.template_type;
        let needs_review = classification_result.needs_review;

        if self.enable_per_image_classification {
            if needs_review {
                warn!(
                    "低置信度分类结果：{:?} (置信度：{:.2})，文件：{}",
                    template_type,
                    classification_result.confidence,
                    path.display()
                );
            } else {
                info!(
                    "模板分类完成：{:?} (置信度：{:.2}, 来源：{})",
                    template_type,
                    classification_result.confidence,
                    classification_result.source
                );
            }
        }

        // 构建动态 prompt
        Ok(format!(
            "这是一张{}。{}\n\n请分析图片并回答。",
            template_type.as_str(),
            self.base_question
        ))
    }

    /// 获取客户端名称
    pub fn client_name(&self) -> &str {
        self.client.client_name()
    }
}

/// 从内存数据编码图片（用于 spawn_blocking）
///
/// ## 性能优化
/// 避免重复读文件，直接从内存数据编码
fn encode_and_compress_image_from_data(image_data: &[u8], max_dimension: u32) -> Result<String> {
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
    use image::ImageFormat;

    // 从内存数据加载图片
    let img = image::load_from_memory(image_data)
        .map_err(|e| Error::Internal(e.to_string()))?;

    // 限制最大尺寸
    let img = if img.width() > max_dimension || img.height() > max_dimension {
        img.thumbnail(max_dimension, max_dimension)
    } else {
        img
    };

    // 压缩为 JPEG
    let mut jpeg_buffer = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut jpeg_buffer),
        ImageFormat::Jpeg,
    ).map_err(|e| Error::Internal(e.to_string()))?;

    Ok(BASE64.encode(&jpeg_buffer))
}

/// 会话池 - 预创建多个会话
pub struct SessionPool {
    sessions: Vec<BatchSession>,
    /// 当前索引（轮询调度）
    current_index: std::sync::atomic::AtomicUsize,
}

impl SessionPool {
    /// 创建会话池（固定类型模式，向后兼容）
    pub fn new(
        base_client: ApiClient,
        pool_size: usize,
        drawing_type: String,
        question: String,
        max_image_dimension: u32,
        max_retries: u32,
        base_delay_ms: u64,
    ) -> Self {
        let mut sessions = Vec::with_capacity(pool_size);

        // 预创建会话
        for _i in 0..pool_size {
            // 每个会话使用相同的客户端配置（ApiClient 内部已有连接池）
            let client = base_client.clone_for_session();
            sessions.push(BatchSession::new(
                client,
                drawing_type.clone(),
                question.clone(),
                max_image_dimension,
                max_retries,
                base_delay_ms,
            ));
        }

        Self {
            sessions,
            current_index: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// 创建支持每图识别的会话池
    pub fn with_per_image_classification(
        base_client: ApiClient,
        pool_size: usize,
        base_question: String,
        max_image_dimension: u32,
        max_retries: u32,
        base_delay_ms: u64,
        classification_config: HybridClassifierConfig,
    ) -> Self {
        // 在池层面创建单个分类器实例，所有会话共享
        let classifier = Arc::new(HybridTemplateClassifier::with_api_client(
            classification_config.clone(),
            base_client.clone_for_session(),
        ));

        let mut sessions = Vec::with_capacity(pool_size);

        // 预创建会话
        for _i in 0..pool_size {
            let client = base_client.clone_for_session();
            sessions.push(BatchSession::with_per_image_classification_shared_classifier(
                client,
                base_question.clone(),
                max_image_dimension,
                max_retries,
                base_delay_ms,
                classification_config.clone(),
                Arc::clone(&classifier),
            ));
        }

        Self {
            sessions,
            current_index: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// 获取下一个可用会话（轮询）
    pub fn next_session(&self) -> BatchSession {
        let idx = self.current_index.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.sessions[idx % self.sessions.len()].clone()
    }

    /// 获取池大小
    pub fn size(&self) -> usize {
        self.sessions.len()
    }
}
