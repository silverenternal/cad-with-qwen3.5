//! 流式批处理器
//!
//! 特性：
//! - 流式处理 PDF，避免一次性加载
//! - 并发控制（Semaphore 限流）
//! - 动态并发调整（根据 API 响应时间）
//! - 进度持久化
//! - 配额管理

use crate::infrastructure::external::ApiClient;
use crate::batch::session::SessionPool;
use crate::batch::circuit_breaker::CircuitBreaker;
use crate::batch::dead_letter_queue::DeadLetterQueue;
use crate::batch::progress::{BatchProgress, ProgressGuard, BatchStatus};
use crate::batch::planner::{ProcessingPlan, BatchConfig};
use crate::batch::merger::{FinalResult, TempDirGuard, create_temp_dir};
use crate::batch::concurrency_controller::ConcurrencyController;
use crate::batch_result::BatchResult;
use crate::error::{Result, Error};

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{info, warn, error};

/// 流式批处理器配置
#[derive(Debug, Clone)]
pub struct StreamBatchProcessorConfig {
    /// 每批 PDF 数量
    pub pdfs_per_batch: usize,
    /// 并发处理 PDF 数量
    pub concurrency: usize,
    /// 启用动态并发调整
    pub enable_dynamic_concurrency: bool,
    /// 最小并发数
    pub min_concurrency: usize,
    /// 最大并发数
    pub max_concurrency: usize,
    /// 目标响应时间（毫秒）
    pub target_latency_ms: u64,
    /// 每 PDF 最大页数（0=无限制）
    pub max_pages_per_pdf: usize,
    /// 启用配额检查
    pub enable_quota_check: bool,
    /// 用户 ID（用于配额检查）
    pub user_id: Option<String>,
    /// 进度文件路径
    pub progress_file: Option<PathBuf>,
    /// 输出文件路径
    pub output_file: Option<PathBuf>,
}

impl Default for StreamBatchProcessorConfig {
    fn default() -> Self {
        Self {
            pdfs_per_batch: 5,
            concurrency: 2,
            enable_dynamic_concurrency: true,
            min_concurrency: 1,
            max_concurrency: 8,
            target_latency_ms: 3000, // 3 秒目标响应时间
            max_pages_per_pdf: 0,
            enable_quota_check: true,
            user_id: None,
            progress_file: None,
            output_file: None,
        }
    }
}

/// 流式批处理器
pub struct StreamBatchProcessor {
    config: StreamBatchProcessorConfig,
    api_client: Arc<ApiClient>,
    session_pool: Arc<SessionPool>,
    circuit_breaker: Arc<CircuitBreaker>,
    dead_letter_queue: Arc<DeadLetterQueue>,
    /// PDF 并发信号量
    pdf_semaphore: Arc<Semaphore>,
    /// 动态并发控制器
    concurrency_controller: Option<Arc<ConcurrencyController>>,
}

impl StreamBatchProcessor {
    /// 创建新的流式批处理器
    pub fn new(
        config: StreamBatchProcessorConfig,
        api_client: Arc<ApiClient>,
        session_pool: Arc<SessionPool>,
        circuit_breaker_config: crate::batch::CircuitBreakerConfig,
        dead_letter_queue: Arc<DeadLetterQueue>,
    ) -> Self {
        let pdf_semaphore = Arc::new(Semaphore::new(config.concurrency));
        let circuit_breaker = Arc::new(CircuitBreaker::new(circuit_breaker_config));

        let concurrency_controller = if config.enable_dynamic_concurrency {
            Some(Arc::new(ConcurrencyController::new(
                config.concurrency,
                config.min_concurrency,
                config.max_concurrency,
                config.target_latency_ms,
                5, // 5 秒冷却时间
            )))
        } else {
            None
        };

        Self {
            config,
            api_client,
            session_pool,
            circuit_breaker,
            dead_letter_queue,
            pdf_semaphore,
            concurrency_controller,
        }
    }

    /// 获取并发控制器（如果启用）
    pub fn concurrency_controller(&self) -> Option<&ConcurrencyController> {
        self.concurrency_controller.as_deref()
    }

    /// 处理 PDF 目录
    pub async fn process_directory(&self, pdf_dir: &Path) -> Result<FinalResult> {
        info!("开始流式批处理目录：{}", pdf_dir.display());

        // 获取可用配额
        let available_quota = u32::MAX;

        // 创建处理计划
        let batch_config = BatchConfig {
            pdfs_per_batch: self.config.pdfs_per_batch,
            max_pages_per_pdf: self.config.max_pages_per_pdf,
            concurrency: self.config.concurrency,
            enable_quota_check: self.config.enable_quota_check,
        };

        let plan = crate::batch::create_processing_plan(
            pdf_dir,
            available_quota,
            &batch_config,
        ).await?;

        info!("处理计划：{} PDF, {} 页，分为 {} 批", 
              plan.total_pdfs, plan.total_pages, plan.batches.len());

        // 创建进度管理
        let batches: Vec<crate::batch::BatchPlan> = plan.batches.iter().map(|b| {
            crate::batch::BatchPlan {
                batch_id: b.batch_id,
                pdfs: b.pdfs.iter().map(|p| p.path.clone()).collect(),
                status: BatchStatus::Pending,
                started_at: None,
                completed_at: None,
                results_file: None,
                error: None,
            }
        }).collect();

        let progress = BatchProgress::new(plan.total_pdfs, plan.total_pages, batches);
        
        let progress_guard = if let Some(ref path) = self.config.progress_file {
            // 尝试加载现有进度
            if let Some(mut existing) = BatchProgress::load_from_file(path) {
                info!("恢复现有进度：{}/{} PDF", existing.processed_pdfs, existing.total_pdfs);
                existing.output_path = self.config.output_file.clone();
                Some(ProgressGuard::new(existing, path.clone()))
            } else {
                let mut p = progress;
                p.output_path = self.config.output_file.clone();
                Some(ProgressGuard::new(p, path.clone()))
            }
        } else {
            None
        };

        // 创建临时目录（用于存储批次结果）
        let temp_dir = create_temp_dir("batch_processor")?;
        let _temp_guard = TempDirGuard::new(temp_dir.clone());

        info!("临时目录：{}", temp_dir.display());

        // 处理所有批次
        self.process_all_batches(plan, progress_guard, &temp_dir).await?;

        // 合并结果
        let final_result = self.merge_results(&temp_dir).await?;

        // 保存到输出文件
        if let Some(ref output_path) = self.config.output_file {
            final_result.save_to_file(output_path)?;
        }

        Ok(final_result)
    }

    /// 处理所有批次
    async fn process_all_batches(
        &self,
        plan: ProcessingPlan,
        mut progress_guard: Option<ProgressGuard>,
        temp_dir: &Path,
    ) -> Result<()> {
        for batch_plan in plan.batches {
            let batch_id = batch_plan.batch_id;

            // 检查是否已完成
            if let Some(ref progress) = progress_guard {
                if let Some(batch) = progress.progress().batches.iter()
                    .find(|b| b.batch_id == batch_id)
                {
                    if matches!(batch.status, BatchStatus::Completed { .. }) {
                        info!("批次 {} 已完成，跳过", batch_id);
                        continue;
                    }
                }
            }

            // 处理批次
            info!("开始处理批次 {}", batch_id);
            
            if let Some(ref mut guard) = progress_guard {
                if let Some(first_pdf) = batch_plan.pdfs.first() {
                    guard.progress_mut().mark_batch_started(batch_id, &first_pdf.path.display().to_string());
                    let _ = guard.save();
                }
            }

            match self.process_single_batch(&batch_plan, temp_dir).await {
                Ok(result_file) => {
                    info!("批次 {} 完成，结果：{}", batch_id, result_file.display());

                    if let Some(ref mut guard) = progress_guard {
                        guard.progress_mut().mark_batch_completed(
                            batch_id,
                            result_file.clone(),
                            batch_plan.pdfs.len(),
                            batch_plan.total_pages,
                        );
                        let _ = guard.save();
                    }
                }
                Err(e) => {
                    error!("批次 {} 失败：{}", batch_id, e);
                    
                    if let Some(ref mut guard) = progress_guard {
                        if let Some(first_pdf) = batch_plan.pdfs.first() {
                            guard.progress_mut().mark_batch_failed(
                                batch_id,
                                e.to_string(),
                                first_pdf.path.display().to_string(),
                            );
                            let _ = guard.save();
                        }
                    }

                    // 根据错误类型决定是否继续
                    if let Error::Validation(msg) = &e {
                        if msg.contains("配额") {
                            error!("配额耗尽，停止处理");
                            return Err(e);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// 处理单个批次
    async fn process_single_batch(
        &self,
        batch_plan: &crate::batch::planner::BatchPlan,
        temp_dir: &Path,
    ) -> Result<PathBuf> {
        let batch_id = batch_plan.batch_id;
        let result_file = temp_dir.join(format!("batch_{:03}_results.json", batch_id));

        // 创建批次结果
        let mut batch_result = BatchResult::new(
            format!("batch_{}", batch_id),
            chrono::Utc::now(),
        );

        info!("开始处理批次 {}，共 {} 个 PDF，{} 页",
              batch_id, batch_plan.pdfs.len(), batch_plan.total_pages);

        // 获取当前并发数（可能已动态调整）
        let current_concurrency = if let Some(controller) = &self.concurrency_controller {
            let stats = controller.stats();
            info!("动态并发统计：{}", stats);
            stats.current
        } else {
            self.config.concurrency
        };

        // 更新信号量
        self.pdf_semaphore.add_permits(current_concurrency);

        // 创建进度条
        use indicatif::{ProgressBar, ProgressStyle};
        let pb = ProgressBar::new(batch_plan.total_pages as u64);
        pb.set_style(ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} 页 ({eta})")
            .unwrap()
            .progress_chars("#>-"));

        // 流式处理每个 PDF
        for pdf_info in &batch_plan.pdfs {
            let pdf_path = &pdf_info.path;
            let page_count = pdf_info.page_count;

            info!("处理 PDF: {} ({} 页)", pdf_path.display(), page_count);
            pb.set_message(format!("处理 {}", pdf_path.file_name().unwrap_or_default().to_string_lossy()));

            // 逐页处理
            for page_num in 1..=page_count {
                // 检查熔断器状态
                let cb_state = self.circuit_breaker.state();
                if matches!(cb_state, crate::batch::CircuitState::Open | crate::batch::CircuitState::HalfOpen) {
                    // 等待熔断器恢复
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }

                // 获取信号量许可（带超时）
                let permit = if let Some(controller) = &self.concurrency_controller {
                    controller.acquire_with_timeout(
                        &self.pdf_semaphore,
                        std::time::Duration::from_secs(30),
                    ).await
                } else {
                    self.pdf_semaphore.acquire().await.ok()
                };

                if permit.is_none() {
                    warn!("获取处理许可失败，跳过此页");
                    continue;
                }

                // 转换 PDF 页为图片并分析
                match self.process_pdf_page(pdf_path, page_num).await {
                    Ok(file_result) => {
                        // 记录成功请求到并发控制器（用于退出限流保护模式）
                        if let Some(controller) = &self.concurrency_controller {
                            controller.record_success();
                        }
                        batch_result.add_result(file_result);
                    }
                    Err(e) => {
                        // 检测是否为限流错误
                        let is_rate_limit_error = e.to_string().contains("请求过于频繁")
                            || e.to_string().contains("RATE_LIMIT")
                            || e.to_string().contains("429");
                        
                        // 记录限流错误到并发控制器
                        if is_rate_limit_error {
                            if let Some(controller) = &self.concurrency_controller {
                                controller.record_rate_limit_error();
                            }
                        }
                        
                        warn!("处理 PDF 页失败 {}:{} - {}", pdf_path.display(), page_num, e);
                        // 创建失败结果
                        let file_name = pdf_path.file_name()
                            .unwrap_or_default()
                            .to_string_lossy();
                        batch_result.add_result(crate::batch_result::FileResult::failed(
                            format!("{}:{}", file_name, page_num),
                            "cad_drawing".to_string(),
                            "请分析这张 CAD 图纸".to_string(),
                            e.to_string(),
                        ));
                    }
                }

                pb.inc(1);
            }
        }

        pb.finish_with_message("批次处理完成");

        // 保存批次结果
        batch_result.save_to_file(&result_file)
            .map_err(|e| Error::Internal(e.to_string()))?;

        info!("批次 {} 完成，结果保存到 {}", batch_id, result_file.display());

        Ok(result_file)
    }

    /// 处理单个 PDF 页
    async fn process_pdf_page(
        &self,
        pdf_path: &Path,
        page_num: usize,
    ) -> Result<crate::batch_result::FileResult> {
        use std::time::Instant;

        let start_time = Instant::now();
        let file_name = pdf_path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let file_id = format!("{}:{}", file_name, page_num);

        // 获取会话
        let session = self.session_pool.next_session();

        // 处理 PDF（session.process_with_retry 会处理 PDF 转换和 API 调用）
        let answer = session.process_with_retry(pdf_path, &file_id).await?;

        let latency_ms = start_time.elapsed().as_millis() as u64;

        // 记录响应时间到并发控制器
        if let Some(controller) = &self.concurrency_controller {
            controller.record_latency(latency_ms);
        }

        Ok(crate::batch_result::FileResult::success(
            file_id,
            "cad_drawing".to_string(),
            "请分析这张 CAD 图纸".to_string(),
            answer,
            latency_ms,
        ))
    }

    /// 合并所有批次结果
    async fn merge_results(&self, temp_dir: &Path) -> Result<FinalResult> {
        let mut batch_files: Vec<PathBuf> = Vec::new();

        let mut entries = tokio::fs::read_dir(temp_dir).await
            .map_err(|e| Error::Internal(e.to_string()))?;

        while let Some(entry) = entries.next_entry().await
            .map_err(|e| Error::Internal(e.to_string()))?
        {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "json") {
                batch_files.push(path);
            }
        }

        batch_files.sort();

        info!("合并 {} 个批次结果", batch_files.len());

        Ok(FinalResult::from_batch_results(&batch_files, chrono::Utc::now())
            .map_err(|e| Error::Internal(e.to_string()))?)
    }
}
