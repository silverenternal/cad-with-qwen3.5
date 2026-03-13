//! 批量处理模式启动逻辑

use crate::batch;
use crate::batch::{BatchProcessor, BatchProcessorConfig, save_result};
use crate::batch_result::OutputFormat;
use crate::cli::load_config;
use crate::error::Result;
use crate::infrastructure::external::ApiClient;
use std::path::Path;
use tracing::info;

/// 启动批量处理模式
pub async fn start_batch_mode(batch_path: &str, args: &[String]) -> Result<()> {
    info!("╔═══════════════════════════════════════════════════════════╗");
    info!("║     CAD Drawing Recognition - Batch Mode v0.10.0          ║");
    info!("╚═══════════════════════════════════════════════════════════╝");

    // 加载配置获取默认并发数
    let config = load_config();

    // 从 preset 获取并发配置
    let concurrency_config = config.get_concurrency_config();

    // 解析命令行参数
    let mut output_path: Option<String> = None;
    let mut progress_path: Option<String> = None;
    let mut concurrency: usize = concurrency_config.batch_concurrency;
    let mut drawing_type = config.default_drawing_type.clone();
    let mut question = config.default_batch_question.clone();
    let mut output_format = OutputFormat::Json;
    let mut pdfs_per_batch: usize = 5;
    let mut max_pages_per_pdf: usize = 0;
    let mut use_streaming = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--output" | "-o" => {
                if let Some(path) = args.get(i + 1) {
                    output_path = Some(path.clone());
                    i += 1;
                }
            }
            "--resume" | "-r" => {
                if let Some(path) = args.get(i + 1) {
                    progress_path = Some(path.clone());
                    i += 1;
                }
            }
            "--concurrency" | "-c" => {
                if let Some(val) = args.get(i + 1) {
                    if let Ok(n) = val.parse::<usize>() {
                        concurrency = n;
                    }
                    i += 1;
                }
            }
            "--type" | "-t" => {
                if let Some(val) = args.get(i + 1) {
                    drawing_type = val.clone();
                    i += 1;
                }
            }
            "--question" | "-q" => {
                if let Some(val) = args.get(i + 1) {
                    question = val.clone();
                    i += 1;
                }
            }
            "--format" | "-f" => {
                if let Some(val) = args.get(i + 1) {
                    output_format = val.parse().unwrap_or_default();
                    i += 1;
                }
            }
            "--batch-size" => {
                if let Some(val) = args.get(i + 1) {
                    if let Ok(n) = val.parse::<usize>() {
                        pdfs_per_batch = n;
                    }
                    i += 1;
                }
            }
            "--max-pages" => {
                if let Some(val) = args.get(i + 1) {
                    if let Ok(n) = val.parse::<usize>() {
                        max_pages_per_pdf = n;
                    }
                    i += 1;
                }
            }
            "--streaming" | "-s" => {
                use_streaming = true;
            }
            "--help" | "-h" => {
                print_batch_help();
                return Ok(());
            }
            _ => {}
        }
        i += 1;
    }

    // 确定输出路径
    let output_path = output_path.unwrap_or_else(|| {
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        format!("batch_results_{}.{}", timestamp, output_format.extension())
    });

    // 确定进度文件路径
    let progress_file = progress_path.unwrap_or_else(|| {
        let output_dir = std::path::Path::new(&output_path)
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf();
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let progress_name = format!(".batch_progress_{}.json", timestamp);
        output_dir.join(progress_name).to_string_lossy().to_string()
    });

    info!("Batch processing directory: {}", batch_path);
    info!("Concurrency: {}", concurrency);
    info!("Drawing type: {}", drawing_type);
    info!("Question: {}", question);
    info!("Output format: {}", output_format);
    info!("Output path: {}", output_path);
    info!("Progress file: {}", progress_file);
    info!("PDFs per batch: {}", pdfs_per_batch);
    info!("Max pages per PDF: {}", max_pages_per_pdf);
    info!("Streaming mode: {}", use_streaming);
    info!("Use --resume {} to resume from interruption", progress_file);

    // 加载配置和 API Key
    let config = load_config();
    let api_key = crate::cli::get_api_key_interactive();

    // 创建 API 客户端
    let client = if let Some(key) = &api_key {
        info!("使用 Ollama Cloud API");
        ApiClient::cloud(&config.default_cloud_model, key, 3)
    } else {
        info!("使用本地 Ollama API");
        ApiClient::local(&config.default_local_model, 3)
    };

    info!("已初始化 API 客户端：{}", client.client_name());
    info!("使用模型：{}", client.model());

    // 创建批量处理器配置
    let processor_config = BatchProcessorConfig {
        session_pool_size: concurrency,
        encoding_concurrency: (concurrency / 2).max(1),
        api_concurrency: concurrency,
        max_image_dimension: config.max_image_dimension,
        max_retries: 3,
        base_delay_ms: 100,
        drawing_type,
        question,
        user_id: None,
        enable_quota_check: false,
        circuit_breaker: batch::circuit_breaker::CircuitBreakerConfig::default(),
        dead_letter_queue_path: None,
        working_dir: Some(Path::new(batch_path).to_path_buf()),
        template_selection: batch::BatchTemplateSelectionConfig {
            enabled: false,
            strategy: "hybrid".to_string(),
            model: "llava:7b".to_string(),
            confidence_threshold: 0.6,
            low_confidence_fallback: "default_type".to_string(),
            default_type: "culvert_layout".to_string(),
            enable_cache: true,
            cache_max_entries: 1000,
        },
    };

    // 创建批量处理器
    let processor = BatchProcessor::new(client, processor_config);

    // 处理目录
    let batch_path = Path::new(batch_path);
    let output_path = Path::new(&output_path);
    let progress_path = Path::new(&progress_file);

    let mut result = processor
        .process_directory(batch_path, Some(output_path), Some(progress_path))
        .await?;

    // 完成处理
    result.finish();

    // 保存最终结果
    save_result(&result, output_path, output_format)?;

    // 打印摘要
    println!("\n═══════════════════════════════════════════════════════════");
    println!("Batch processing completed!");
    println!("  Total files: {}", result.total);
    println!("  Success: {}", result.success);
    println!("  Failed: {}", result.failed);
    if let Some(rate) = result.stats.get("success_rate") {
        println!("  Success rate: {}%", rate);
    }
    if let Some(avg) = result.stats.get("avg_latency_ms") {
        println!("  Avg latency: {}ms", avg);
    }
    println!("  Results saved to: {}", output_path.display());
    println!("═══════════════════════════════════════════════════════════\n");

    Ok(())
}

/// 打印批处理帮助信息
pub fn print_batch_help() {
    println!(
        r#"
CAD Drawing Recognition - 批处理模式用法

用法:
  cargo run --release -- --batch <目录> [选项]

必选参数:
  --batch, -b <目录>       要处理的 PDF/图片目录

可选参数:
  --output, -o <路径>      输出文件路径（默认：batch_results_时间戳.json）
  --resume, -r <路径>      从进度文件恢复（断点续传）
  --concurrency, -c <数字> 并发数（默认：从 config.toml 读取 batch_preset）
  --type, -t <类型>        图纸类型（默认：从 config.toml 读取）
  --question, -q <问题>    提示词（默认：从 config.toml 读取）
  --format, -f <格式>      输出格式：json 或 csv（默认：json）
  --batch-size <数字>      每批 PDF 数量（默认：5）
  --max-pages <数字>       每 PDF 最大页数（默认：无限制）
  --streaming, -s          启用流式处理模式（推荐用于大文件）
  --report-dir <目录>      报告输出目录（默认：当前目录）
  --no-dynamic-concurrency 禁用动态并发调整
  --min-concurrency <数字> 最小并发数（默认：1）
  --max-concurrency <数字> 最大并发数（默认：8）
  --target-latency <毫秒>  目标响应时间（默认：3000ms）
  --help, -h               显示此帮助信息

批处理预设档位（在 config.toml 中设置 batch_preset）:
  - fast:       并发 2，适合测试和小批量（<10 文件）
  - balanced:   并发 4，推荐默认值，适合中等批量（10-50 文件）
  - aggressive: 并发 8，适合大批量处理（>50 文件）

示例:
  # 基本用法
  cargo run --release -- --batch ./cad_images/

  # 指定输出和并发数
  cargo run --release -- --batch ./cad_images/ -o results.json -c 4

  # 流式处理大文件（推荐 80+ PDF）
  cargo run --release -- --batch ./pdfs/ --streaming --batch-size 5 -c 2

  # 断点续传
  cargo run --release -- --batch ./pdfs/ --resume .batch_progress_xxx.json

  # 限制每 PDF 处理页数
  cargo run --release -- --batch ./pdfs/ --max-pages 3

注意:
  - 处理大量 PDF 时建议使用 --streaming 模式
  - 进度文件会自动保存，中断后可使用 --resume 恢复
  - 确保 .env 文件中配置了 OLLAMA_API_KEY（云端模式）
  - 推荐在 config.toml 中设置 batch_preset = "balanced" 简化配置
"#
    );
}
