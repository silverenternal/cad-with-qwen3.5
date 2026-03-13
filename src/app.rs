//! 应用运行时模块 - 简化版

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::io::{self, Write};
use std::time::Instant;
use std::path::{Path, PathBuf};

use crate::infrastructure::external::ApiClient;
use crate::cache::ImageCache;
use crate::cli;
use crate::config::Config;
use crate::dialog::DialogManager;
use crate::server::types::DrawingType;
use crate::telemetry::TelemetryRecorder;
use tracing::{info, warn, error};

/// 创建 tar.gz 归档文件
fn create_tar_gz_archive(archive_path: &str, files: &[PathBuf]) -> io::Result<u64> {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::fs::File;
    use tar::Builder;

    let tar_gz = File::create(archive_path)?;
    let enc = GzEncoder::new(tar_gz, Compression::default());
    let mut tar = Builder::new(enc);

    for file_path in files {
        if file_path.is_file() {
            let file_name = file_path.file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new("unknown"))
                .to_string_lossy();
            // 使用扁平化结构，所有文件放在根目录
            tar.append_path_with_name(file_path, &*file_name)?;
        }
    }

    let enc = tar.into_inner()?;
    let file = enc.finish()?;
    
    // 获取归档文件大小
    let metadata = file.metadata()?;
    Ok(metadata.len())
}

/// 应用运行时状态
pub struct App {
    client: ApiClient,
    image_cache: ImageCache,
    dialog_manager: DialogManager,
    drawing_type: DrawingType,
    telemetry: TelemetryRecorder,
    config: Config,
}

impl App {
    /// 创建新的应用实例
    pub fn new(client: ApiClient, config: &Config, telemetry: TelemetryRecorder) -> Result<Self, std::io::Error> {
        // 使用当前目录作为图片根目录（CLI 模式）
        let root_dir = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."));
        
        let image_cache = ImageCache::new(
            config.cache_max_entries, 
            100, 
            config.max_image_dimension, 
            85,
            root_dir.join("cad_image"),
        )
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string()))?;
        let dialog_manager = DialogManager::new(
            client.model(),
            28000,  // max_tokens
            30,     // max_rounds
        );

        Ok(Self {
            client,
            image_cache,
            dialog_manager,
            drawing_type: DrawingType::BuildingPlan,
            telemetry,
            config: config.clone(),
        })
    }

    /// 初始化提示词模板（使用默认类型，模板自动选择由后端处理）
    pub fn init_prompt_template(&mut self) -> io::Result<()> {
        use crate::prompt::load_prompt_template;

        // 使用默认图纸类型（模板自动选择由后端服务处理）
        let drawing_type = self.drawing_type.clone();
        info!("Using default drawing type: {}", drawing_type.as_str());

        // 加载提示词模板（运行时加载，支持热更新）
        let template = load_prompt_template(drawing_type);
        let system_prompt = template.build();
        self.dialog_manager.add_system(system_prompt);

        info!("Loaded prompt template with auto template selection enabled");
        Ok(())
    }

    /// 运行应用主循环
    pub async fn run(&mut self, shutdown_flag: &Arc<AtomicBool>) -> io::Result<()> {
        info!("Entering interactive dialog mode");

        loop {
            if shutdown_flag.load(Ordering::Relaxed) {
                break;
            }

            print!("\n👤 你：");
            io::stdout().flush()?;

            let mut user_input = String::new();
            if io::stdin().read_line(&mut user_input).is_err() {
                continue;
            }

            let user_input = user_input.trim();

            // 处理内置命令
            match user_input.to_lowercase().as_str() {
                "quit" | "exit" => break,
                "clear" => {
                    self.dialog_manager.clear();
                    self.image_cache.clear();
                    info!("Dialog history and image cache cleared");
                    continue;
                }
                "help" => {
                    self.print_help();
                    continue;
                }
                "stats" => {
                    info!("Dialog: {} | Cache: {}",
                        self.dialog_manager.stats(),
                        self.image_cache.stats());
                    continue;
                }
                "config" => {
                    self.reconfigure_api_key();
                    continue;
                }
                "diagnose" => {
                    // 诊断 PDF 文件
                    println!("\n📋 PDF 诊断工具");
                    println!("请输入 PDF 文件路径：");
                    print!("> ");
                    let _ = io::stdout().flush();
                    
                    let mut pdf_path = String::new();
                    if io::stdin().read_line(&mut pdf_path).is_ok() {
                        let pdf_path = pdf_path.trim();
                        if !pdf_path.is_empty() {
                            use crate::pdf_utils::{diagnose_pdf, print_diagnostic_report};
                            let diagnostic = diagnose_pdf(std::path::Path::new(pdf_path));
                            print_diagnostic_report(&diagnostic);
                        }
                    }
                    continue;
                }
                "export" => {
                    // 导出对话历史
                    let export_path = format!("dialog_{}.md", chrono::Local::now().format("%Y%m%d_%H%M%S"));
                    match self.export_dialog_history(std::path::Path::new(&export_path)) {
                        Ok(()) => {
                            println!("✅ 对话历史已导出到：{}", export_path);
                            info!("Dialog history exported: {}", export_path);
                        }
                        Err(e) => {
                            println!("⚠️  导出失败：{}", e);
                            error!("Export failed: {}", e);
                        }
                    }
                    continue;
                }
                "export-all" => {
                    // 批量导出所有报告和对话
                    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
                    let export_dir = format!("export_{}", timestamp);
                    
                    // 创建导出目录
                    if let Err(e) = std::fs::create_dir_all(&export_dir) {
                        println!("⚠️  创建目录失败：{}", e);
                        continue;
                    }
                    
                    println!("📦 正在导出所有文件到：{}", export_dir);
                    
                    // 查找当前目录的所有报告和对话文件
                    let mut exported_count = 0;
                    if let Ok(entries) = std::fs::read_dir(".") {
                        for entry in entries.flatten() {
                            let file_name = entry.file_name();
                            let file_name_str = file_name.to_string_lossy();
                            
                            if (file_name_str.starts_with("pdf_report_") && file_name_str.ends_with(".md"))
                                || (file_name_str.starts_with("dialog_") && file_name_str.ends_with(".md"))
                                || (file_name_str.starts_with("batch_results_") && (file_name_str.ends_with(".json") || file_name_str.ends_with(".csv")))
                            {
                                let dest_path = std::path::Path::new(&export_dir).join(&*file_name);
                                if let Err(e) = std::fs::copy(entry.path(), &dest_path) {
                                    warn!("复制文件失败 {}: {}", file_name_str, e);
                                } else {
                                    exported_count += 1;
                                }
                            }
                        }
                    }
                    
                    println!("✅ 已导出 {} 个文件到：{}/", exported_count, export_dir);
                    info!("Exported {} files to {}/", exported_count, export_dir);
                    continue;
                }
                "archive" => {
                    // 压缩归档所有报告和历史文件
                    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
                    let archive_name = format!("cad_reports_{}.tar.gz", timestamp);
                    
                    println!("📦 正在创建归档文件：{}", archive_name);
                    
                    // 收集要归档的文件
                    let mut files_to_archive: Vec<std::path::PathBuf> = Vec::new();
                    if let Ok(entries) = std::fs::read_dir(".") {
                        for entry in entries.flatten() {
                            let file_name = entry.file_name();
                            let file_name_str = file_name.to_string_lossy();
                            
                            if (file_name_str.starts_with("pdf_report_") && file_name_str.ends_with(".md"))
                                || (file_name_str.starts_with("dialog_") && file_name_str.ends_with(".md"))
                                || (file_name_str.starts_with("batch_results_") && (file_name_str.ends_with(".json") || file_name_str.ends_with(".csv")))
                                || (file_name_str.starts_with(".batch_progress_") && file_name_str.ends_with(".json"))
                            {
                                files_to_archive.push(entry.path());
                            }
                        }
                    }
                    
                    if files_to_archive.is_empty() {
                        println!("⚠️  没有找到需要归档的文件");
                        continue;
                    }
                    
                    // 创建 tar.gz 归档
                    match create_tar_gz_archive(&archive_name, &files_to_archive) {
                        Ok(size) => {
                            println!("✅ 归档完成：{} ({} 个文件，{:.2} KB)", archive_name, files_to_archive.len(), size as f64 / 1024.0);
                            info!("Archive created: {} ({} files, {:.2} KB)", archive_name, files_to_archive.len(), size as f64 / 1024.0);
                        }
                        Err(e) => {
                            println!("⚠️  归档失败：{}", e);
                            error!("Archive failed: {}", e);
                        }
                    }
                    continue;
                }
                _ => {}
            }

            // 解析 @图片路径（支持 PDF 多页）
            let mut attached_images: Vec<String> = Vec::new();
            let mut pdf_page_results: Vec<(String, String)> = Vec::new();  // (页码标识，base64)
            let input_text: String = user_input
                .split_whitespace()
                .filter(|part| !part.starts_with('@'))
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .join(" ");

            for part in user_input.split_whitespace().filter(|p| p.starts_with('@')) {
                if let Some(path) = part.strip_prefix('@') {
                    let safe_path = crate::security::sanitize_path(&self.image_cache.root_dir(), path)
                        .unwrap_or_else(|_| std::path::PathBuf::from(path));
                    
                    // 检测是否为 PDF
                    if safe_path.extension().map_or(false, |ext| ext.to_ascii_lowercase() == "pdf") {
                        // PDF 多页处理
                        info!("检测到 PDF 文件，逐页加载：{}", path);
                        println!("[INFO] 正在加载 PDF: {} ...", path);
                        
                        match self.image_cache.get_or_load_pdf(path).await {
                            Ok(base64_pages) => {
                                info!("PDF 加载成功：{} 页", base64_pages.len());
                                println!("[OK] PDF 加载成功：{} 页", base64_pages.len());
                                
                                // 为每页创建标识
                                for (page_idx, base64_data) in base64_pages.iter().enumerate() {
                                    let page_id = format!("{}:{}", path, page_idx + 1);
                                    pdf_page_results.push((page_id, base64_data.clone()));
                                }
                            }
                            Err(e) => {
                                error!("PDF 加载失败 '{}': {}", path, e);
                                println!("[ERROR] PDF 加载失败：{}", e);
                            }
                        }
                    } else {
                        // 普通图片处理
                        match self.image_cache.get_or_load(path).await {
                            Ok(base64_data) => {
                                info!("Loaded image: {}", path);
                                println!("[OK] 已加载图片：{}", path);
                                attached_images.push(base64_data);
                            }
                            Err(e) => {
                                error!("Failed to load image '{}': {}", path, e);
                                println!("[ERROR] 图片加载失败：{}", e);
                            }
                        }
                    }
                }
            }

            // PDF 逐页分析（带进度条和并发控制）
            if !pdf_page_results.is_empty() {
                use indicatif::{ProgressBar, ProgressStyle};
                use std::sync::Arc;
                use tokio::sync::Semaphore;

                let total_pages = pdf_page_results.len();
                info!("开始逐页分析 PDF，共 {} 页", total_pages);
                
                // 创建进度条
                let pb = ProgressBar::new(total_pages as u64);
                pb.set_style(ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} 页 ({eta})")
                    .unwrap()
                    .progress_chars("#>-"));
                println!("\n📄 开始逐页分析 PDF，共 {} 页...\n", total_pages);

                // 并发控制（最多 2 个并发请求，避免 API 限流）
                let semaphore = Arc::new(Semaphore::new(2));
                let mut page_results: Vec<(String, Result<String, crate::error::Error>)> = Vec::new();

                // 逐页处理（串行，保证顺序）
                for (page_id, base64_image) in pdf_page_results.iter() {
                    let page_num = page_id.split(':').last().unwrap_or("");
                    pb.set_message(format!("处理第 {} 页", page_num));

                    // 获取并发许可
                    let _permit = semaphore.acquire().await.unwrap();

                    println!("\n━━━━━ 第 {} 页 ━━━━━", page_num);

                    // 添加用户消息（带单页图片）
                    let user_msg = if input_text.trim().is_empty() {
                        format!("请分析这张 CAD 图纸的第 {} 页", page_num)
                    } else {
                        format!("{} (第 {} 页)", input_text.trim(), page_num)
                    };

                    self.dialog_manager.add_user_with_images(
                        user_msg,
                        vec![base64_image.clone()]
                    );

                    // 调用 API（带验证重试）
                    println!("[INFO] 正在分析第 {} 页...", page_num);
                    let start_time = Instant::now();

                    let retry_config = crate::recognition_validator::RetryConfig {
                        max_retries: 2,
                        min_confidence: 0.5,
                        enable_validation: true,
                        initial_delay_ms: 100,
                        backoff_multiplier: 2.0,
                        max_delay_ms: 2000,
                        weights: crate::recognition_validator::ValidationWeights::default(),
                    };

                    let api_result: Result<String, String> = 
                        crate::recognition_validator::call_with_validation(
                            || async {
                                let request = self.dialog_manager.build_request();
                                self.client.chat_stream(request.messages.as_slice()).await
                                    .map_err(|e| e.to_string())
                            },
                            &retry_config
                        ).await;

                    let result: Result<String, crate::error::Error> = match api_result {
                        Ok(content) => {
                            let latency_ms = start_time.elapsed().as_millis() as u64;
                            println!("\n🤖 AI (第 {} 页):\n{}\n", page_num, content);
                            self.dialog_manager.add_assistant(content.clone());

                            // 记录遥测
                            self.telemetry.log_request(
                                "/api/chat",
                                latency_ms,
                                true,
                                Some(self.client.model()),
                            ).await;

                            Ok(content)
                        }
                        Err(e) => {
                            let latency_ms = start_time.elapsed().as_millis() as u64;
                            self.telemetry.log_request(
                                "/api/chat",
                                latency_ms,
                                false,
                                Some(self.client.model()),
                            ).await;
                            self.telemetry.log_error(
                                &format!("{:?}", e),
                                &e.to_string(),
                                serde_json::json!({"page": page_id}),
                            ).await;
                            println!("[ERROR] 第 {} 页分析失败：{}", page_num, e);

                            Err(crate::error::Error::Internal(e))
                        }
                    };

                    page_results.push((page_id.clone(), result));

                    // 清空对话历史，为下一页准备（保留系统提示）
                    self.dialog_manager.clear();
                    self.dialog_manager.add_system(self.dialog_manager.system_prompt().to_string());

                    pb.inc(1);
                }

                pb.finish_with_message("PDF 分析完成");

                // 生成汇总报告
                println!("\n\n═══════════════════════════════════════════════════════════");
                println!("  📊 PDF 分析汇总报告");
                println!("═══════════════════════════════════════════════════════════");
                
                let success_count = page_results.iter().filter(|(_, r)| r.is_ok()).count();
                let failed_count = page_results.iter().filter(|(_, r)| r.is_err()).count();
                
                println!("\n总页数：{}", total_pages);
                println!("成功：{} 页 | 失败：{} 页", success_count, failed_count);
                
                if failed_count > 0 {
                    println!("\n⚠️  失败的页面：");
                    for (page_id, result) in &page_results {
                        if let Err(e) = result {
                            println!("  - {}: {}", page_id.split(':').last().unwrap_or(""), e);
                        }
                    }
                }
                
                // 收集所有成功结果进行批量汇总分析
                let successful_results: Vec<(String, String)> = page_results.iter()
                    .filter_map(|(page_id, result)| {
                        result.as_ref().ok().map(|content| (page_id.clone(), content.clone()))
                    })
                    .collect();
                
                if successful_results.len() > 1 {
                    println!("\n\n📋 正在生成跨页汇总分析...");

                    // 构建汇总提示词
                    let summary_prompt = format!(
                        "请根据以下 CAD 图纸的各页分析结果，生成一个整体的汇总报告：\n\n{}",
                        successful_results.iter()
                            .map(|(page_id, content)| format!("【{}】\n{}\n", page_id, content))
                            .collect::<Vec<_>>()
                            .join("\n")
                    );

                    // 添加系统提示
                    self.dialog_manager.add_system(self.dialog_manager.system_prompt().to_string());

                    // 添加汇总请求
                    self.dialog_manager.add_user(summary_prompt);

                    // 调用 API 生成汇总
                    println!("[INFO] 正在生成跨页汇总报告...");
                    let request = self.dialog_manager.build_request();
                    let start_time = Instant::now();

                    let summary_content: Option<String> = match self.client.chat_stream(request.messages.as_slice()).await {
                        Ok(content) => {
                            let latency_ms = start_time.elapsed().as_millis() as u64;
                            println!("\n\n═══════════════════════════════════════════════════════════");
                            println!("  📄 跨页汇总分析报告");
                            println!("═══════════════════════════════════════════════════════════\n");
                            println!("{}", content);
                            println!("\n═══════════════════════════════════════════════════════════\n");

                            // 记录遥测
                            self.telemetry.log_request(
                                "/api/chat",
                                latency_ms,
                                true,
                                Some(self.client.model()),
                            ).await;

                            Some(content)
                        }
                        Err(e) => {
                            warn!("生成汇总报告失败：{}", e);
                            println!("\n⚠️  生成汇总报告失败：{}", e);

                            self.telemetry.log_request(
                                "/api/chat",
                                start_time.elapsed().as_millis() as u64,
                                false,
                                Some(self.client.model()),
                            ).await;

                            None
                        }
                    };

                    // 自动导出 Markdown 报告
                    let report_path = std::path::PathBuf::from(format!(
                        "pdf_report_{}.md",
                        chrono::Local::now().format("%Y%m%d_%H%M%S")
                    ));

                    match self.export_pdf_report(&page_results, summary_content.as_deref(), &report_path) {
                        Ok(()) => {
                            println!("✅ 分析报告已保存到：{}", report_path.display());
                            info!("PDF 分析报告已导出：{}", report_path.display());
                        }
                        Err(e) => {
                            warn!("导出报告失败：{}", e);
                            println!("⚠️  导出报告失败：{}", e);
                        }
                    }
                }
                
                println!("\n═══════════════════════════════════════════════════════════\n");

                continue;  // 跳过后续的普通 API 调用
            }

            if input_text.trim().is_empty() && attached_images.is_empty() {
                continue;
            }

            // 添加用户消息
            let truncate_info = if attached_images.is_empty() {
                self.dialog_manager.add_user(input_text.trim().to_string())
            } else {
                self.dialog_manager.add_user_with_images(
                    input_text.trim().to_string(),
                    attached_images.clone()
                )
            };

            if let Some(info) = truncate_info {
                warn!("Dialog history truncated, removed {} old messages", info.removed_messages);
            }

            // 调用 API（带遥测）
            info!("Calling API...");
            println!("[INFO] 正在思考...");
            let request = self.dialog_manager.build_request();
            let start_time = Instant::now();

            match self.client.chat_stream(request.messages.as_slice()).await {
                Ok(content) => {
                    let latency_ms = start_time.elapsed().as_millis() as u64;
                    info!("AI response: {}", content.trim());
                    println!("\n🤖 AI: {}\n", content);
                    self.dialog_manager.add_assistant(content);

                    // 记录成功的 API 请求
                    self.telemetry.log_request(
                        "/api/chat",
                        latency_ms,
                        true,
                        Some(self.client.model()),
                    ).await;
                }
                Err(e) => {
                    let latency_ms = start_time.elapsed().as_millis() as u64;

                    // 记录错误的 API 请求
                    self.telemetry.log_request(
                        "/api/chat",
                        latency_ms,
                        false,
                        Some(self.client.model()),
                    ).await;

                    // 记录详细错误
                    self.telemetry.log_error(
                        &format!("{:?}", e),
                        &e.to_string(),
                        serde_json::json!({
                            "is_auth_error": e.is_auth_error(),
                            "is_model_not_found": e.is_model_not_found(),
                        }),
                    ).await;

                    if e.is_auth_error() {
                        error!("Invalid or expired API Key");
                    } else if e.is_model_not_found() {
                        error!("Model does not exist");
                    } else {
                        error!("API request failed: {}", e);
                    }
                    eprintln!("[ERROR] {}", e);

                    // API 出错时，提供更换 API Key 选项
                    if e.is_auth_error() || e.is_model_not_found() || e.is_client_error() {
                        println!("\n⚠️  检测到 API 配置问题");
                        let new_api_key = crate::cli::prompt_change_api_key();

                        // 根据用户选择更新客户端
                        if let Some(key) = new_api_key {
                            // 用户输入了新的 API Key
                            self.client = ApiClient::cloud(&self.config.default_cloud_model, &key, 3);
                            println!("✅ 已更新 API Key，切换到云端模式");
                            info!("API Key updated, switched to Cloud mode");
                        } else {
                            // 用户选择切换到本地模式或跳过
                            // 检查 .env 文件是否有新的 API Key
                            if let Some(key) = crate::cli::load_api_key_from_env() {
                                self.client = ApiClient::cloud(&self.config.default_cloud_model, &key, 3);
                                println!("✅ 已加载 .env 文件中的 API Key，使用云端模式");
                                info!("Loaded API Key from .env file, using Cloud mode");
                            } else {
                                self.client = ApiClient::local(&self.config.default_local_model, 3);
                                println!("✅ 已切换到本地模式");
                                info!("Switched to local mode");
                            }
                        }
                        info!("API client updated: {}", self.client.client_name());
                        info!("Using model: {}", self.client.model());
                    }
                }
            }
        }

        Ok(())
    }

    fn print_help(&self) {
        info!("Displaying help information");
        println!("\n╔═══════════════════════════════════════════════════════════╗");
        println!("║                    CAD 图纸识别 - 帮助                     ║");
        println!("╚═══════════════════════════════════════════════════════════╝");

        println!("\n📖 基本用法：");
        println!("  • 使用 @路径 附加图片，例如：@cad_image/plan.jpg");
        println!("  • 支持多图片对比：@plan1.jpg @plan2.jpg 对比差异");
        println!("  • 支持 PDF 多页逐页分析：@document.pdf 分析这个图纸");
        println!("  • 支持连续对话，上下文自动保留");

        println!("\n📋 内置命令：");
        println!("  • help / h              - 查看本帮助信息");
        println!("  • clear / cls           - 清空对话历史和图片缓存");
        println!("  • stats / status        - 查看统计信息（请求数、缓存命中率等）");
        println!("  • config / cfg          - 配置 API Key 或切换模式");
        println!("  • diagnose              - 诊断 PDF 文件（检查是否损坏/加密）");
        println!("  • export                - 导出当前对话历史为 Markdown");
        println!("  • export-all            - 批量导出所有报告和对话文件");
        println!("  • archive               - 压缩归档所有报告和历史文件 (.tar.gz)");
        println!("  • type / t              - 切换图纸类型（重新选择）");
        println!("  • quit / exit / q       - 退出程序");

        println!("\n🔧 当前配置：");
        println!("  • 运行模式：{}", self.client.client_name());
        println!("  • 使用模型：{}", self.client.model());
        println!("  • 图纸类型：{}", self.drawing_type.as_str());

        println!("\n💡 使用示例：");
        println!("  单图分析：@cad_image/plan.jpg 分析这个户型有几个房间？");
        println!("  多图对比：@plan_v1.jpg @plan_v2.jpg 对比这两个方案的差异");
        println!("  PDF 分析：@cad_images/plan.pdf 分析这个图纸（自动逐页分析）");
        println!("  连续对话：先问\"比例尺是多少\"，再问\"有哪些房间\"");

        println!("\n📁 其他模式：");
        println!("  • 批量处理：cargo run -- --batch ./cad_images/");
        println!("  • 流式批处理：cargo run -- --batch ./pdfs/ --streaming");
        println!("  • Web 服务：cargo run -- --server");

        println!("\n🛠️  故障排除：");
        println!("  • API 失败：输入 config 更换 API Key 或切换本地模式");
        println!("  • 图片加载失败：检查路径是否正确，支持 JPG/PNG/GIF/WebP/BMP");
        println!("  • PDF 加载失败：确保已安装 poppler-utils (pdftoppm)");
        println!("  • 更多帮助：查看 README.md 文档");

        println!("\n═══════════════════════════════════════════════════════════\n");
    }

    /// 重新配置 API Key 和图纸类型
    fn reconfigure_api_key(&mut self) {
        println!("\n═══════════════════════════════════════════════════════════");
        println!("  系统配置");
        println!("═══════════════════════════════════════════════════════════");

        // 让用户选择配置选项
        println!("\n请选择操作：");
        println!("  1. 配置云端 API Key");
        println!("  2. 切换到本地模式");
        println!("  3. 查看模板自动选择说明");
        println!("  4. 查看当前配置");
        println!("  5. 返回");

        print!("\n👉 输入选项 (1/2/3/4/5): ");
        let _ = io::stdout().flush();

        let mut choice = String::new();
        if io::stdin().read_line(&mut choice).is_err() {
            return;
        }

        match choice.trim() {
            "1" => {
                print!("\n📝 请输入 Ollama API Key: ");
                let _ = io::stdout().flush();

                let mut api_key = String::new();
                if io::stdin().read_line(&mut api_key).is_ok() {
                    let api_key = api_key.trim().to_string();
                    if !api_key.is_empty() {
                        if let Err(e) = cli::save_api_key_to_env_file(&api_key) {
                            println!("⚠️  保存失败：{}", e);
                        } else {
                            println!("✅ API Key 已保存到 .env 文件");
                            // 更新客户端
                            self.client = ApiClient::cloud(&self.config.default_cloud_model, &api_key, 3);
                            println!("✅ 已切换到云端模式");
                            info!("API Key updated, switched to Cloud mode");
                            info!("API client: {}", self.client.client_name());
                            info!("Using model: {}", self.client.model());
                        }
                    } else {
                        println!("⚠️  输入为空");
                    }
                }
            }
            "2" => {
                self.client = ApiClient::local(&self.config.default_local_model, 3);
                println!("\n✅ 已切换到本地模式");
                println!("   请确保已启动本地 Ollama 服务：ollama serve");
                println!("   如需切换回云端模式，请使用 config 命令配置 API Key");
                info!("Switched to local mode");
                info!("API client: {}", self.client.client_name());
                info!("Using model: {}", self.client.model());
            }
            "3" => {
                println!("\n═══════════════════════════════════════════════════════════");
                println!("  图纸类型自动选择");
                println!("═══════════════════════════════════════════════════════════");
                println!("\nℹ️  模板自动选择功能已启用");
                println!("   系统会根据图片内容自动选择最适合的模板类型");
                println!("   无需手动选择图纸类型");
                println!("\n💡 提示：");
                println!("   - 上传涵洞图纸后，系统会自动识别类型");
                println!("   - 支持的类型包括：涵洞设置一览表、工程数量表、布置图等 18 种");
                println!("   - 置信度低于阈值时会记录警告日志");
                println!("═══════════════════════════════════════════════════════════\n");
                // 递归调用继续显示配置菜单
                self.reconfigure_api_key();
                return;
            }
            "4" => {
                if cli::load_api_key_from_env().is_some() {
                    println!("\n✅ 当前模式：云端模式");
                    println!("   API Key 已配置");
                } else {
                    println!("\n✅ 当前模式：本地模式");
                    println!("   未配置 API Key");
                }
                println!("   当前客户端：{}", self.client.client_name());
                println!("   使用模型：{}", self.client.model());
                println!("   图纸类型：{}", self.drawing_type.as_str());
                println!("   对话轮数：{}", self.dialog_manager.message_count());
                println!("   缓存统计：{}", self.image_cache.stats());
            }
            "5" => {
                println!("\n返回对话界面");
            }
            _ => {
                println!("\n⚠️  无效选项，请输入 1、2、3、4 或 5");
            }
        }

        println!("═══════════════════════════════════════════════════════════\n");
    }

    /// 导出 PDF 分析报告到 Markdown 文件
    pub fn export_pdf_report(
        &self,
        page_results: &[(String, Result<String, crate::error::Error>)],
        summary_content: Option<&str>,
        output_path: &Path,
    ) -> io::Result<()> {
        use std::fs::File;
        use std::io::BufWriter;

        let file = File::create(output_path)?;
        let mut writer = BufWriter::new(file);

        // 写入报告头
        writeln!(writer, "# 📊 CAD 图纸 PDF 分析报告")?;
        writeln!(writer)?;
        writeln!(writer, "**生成时间**: {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"))?;
        writeln!(writer, "**总页数**: {}", page_results.len())?;
        writeln!(writer)?;

        // 统计成功/失败
        let success_count = page_results.iter().filter(|(_, r)| r.is_ok()).count();
        let failed_count = page_results.iter().filter(|(_, r)| r.is_err()).count();
        let success_rate = if page_results.is_empty() { 0.0 } else { (success_count as f64 / page_results.len() as f64) * 100.0 };

        writeln!(writer, "## 📈 处理统计")?;
        writeln!(writer)?;
        writeln!(writer, "| 指标 | 数值 |")?;
        writeln!(writer, "|------|------|")?;
        writeln!(writer, "| 总页数 | {} |", page_results.len())?;
        writeln!(writer, "| 成功 | {} |", success_count)?;
        writeln!(writer, "| 失败 | {} |", failed_count)?;
        writeln!(writer, "| 成功率 | {:.1}% |", success_rate)?;
        writeln!(writer)?;

        // 目录
        writeln!(writer, "## 📑 目录")?;
        writeln!(writer)?;
        if failed_count > 0 {
            writeln!(writer, "1. [⚠️ 失败的页面](#-失败的页面)")?;
        }
        writeln!(writer, "2. [单页分析结果](#单页分析结果)")?;
        for (_idx, (page_id, _)) in page_results.iter().enumerate() {
            let page_num = page_id.split(':').last().unwrap_or("");
            writeln!(writer, "   - [第 {} 页](#第-{}-页)", page_num, page_num)?;
        }
        if summary_content.is_some() {
            writeln!(writer, "3. [📄 跨页汇总分析](#-跨页汇总分析)")?;
        }
        writeln!(writer)?;

        // 失败页面列表
        if failed_count > 0 {
            writeln!(writer, "---")?;
            writeln!(writer, "## ⚠️ 失败的页面")?;
            writeln!(writer)?;
            for (page_id, result) in page_results {
                if let Err(e) = result {
                    let page_num = page_id.split(':').last().unwrap_or("");
                    writeln!(writer, "- **第 {} 页**: {}", page_num, e)?;
                }
            }
            writeln!(writer)?;
        }

        // 单页分析结果
        writeln!(writer, "---")?;
        writeln!(writer, "## 单页分析结果")?;
        writeln!(writer)?;

        for (page_id, result) in page_results {
            let page_num = page_id.split(':').last().unwrap_or("");
            writeln!(writer, "### 第 {} 页", page_num)?;
            writeln!(writer)?;

            match result {
                Ok(content) => {
                    writeln!(writer, "{}", content)?;
                }
                Err(e) => {
                    writeln!(writer, "*分析失败：{}*", e)?;
                }
            }
            writeln!(writer)?;
            writeln!(writer, "**[返回顶部](#-cad-图纸-pdf-分析报告)**")?;
            writeln!(writer)?;
        }

        // 跨页汇总
        if let Some(summary) = summary_content {
            writeln!(writer, "---")?;
            writeln!(writer, "## 📄 跨页汇总分析")?;
            writeln!(writer)?;
            writeln!(writer, "{}", summary)?;
            writeln!(writer)?;
            writeln!(writer, "**[返回顶部](#-cad-图纸-pdf-分析报告)**")?;
            writeln!(writer)?;
        }

        // 文件尾
        writeln!(writer, "---")?;
        writeln!(writer)?;
        writeln!(writer, "*报告由 CAD 图纸识别系统自动生成*")?;
        writeln!(writer, "> 版本号：v0.10.0 | [GitHub](https://github.com/cad-ocr)")?;

        writer.flush()?;

        Ok(())
    }

    /// 导出当前对话历史到 Markdown 文件
    pub fn export_dialog_history(&self, output_path: &Path) -> io::Result<()> {
        use std::fs::File;
        use std::io::BufWriter;

        let file = File::create(output_path)?;
        let mut writer = BufWriter::new(file);

        writeln!(writer, "# 💬 对话历史记录")?;
        writeln!(writer)?;
        writeln!(writer, "**导出时间**: {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"))?;
        writeln!(writer, "**模型**: {}", self.client.model())?;
        writeln!(writer, "**模式**: {}", self.client.client_name())?;
        writeln!(writer, "**图纸类型**: {}", self.drawing_type.as_str())?;
        writeln!(writer)?;
        writeln!(writer, "---")?;
        writeln!(writer)?;

        // 获取对话历史
        let history = self.dialog_manager.get_history();

        for (idx, msg) in history.iter().enumerate() {
            match msg.role.as_str() {
                "system" => {
                    writeln!(writer, "### 🔧 系统提示")?;
                    writeln!(writer)?;
                    writeln!(writer, "```")?;
                    writeln!(writer, "{}", msg.content)?;
                    writeln!(writer, "```")?;
                    writeln!(writer)?;
                }
                "user" => {
                    writeln!(writer, "### 👤 用户 (第 {} 轮)", (idx + 1) / 2)?;
                    writeln!(writer)?;
                    writeln!(writer, "{}", msg.content)?;
                    if let Some(images) = &msg.images {
                        if !images.is_empty() {
                            writeln!(writer)?;
                            writeln!(writer, "**附加图片**: {} 张", images.len())?;
                        }
                    }
                    writeln!(writer)?;
                }
                "assistant" => {
                    writeln!(writer, "### 🤖 AI (第 {} 轮)", (idx + 1) / 2)?;
                    writeln!(writer)?;
                    writeln!(writer, "{}", msg.content)?;
                    writeln!(writer)?;
                }
                _ => {}
            }
            writeln!(writer, "---")?;
            writeln!(writer)?;
        }

        writeln!(writer, "*对话历史由 CAD 图纸识别系统导出*")?;
        writeln!(writer, "> 版本号：v0.10.0")?;

        writer.flush()?;

        Ok(())
    }
}
