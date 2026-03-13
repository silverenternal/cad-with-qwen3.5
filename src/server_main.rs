//! Web 服务器模式启动逻辑

use crate::cli::load_config;
use crate::error::Result;
use crate::server::start_server;
use tracing::{info, warn};

/// 启动 Web 服务器模式
pub async fn start_server_mode() -> Result<()> {
    info!("╔═══════════════════════════════════════════════════════════╗");
    info!("║     CAD Drawing Recognition - Web API Server v0.8.0       ║");
    info!("╚═══════════════════════════════════════════════════════════╝");

    let config = load_config();

    // 检查外部依赖
    check_external_dependencies(&config);

    // 从环境变量或命令行获取端口
    let port = std::env::var("SERVER_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3000);

    info!("Server starting on port {}", port);
    start_server(config, port).await?;

    Ok(())
}

/// 检查外部依赖并给出友好提示
fn check_external_dependencies(config: &crate::config::Config) {
    // 检查 PDF 转换工具
    if config.pdf_conversion_enabled {
        match std::process::Command::new("pdftoppm")
            .arg("--version")
            .output()
        {
            Ok(output) if output.status.success() => {
                let version = String::from_utf8_lossy(&output.stdout);
                info!("✓ PDF 转换工具已安装：pdftoppm {}", version.lines().next().unwrap_or("unknown"));
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!("⚠ pdftoppm 执行失败：{}", stderr);
                warn!("  如需启用 PDF 转换功能，请安装 poppler-utils：");
                warn!("  - Ubuntu/Debian: sudo apt-get install poppler-utils");
                warn!("  - macOS: brew install poppler");
                warn!("  - Windows: 下载预编译二进制并添加到 PATH");
            }
            Err(e) => {
                warn!("⚠ pdftoppm 未安装或不在 PATH 中：{}", e);
                warn!("  如需启用 PDF 转换功能，请安装 poppler-utils：");
                warn!("  - Ubuntu/Debian: sudo apt-get install poppler-utils");
                warn!("  - macOS: brew install poppler");
                warn!("  - Windows: 下载预编译二进制并添加到 PATH");
                warn!("  或者在 config.toml 中设置 pdf_conversion_enabled = false 禁用此功能");
            }
        }
    } else {
        info!("ℹ PDF 转换功能已禁用（pdf_conversion_enabled = false）");
    }
}
