use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// 配置结构
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config {
    default_image_path: String,
    default_local_model: String,
    default_cloud_model: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_image_path: "".to_string(),
            default_local_model: "llava:7b".to_string(),
            default_cloud_model: "qwen3.5:397b-cloud".to_string(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    message: ResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ResponseMessage {
    content: String,
}

/// 日志级别
enum LogLevel {
    Info,
    Warn,
    Error,
    Success,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Info => write!(f, "ℹ️ "),
            LogLevel::Warn => write!(f, "⚠️ "),
            LogLevel::Error => write!(f, "❌ "),
            LogLevel::Success => write!(f, "✅ "),
        }
    }
}

/// 带时间戳的日志输出
fn log(level: LogLevel, message: &str) {
    let timestamp = chrono::Local::now().format("%H:%M:%S");
    println!("[{}] {} {}", timestamp, level, message);
}

/// .env 文件路径
const ENV_FILE_PATH: &str = ".env";

/// 从 .env 文件加载 API Key
fn load_api_key_from_env() -> Option<String> {
    // 先尝试环境变量
    if let Ok(key) = env::var("OLLAMA_API_KEY") {
        return Some(key);
    }
    
    // 再尝试从 .env 文件读取
    dotenv::from_path(".env").ok()?;
    env::var("OLLAMA_API_KEY").ok()
}

/// 保存 API Key 到 .env 文件
fn save_api_key_to_env(api_key: &str) -> Result<(), String> {
    let env_content = format!(
        "# Ollama API Key 配置文件\n# 请勿将此文件提交到版本控制系统\nOLLAMA_API_KEY={}\n",
        api_key
    );
    
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(ENV_FILE_PATH)
        .map_err(|e| format!("无法创建 .env 文件：{}", e))?
        .write_all(env_content.as_bytes())
        .map_err(|e| format!("无法写入 .env 文件：{}", e))?;
    
    // 设置文件权限（仅 Unix 系统）
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = fs::metadata(ENV_FILE_PATH) {
            let _ = fs::set_permissions(ENV_FILE_PATH, fs::Permissions::from_mode(0o600));
        }
    }
    
    Ok(())
}

/// 获取 API Key（支持交互式保存）
fn get_api_key_interactive() -> ApiKeyResult {
    // 尝试从环境变量或 .env 文件加载
    if let Some(key) = load_api_key_from_env() {
        log(LogLevel::Success, "已从环境变量或 .env 文件加载 API Key");
        return ApiKeyResult::UseExisting(key);
    }

    log(LogLevel::Warn, "未检测到 API Key（环境变量或 .env 文件）");
    println!("\n╔═══════════════════════════════════════════════════════════╗");
    println!("║  使用 Ollama Cloud 需要 API Key                           ║");
    println!("╚═══════════════════════════════════════════════════════════╝\n");

    println!("📌 获取 API Key 的步骤:");
    println!("   1. 访问 https://ollama.com/connect");
    println!("   2. 登录你的账户");
    println!("   3. 复制显示的 API Key\n");

    print!("👉 请输入 API Key (或按回车跳过使用本地模型): ");
    let _ = io::stdout().flush();

    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        return ApiKeyResult::Skip;
    }

    let api_key = input.trim().to_string();

    if api_key.is_empty() {
        log(LogLevel::Info, "已跳过，将尝试使用本地模型");
        return ApiKeyResult::Skip;
    }

    if api_key.len() < 10 {
        log(LogLevel::Error, "API Key 格式似乎不正确");
        return ApiKeyResult::Skip;
    }

    // 询问是否保存
    print!("\n💾 是否保存 API Key 到 .env 文件？(y/n): ");
    let _ = io::stdout().flush();
    
    let mut choice = String::new();
    if io::stdin().read_line(&mut choice).is_ok() {
        match choice.trim().to_lowercase().as_str() {
            "y" | "yes" | "是" => {
                if let Err(e) = save_api_key_to_env(&api_key) {
                    log(LogLevel::Error, &format!("保存失败：{}", e));
                } else {
                    log(LogLevel::Success, "API Key 已保存到 .env 文件");
                    log(LogLevel::Info, "下次运行将自动使用此 Key");
                }
                return ApiKeyResult::UseNew(api_key);
            }
            _ => {
                log(LogLevel::Info, "本次会话将使用此 API Key（不保存）");
                return ApiKeyResult::UseNew(api_key);
            }
        }
    }

    log(LogLevel::Info, "本次会话将使用此 API Key（不保存）");
    ApiKeyResult::UseNew(api_key)
}

/// API Key 结果枚举
enum ApiKeyResult {
    UseExisting(String),  // 从已有配置加载
    UseNew(String),       // 新输入但不保存
    Skip,                 // 跳过使用本地模型
}

/// 使用 curl 调用本地 Ollama API（带对话历史）
fn try_chat_local(
    messages: &[DialogMessage],
    attached_images: &[String],
    model: &str,
) -> Result<String, String> {
    // 构建 API 消息，第一条带图片标记的消息附加所有图片
    let api_messages: Vec<ApiMessage> = messages
        .iter()
        .enumerate()
        .map(|(i, msg)| {
            if msg.content.contains("[图片：") {
                // 带图片的消息
                let content = msg.content.replace(&format!("\n\n[图片：{} 张]", attached_images.len()), "");
                ApiMessage {
                    role: msg.role.clone(),
                    content: content.trim().to_string(),
                    images: if i == 0 { Some(attached_images.to_vec()) } else { None },
                }
            } else {
                ApiMessage {
                    role: msg.role.clone(),
                    content: msg.content.clone(),
                    images: None,
                }
            }
        })
        .collect();

    let messages_json = serde_json::to_string(&api_messages)
        .map_err(|e| format!("序列化消息失败：{}", e))?;

    let request_json = format!(
        r#"{{"model": "{}", "messages": {}, "stream": false}}"#,
        model, messages_json
    );

    // Windows 上使用 cmd /c 执行 curl，通过 stdin 传递 JSON
    #[cfg(windows)]
    let mut child = Command::new("cmd")
        .args(["/C", "curl", "http://localhost:11434/api/chat", "-d", "@-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动 curl 失败：{}", e))?;

    #[cfg(not(windows))]
    let mut child = Command::new("curl")
        .args(["http://localhost:11434/api/chat", "-d", "@-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动 curl 失败：{}", e))?;

    // 写入 JSON 到 stdin
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(request_json.as_bytes())
            .map_err(|e| format!("写入数据失败：{}", e))?;
    }

    let output = child.wait_with_output()
        .map_err(|e| format!("执行 curl 失败：{}", e))?;

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        return Err(format!("HTTP 请求失败：{}", error_msg));
    }

    let response_body = String::from_utf8_lossy(&output.stdout);
    let response: ChatResponse = serde_json::from_str(&response_body)
        .map_err(|e| format!("解析响应失败：{}，原始响应：{}", e, response_body))?;

    Ok(response.message.content)
}

/// 使用 curl 调用 Ollama Cloud API（带对话历史）
fn try_chat_cloud(
    messages: &[DialogMessage],
    attached_images: &[String],
    api_key: &str,
    model: &str,
) -> Result<String, String> {
    // 构建 API 消息，第一条带图片标记的消息附加所有图片
    let api_messages: Vec<ApiMessage> = messages
        .iter()
        .enumerate()
        .map(|(i, msg)| {
            if msg.content.contains("[图片：") {
                // 带图片的消息
                let content = msg.content.replace(&format!("\n\n[图片：{} 张]", attached_images.len()), "");
                ApiMessage {
                    role: msg.role.clone(),
                    content: content.trim().to_string(),
                    images: if i == 0 { Some(attached_images.to_vec()) } else { None },
                }
            } else {
                ApiMessage {
                    role: msg.role.clone(),
                    content: msg.content.clone(),
                    images: None,
                }
            }
        })
        .collect();

    let messages_json = serde_json::to_string(&api_messages)
        .map_err(|e| format!("序列化消息失败：{}", e))?;

    let request_json = format!(
        r#"{{"model": "{}", "messages": {}, "stream": false}}"#,
        model, messages_json
    );

    let auth_header = format!("Authorization: Bearer {}", api_key);

    // Windows 上使用 cmd /c 执行 curl，通过 stdin 传递 JSON
    #[cfg(windows)]
    let mut child = Command::new("cmd")
        .args([
            "/C",
            "curl",
            "https://ollama.com/api/chat",
            "-H",
            &auth_header,
            "-d",
            "@-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动 curl 失败：{}", e))?;

    #[cfg(not(windows))]
    let mut child = Command::new("curl")
        .args([
            "https://ollama.com/api/chat",
            "-H",
            &auth_header,
            "-d",
            "@-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动 curl 失败：{}", e))?;

    // 写入 JSON 到 stdin
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(request_json.as_bytes())
            .map_err(|e| format!("写入数据失败：{}", e))?;
    }

    let output = child.wait_with_output()
        .map_err(|e| format!("执行 curl 失败：{}", e))?;

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        let status_code = output.status.code().unwrap_or(0);
        
        if status_code == 401 {
            return Err("API Key 无效或已过期".to_string());
        }
        return Err(format!("HTTP {}：{}", status_code, error_msg));
    }

    let response_body = String::from_utf8_lossy(&output.stdout);
    let response: ChatResponse = serde_json::from_str(&response_body)
        .map_err(|e| format!("解析响应失败：{}，原始响应：{}", e, response_body))?;

    Ok(response.message.content)
}

/// 对话消息
#[derive(Debug, Serialize, Deserialize, Clone)]
struct DialogMessage {
    role: String,
    content: String,
}

/// 带图片的对话消息（用于 API 请求）
#[derive(Debug, Serialize)]
struct ApiMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    images: Option<Vec<String>>,
}

/// 等待用户按回车（阻塞式，防止程序自动退出）
fn wait_for_enter(prompt: &str) {
    print!("{}", prompt);
    let _ = io::stdout().flush();

    let mut input = String::new();
    match io::stdin().read_line(&mut input) {
        Ok(_) => {},
        Err(e) => log(LogLevel::Error, &format!("读取输入失败：{}", e)),
    }
}

/// 检查是否请求关闭
fn check_shutdown(shutdown_flag: &Arc<AtomicBool>) -> bool {
    if shutdown_flag.load(Ordering::Relaxed) {
        log(LogLevel::Warn, "检测到关闭信号，正在退出...");
        true
    } else {
        false
    }
}

/// 加载配置文件
fn load_config() -> Config {
    let config_path = std::path::PathBuf::from("config.toml");
    
    if config_path.exists() {
        match fs::read_to_string(&config_path) {
            Ok(content) => {
                match toml::from_str::<Config>(&content) {
                    Ok(config) => {
                        log(LogLevel::Success, "配置文件已加载");
                        return config;
                    }
                    Err(e) => {
                        log(LogLevel::Warn, &format!("配置文件解析失败：{}，使用默认配置", e));
                    }
                }
            }
            Err(e) => {
                log(LogLevel::Warn, &format!("读取配置文件失败：{}，使用默认配置", e));
            }
        }
    }
    
    log(LogLevel::Info, "使用默认配置");
    Config::default()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 设置 Ctrl+C 处理器 - 优雅退出机制
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    let shutdown_flag_clone = Arc::clone(&shutdown_flag);

    ctrlc::set_handler(move || {
        log(LogLevel::Warn, "\n收到中断信号 (Ctrl+C)");
        shutdown_flag_clone.store(true, Ordering::Relaxed);
    }).expect("设置 Ctrl+C 处理器失败");

    println!("╔═══════════════════════════════════════════════════════════╗");
    println!("║     CAD 图纸识别 - 智能对话助手                            ║");
    println!("║     企业版 v0.1.0                                         ║");
    println!("╚═══════════════════════════════════════════════════════════╝\n");

    // 加载配置
    let config = load_config();

    let cloud_model = &config.default_cloud_model;
    let local_model = &config.default_local_model;

    // 交互式获取 API Key
    let api_key_result = get_api_key_interactive();

    if check_shutdown(&shutdown_flag) {
        wait_for_enter("\n按回车键退出...");
        return Ok(());
    }

    // 确定使用的模型和 API Key
    let (use_cloud, api_key_opt, model) = match &api_key_result {
        ApiKeyResult::UseExisting(key) | ApiKeyResult::UseNew(key) => {
            (true, Some(key), cloud_model.as_str())
        }
        ApiKeyResult::Skip => (false, None, local_model.as_str()),
    };

    // 初始化对话历史
    let mut messages: Vec<DialogMessage> = Vec::new();

    // 缓存的图片 (路径 -> base64)
    let mut image_cache: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    log(LogLevel::Info, &format!("使用模型：{}", model));
    if use_cloud {
        log(LogLevel::Info, "模式：Ollama Cloud");
    } else {
        log(LogLevel::Info, "模式：本地 Ollama");
    }

    println!("\n╔═══════════════════════════════════════════════════════════╗");
    println!("║  💬 交互式对话模式                                        ║");
    println!("║  输入你的问题，按回车发送                                 ║");
    println!("║  使用 @图片路径 附加图片 (例如：@cad_image/test.jpg)      ║");
    println!("║  输入 'quit' 或 'exit' 退出                               ║");
    println!("║  输入 'clear' 清空对话历史                                ║");
    println!("║  输入 'images' 查看已加载的图片                           ║");
    println!("╚═══════════════════════════════════════════════════════════╝\n");

    // 对话循环
    loop {
        if check_shutdown(&shutdown_flag) {
            break;
        }

        print!("👤 你：");
        let _ = io::stdout().flush();

        let mut user_input = String::new();
        if io::stdin().read_line(&mut user_input).is_err() {
            continue;
        }

        let user_input = user_input.trim();

        if user_input.is_empty() {
            continue;
        }

        // 检查退出命令
        if user_input.eq_ignore_ascii_case("quit") || user_input.eq_ignore_ascii_case("exit") {
            break;
        }

        // 检查清空命令
        if user_input.eq_ignore_ascii_case("clear") {
            messages.clear();
            image_cache.clear();
            log(LogLevel::Info, "对话历史和图片缓存已清空");
            continue;
        }

        // 检查查看图片命令
        if user_input.eq_ignore_ascii_case("images") {
            if image_cache.is_empty() {
                log(LogLevel::Info, "暂无已加载的图片");
            } else {
                println!("📷 已加载的图片:");
                for path in image_cache.keys() {
                    println!("   - {}", path);
                }
            }
            println!();
            continue;
        }

        // 解析输入中的 @图片路径
        let mut input_text = user_input.to_string();
        let mut attached_images: Vec<String> = Vec::new();

        // 查找所有 @开头的路径
        let mut parts: Vec<&str> = input_text.split_whitespace().collect();
        let mut i = 0;
        while i < parts.len() {
            if parts[i].starts_with('@') {
                let image_path = &parts[i][1..]; // 去掉 @ 符号
                if !image_cache.contains_key(image_path) {
                    // 加载图片
                    match load_image_to_base64(image_path) {
                        Ok(base64_data) => {
                            log(LogLevel::Success, &format!("已加载图片：{}", image_path));
                            image_cache.insert(image_path.to_string(), base64_data);
                        }
                        Err(e) => {
                            log(LogLevel::Error, &format!("无法加载图片 '{}': {}", image_path, e));
                            parts.remove(i);
                            continue;
                        }
                    }
                }
                if let Some(base64_data) = image_cache.get(image_path) {
                    attached_images.push(base64_data.clone());
                }
                parts.remove(i);
            } else {
                i += 1;
            }
        }

        // 重建输入文本（去掉 @路径）
        input_text = parts.join(" ");

        if input_text.trim().is_empty() && attached_images.is_empty() {
            continue;
        }

        // 添加用户消息
        messages.push(DialogMessage {
            role: "user".to_string(),
            content: if attached_images.is_empty() {
                input_text.trim().to_string()
            } else {
                format!("{}\n\n[图片：{} 张]", input_text.trim(), attached_images.len())
            },
        });

        // 调用模型
        log(LogLevel::Info, "正在思考...");

        let response = if use_cloud {
            if let Some(key) = &api_key_opt {
                try_chat_cloud(&messages, &attached_images, key, model)
            } else {
                Err("没有 API Key".to_string())
            }
        } else {
            try_chat_local(&messages, &attached_images, model)
        };

        match response {
            Ok(content) => {
                println!("\n🤖 AI: {}\n", content);

                // 添加 AI 响应到历史
                messages.push(DialogMessage {
                    role: "assistant".to_string(),
                    content: content.clone(),
                });
            }
            Err(error) => {
                if error.contains("API Key 无效") {
                    log(LogLevel::Error, "API Key 无效或已过期");
                    log(LogLevel::Info, "请删除 .env 文件并重新运行程序");
                } else if error.contains("404") {
                    log(LogLevel::Error, "模型不存在，请检查模型名称或拉取模型");
                } else {
                    log(LogLevel::Error, &error);
                }
                // 移除最后一条用户消息（因为请求失败了）
                messages.pop();
            }
        }
    }

    println!("\n感谢使用，再见！\n");
    wait_for_enter("按回车键退出...");

    Ok(())
}

/// 加载图片并转换为 base64
fn load_image_to_base64(image_path: &str) -> Result<String, String> {
    if !Path::new(image_path).exists() {
        return Err("文件不存在".to_string());
    }

    let image_data = fs::read(image_path)
        .map_err(|e| format!("读取失败：{}", e))?;

    Ok(STANDARD.encode(&image_data))
}
