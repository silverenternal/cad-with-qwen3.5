//! 命令行交互模块

use tracing::{info, warn};
use std::env;
use std::io::{self, Write};
use crate::config::Config;

/// 从环境变量或 .env 文件加载 API Key
pub fn load_api_key_from_env() -> Option<String> {
    if let Ok(key) = env::var("OLLAMA_API_KEY") {
        return Some(key);
    }
    dotenvy::from_path(".env").ok()?;
    env::var("OLLAMA_API_KEY").ok()
}

/// 保存 API Key 到 .env 文件
pub fn save_api_key_to_env_file(api_key: &str) -> io::Result<()> {
    use std::fs;
    use std::path::Path;

    let env_path = Path::new(".env");

    // 读取现有内容（如果存在）
    let mut content = String::new();
    if env_path.exists() {
        content = fs::read_to_string(env_path)
            .unwrap_or_else(|_| String::new());
    }
    
    // 检查是否已存在 OLLAMA_API_KEY
    if content.contains("OLLAMA_API_KEY=") {
        // 更新现有值
        let lines: Vec<&str> = content.lines().collect();
        let mut new_lines: Vec<String> = Vec::new();
        for line in lines {
            if line.starts_with("OLLAMA_API_KEY=") {
                new_lines.push(format!("OLLAMA_API_KEY={}", api_key));
            } else {
                new_lines.push(line.to_string());
            }
        }
        content = new_lines.join("\n");
    } else {
        // 添加新值
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(&format!("OLLAMA_API_KEY={}", api_key));
    }
    
    fs::write(env_path, content)?;
    info!("API Key saved to .env file");
    Ok(())
}

/// 获取 API Key（支持交互式配置）
pub fn get_api_key_interactive() -> Option<String> {
    if let Some(key) = load_api_key_from_env() {
        info!("Loaded API Key from environment variable");
        println!("\n✅ 已检测到 API Key，使用云端模式");
        return Some(key);
    }

    println!("\n⚠️  未检测到 API Key");
    println!("请选择使用模式：");
    println!("  1. 本地模式（使用本地 Ollama 服务）");
    println!("  2. 云端模式（配置 API Key）");
    println!("  3. 稍后配置（跳过）");
    
    loop {
        print!("\n👉 输入选项 (1/2/3): ");
        io::stdout().flush().ok();
        
        let mut choice = String::new();
        if io::stdin().read_line(&mut choice).is_err() {
            return None;
        }
        
        match choice.trim() {
            "1" => {
                info!("Using local Ollama model");
                println!("\n✅ 已选择本地模式");
                println!("   请确保已启动本地 Ollama 服务：ollama serve");
                return None;
            }
            "2" => {
                print!("\n📝 请输入 Ollama API Key: ");
                io::stdout().flush().ok();
                
                let mut api_key = String::new();
                if io::stdin().read_line(&mut api_key).is_ok() {
                    let api_key = api_key.trim().to_string();
                    if !api_key.is_empty() {
                        // 保存到 .env 文件
                        if let Err(e) = save_api_key_to_env_file(&api_key) {
                            warn!("Failed to save API Key: {}", e);
                            println!("⚠️  保存失败，请手动在 .env 文件中设置 OLLAMA_API_KEY");
                        } else {
                            println!("✅ API Key 已保存到 .env 文件");
                        }
                        info!("Loaded API Key from user input");
                        return Some(api_key);
                    }
                }
                println!("⚠️  输入为空，请重新选择");
            }
            "3" => {
                info!("No API Key provided, using local Ollama model");
                println!("\n⚠️  已跳过，将使用本地模式");
                println!("   如需使用云端模式，请在 .env 文件中设置 OLLAMA_API_KEY");
                println!("   获取 API Key: https://ollama.com/connect");
                return None;
            }
            _ => {
                println!("无效选项，请输入 1、2 或 3");
            }
        }
    }
}

/// 加载配置
pub fn load_config() -> Config {
    let config = crate::config::load_config();
    
    // 验证配置
    if let Err(e) = crate::config::ConfigManager::validate(&config) {
        warn!("配置验证警告：{}", e);
        warn!("使用默认配置启动，建议检查 config.toml 文件");
    }
    // 打印配置摘要
    crate::config::ConfigManager::print_summary(&config);
    config
}

/// API 出错时提示用户更换 API Key
/// 返回：Some(新 API Key) 表示用户输入了新 Key，None 表示跳过或切换到本地模式
pub fn prompt_change_api_key() -> Option<String> {
    println!("\n═══════════════════════════════════════════════════════════");
    println!("⚠️  API 请求失败");
    println!("═══════════════════════════════════════════════════════════");
    println!("\n可能的原因：");
    println!("  • API Key 无效或已过期");
    println!("  • 网络连接问题");
    println!("  • 服务端暂时不可用");
    println!("\n请选择操作：");
    println!("  1. 更换 API Key");
    println!("  2. 切换到本地模式（使用本地 Ollama 服务）");
    println!("  3. 跳过（继续尝试使用当前配置）");

    loop {
        print!("\n👉 输入选项 (1/2/3): ");
        io::stdout().flush().ok();

        let mut choice = String::new();
        if io::stdin().read_line(&mut choice).is_err() {
            return None;
        }

        match choice.trim() {
            "1" => {
                print!("\n📝 请输入新的 Ollama API Key: ");
                io::stdout().flush().ok();

                let mut api_key = String::new();
                if io::stdin().read_line(&mut api_key).is_ok() {
                    let api_key = api_key.trim().to_string();
                    if !api_key.is_empty() {
                        // 保存到 .env 文件
                        if let Err(e) = save_api_key_to_env_file(&api_key) {
                            warn!("Failed to save API Key: {}", e);
                            println!("⚠️  保存失败，请手动在 .env 文件中设置 OLLAMA_API_KEY");
                        } else {
                            println!("✅ API Key 已保存到 .env 文件");
                        }
                        return Some(api_key);
                    }
                }
                println!("⚠️  输入为空，请重新选择");
            }
            "2" => {
                println!("\n✅ 已切换到本地模式");
                println!("   请确保已启动本地 Ollama 服务：ollama serve");
                return None;
            }
            "3" => {
                println!("\n⚠️  已跳过，将继续使用当前配置");
                return None;
            }
            _ => {
                println!("无效选项，请输入 1、2 或 3");
            }
        }
    }
}

/// 打印 CLI 欢迎信息和使用提示
pub fn print_welcome() {
    println!("\n╔═══════════════════════════════════════════════════════════╗");
    println!("║        CAD 图纸识别 - CLI 交互模式 v0.10.0              ║");
    println!("║     基于 Qwen3.5 多模态大模型的 CAD 图纸智能分析工具          ║");
    println!("╚═══════════════════════════════════════════════════════════╝");

    println!("\n📖 基本用法：");
    println!("  • 使用 @路径 附加图片，例如：@cad_image/plan.jpg");
    println!("  • 支持多图片对比分析：@plan1.jpg @plan2.jpg 对比差异");
    println!("  • 支持连续对话，上下文自动保留");
    println!("  • 支持 PDF 文件：@drawing.pdf 自动转换为图片");

    println!("\n📋 内置命令：");
    println!("  • help / h              - 查看本帮助信息");
    println!("  • clear / cls           - 清空对话历史");
    println!("  • stats / status        - 查看统计信息（请求数、成功率等）");
    println!("  • config / cfg          - 配置 API Key 或切换模式");
    println!("  • history / hist [N]    - 查看最近 N 条对话记录（默认 10）");
    println!("  • export [文件路径]     - 导出对话历史为 JSON");
    println!("  • quit / exit / q       - 退出程序");

    println!("\n🔧 API 配置：");
    println!("  • 云端模式：设置 .env 文件中的 OLLAMA_API_KEY 或使用 config 命令");
    println!("    - 模型：qwen3.5:397b-cloud（多模态大模型）");
    println!("    - 获取 API Key: https://ollama.com/connect");
    println!("  • 本地模式：启动本地 Ollama 服务 (ollama serve)");
    println!("    - 模型：llava:7b（本地多模态模型）");
    println!("    - 需自行下载：ollama pull llava:7b");

    println!("\n🤖 模板自动选择：");
    println!("  • 系统会自动识别涵洞图纸类型，无需手动选择");
    println!("  • 支持的类型包括：");
    println!("    - 表格类：涵洞设置一览表、工程数量表");
    println!("    - 布置图类：涵洞布置图、暗涵一般布置图");
    println!("    - 钢筋构造图：2m/3m/4m 孔径箱涵钢筋图");
    println!("    - 斜涵类：30°斜度钢筋构造图");
    println!("    - 细部构造：防水、止水带、帽石钢筋等");
    println!("    - 方案图：涵长调整方案图（一/二/三）");

    println!("\n💡 使用示例：");
    println!("  单图分析：");
    println!("    @cad_image/plan.jpg 分析这个户型有几个房间？");
    println!("    @structure.jpg 提取所有梁柱尺寸和配筋信息");
    println!("  PDF 分析：");
    println!("    @drawing.pdf 分析这张涵洞图纸");
    println!("  多图对比：");
    println!("    @plan_v1.jpg @plan_v2.jpg 对比这两个方案的差异");
    println!("    @floor1.jpg @floor2.jpg @floor3.jpg 分析各层户型变化");
    println!("  连续对话：");
    println!("    @plan.jpg 这张图纸的比例尺是多少？");
    println!("    （接着问）有哪些房间？每个房间的面积是多少？");

    println!("\n📁 批量处理模式：");
    println!("  使用命令行参数启动批量处理：");
    println!("  cargo run --release -- --batch ./cad_images/ \\");
    println!("    --output results.json \\");
    println!("    --concurrency 8 \\");
    println!("    --question \"提取所有梁柱尺寸和配筋信息\"");
    println!("  详见：cargo run --release -- --help");

    println!("\n🌐 Web API 服务器模式：");
    println!("  启动 HTTP 服务器提供 REST API：");
    println!("  cargo run --release -- --server");
    println!("  访问：http://localhost:3000/swagger-ui/ 查看 API 文档");

    println!("\n⚙️ 配置文件：");
    println!("  • config.toml - 修改默认模型、并发数、配额等配置");
    println!("  • .env - 配置 API Key、数据库连接等敏感信息");
    println!("  • templates/prompt.toml - 自定义提示词模板");

    println!("\n📝 注意事项：");
    println!("  • 图片格式：支持 JPG, PNG, GIF, WebP, BMP, PDF");
    println!("  • 图片大小：单张最大 10MB，超出会自动压缩");
    println!("  • 图片路径：支持相对路径和绝对路径");
    println!("  • 网络要求：云端模式需要稳定的网络连接");
    println!("  • 配额限制：云端 API 有每日请求限额，请注意查看");
    println!("  • PDF 转换：需要安装 poppler-utils (pdftoppm)");

    println!("\n🛠️  故障排除：");
    println!("  • API 请求失败：检查 API Key 是否有效，或切换到本地模式");
    println!("  • 图片无法识别：检查图片格式是否正确，或尝试重新截图");
    println!("  • PDF 转换失败：请安装 poppler-utils 或 pdftoppm 工具");
    println!("  • 响应速度慢：降低并发数或使用本地模型");
    println!("  • 更多帮助：查看 README.md 或访问项目文档");

    println!("\n═══════════════════════════════════════════════════════════");
    println!("💡 提示：首次使用建议输入 `help` 查看详细说明");
    println!("═══════════════════════════════════════════════════════════\n");
}
