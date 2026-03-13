//! CLI 模式启动逻辑

use crate::app::App;
use crate::cli;
use crate::cli::load_config;
use crate::error::{Error, Result};
use crate::infrastructure::external::ApiClient;
use crate::telemetry::TelemetryRecorder;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{error, info, warn};

/// 启动 CLI 模式
pub async fn start_cli_mode() -> Result<()> {
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    let shutdown_flag_clone = Arc::clone(&shutdown_flag);

    // Ctrl+C handler failure is extremely rare (only fails if handler already set)
    if let Err(e) = ctrlc::set_handler(move || {
        warn!("Received shutdown signal (Ctrl+C)");
        shutdown_flag_clone.store(true, Ordering::Relaxed);
    }) {
        error!("Failed to set Ctrl+C handler: {}", e);
        return Err(Error::Internal(e.to_string()));
    }

    // 打印欢迎信息和使用提示
    cli::print_welcome();

    let config = load_config();
    let api_key = cli::get_api_key_interactive();

    // Initialize telemetry recorder
    let telemetry = TelemetryRecorder::new(None);
    info!("Telemetry recorder initialized");
    info!("User ID: {}", telemetry.user_id());
    info!("Session ID: {}", telemetry.session_id());

    // Record app startup event
    telemetry.log_action("app_started", std::collections::HashMap::new()).await;

    // Create API client
    let client = if let Some(key) = &api_key {
        info!("Using Ollama Cloud API");
        ApiClient::cloud(&config.default_cloud_model, key, 3)
    } else {
        info!("Using local Ollama API");
        ApiClient::local(&config.default_local_model, 3)
    };

    info!("API client initialized: {}", client.client_name());
    info!("Using model: {}", client.model());

    let mut app = App::new(client, &config, telemetry)
        .map_err(|e| Error::Internal(e.to_string()))?;

    // Select drawing type and initialize prompt
    if let Err(e) = app.init_prompt_template() {
        error!("Failed to initialize prompt template: {}", e);
        return Err(Error::Internal(e.to_string()));
    }

    // Run main loop
    if let Err(e) = app.run(&shutdown_flag).await {
        error!("Application error: {}", e);
        return Err(Error::Internal(e.to_string()));
    }

    info!("Thank you for using. Goodbye!");
    Ok(())
}
