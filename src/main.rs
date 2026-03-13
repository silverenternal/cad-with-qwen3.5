//! CAD Drawing Recognition - Main Entry Point
//!
//! 支持三种运行模式：
//! - CLI 模式：交互式命令行
//! - Server 模式：Web API 服务器
//! - Batch 模式：批量处理

mod app;
mod batch;
mod batch_result;
mod cache;
mod cli;
mod config;
mod db;
mod dialog;
mod error;
mod metrics;
mod pdf_utils;
mod prompt;
mod recognition_validator;
mod security;
mod server;
mod telemetry;

// 领域层和基础设施层模块
mod domain;
mod infrastructure;
mod application;

// 启动模式模块
mod batch_main;
mod cli_main;
mod server_main;

use error::Result;
use tracing_subscriber::{util::SubscriberInitExt, EnvFilter, Layer};

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化 tracing
    init_tracing();

    let args: Vec<String> = std::env::args().collect();

    // 检查是否启动 Web 服务器模式
    if args.iter().any(|arg| arg == "--server" || arg == "-s") {
        return server_main::start_server_mode().await;
    }

    // 检查是否批量处理模式
    if let Some(batch_idx) = args.iter().position(|arg| arg == "--batch" || arg == "-b") {
        if let Some(batch_path) = args.get(batch_idx + 1) {
            return batch_main::start_batch_mode(batch_path, &args).await;
        } else {
            eprintln!("错误：--batch 需要指定目录路径");
            eprintln!("用法：cargo run --release -- --batch ./cad_images/ [--output results.json]");
            std::process::exit(1);
        }
    }

    // CLI 模式
    cli_main::start_cli_mode().await
}

/// 初始化 tracing 订阅者
fn init_tracing() {
    use tracing_subscriber::fmt::format::FmtSpan;
    use tracing_subscriber::{layer::SubscriberExt, Registry};

    // 检查是否启用 JSON 格式输出
    let use_json = std::env::var("LOG_FORMAT")
        .map(|v| v.eq_ignore_ascii_case("json"))
        .unwrap_or(false);

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,cad_ocr=debug"));

    let fmt_layer = if use_json {
        // JSON 格式输出，适合 ELK/Splunk 等日志聚合系统
        tracing_subscriber::fmt::layer()
            .json()
            .with_target(true)
            .with_thread_ids(true)
            .with_thread_names(true)
            .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
            .boxed()
    } else {
        // 人类可读格式输出
        tracing_subscriber::fmt::layer()
            .with_target(true)
            .with_thread_ids(false)
            .with_thread_names(false)
            .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
            .pretty()
            .boxed()
    };

    Registry::default()
        .with(filter)
        .with(fmt_layer)
        .init();
}
