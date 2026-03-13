//! 遥测模块 - 支持 WAL 预写日志

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::fs::OpenOptions;
use tracing::{warn, info};

use crate::db::Database;

/// 数据库连接池类型别名（可选）
pub type DbPool = Arc<dyn Database>;

/// 遥测事件类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TelemetryEvent {
    /// API 请求事件
    ApiRequest {
        endpoint: String,
        latency_ms: u64,
        success: bool,
        user_id: Option<String>,
        model_used: Option<String>,
    },
    /// 错误事件
    Error {
        error_code: String,
        error_message: String,
        context: serde_json::Value,
    },
    /// 用户操作事件
    UserAction {
        action: String,
        user_id: Option<String>,
        metadata: HashMap<String, serde_json::Value>,
    },
}

/// 遥测统计数据（使用 db.rs 中的类型）
pub use crate::db::TelemetryStats;

/// 遥测记录器
pub struct TelemetryRecorder {
    user_id: String,
    session_id: String,
    events: Arc<Mutex<Vec<TelemetryEvent>>>,
    start_time: u64,
    stats: Arc<Mutex<TelemetryStats>>,
    db: Option<Arc<dyn Database>>,
    /// WAL 文件路径（可选）
    wal_path: Option<PathBuf>,
}

impl TelemetryRecorder {
    /// 创建新的遥测记录器（不带数据库）
    pub fn new(user_id: Option<String>) -> Self {
        let user_id = user_id.unwrap_or_else(generate_anonymous_id);
        let session_id = generate_session_id();
        let start_time = current_timestamp_ms();

        Self {
            user_id,
            session_id,
            events: Arc::new(Mutex::new(Vec::new())),
            start_time,
            stats: Arc::new(Mutex::new(TelemetryStats::default())),
            db: None,
            wal_path: None,
        }
    }

    /// 创建带数据库的遥测记录器
    pub fn with_database(user_id: Option<String>, db: Arc<dyn Database>) -> Self {
        let user_id = user_id.unwrap_or_else(generate_anonymous_id);
        let session_id = generate_session_id();
        let start_time = current_timestamp_ms();

        Self {
            user_id,
            session_id,
            events: Arc::new(Mutex::new(Vec::new())),
            start_time,
            stats: Arc::new(Mutex::new(TelemetryStats::default())),
            db: Some(db),
            wal_path: None,
        }
    }

    /// 创建带 WAL 的遥测记录器
    pub fn with_wal(user_id: Option<String>, wal_path: PathBuf) -> Self {
        let user_id = user_id.unwrap_or_else(generate_anonymous_id);
        let session_id = generate_session_id();
        let start_time = current_timestamp_ms();

        let recorder = Self {
            user_id,
            session_id,
            events: Arc::new(Mutex::new(Vec::new())),
            start_time,
            stats: Arc::new(Mutex::new(TelemetryStats::default())),
            db: None,
            wal_path: Some(wal_path.clone()),
        };

        // 恢复未完成的 WAL 事件
        recorder.recover_from_wal();

        recorder
    }

    /// 记录 API 请求
    pub async fn log_request(
        &self,
        endpoint: &str,
        latency_ms: u64,
        success: bool,
        model_used: Option<&str>,
    ) {
        let event = TelemetryEvent::ApiRequest {
            endpoint: endpoint.to_string(),
            latency_ms,
            success,
            user_id: Some(self.user_id.clone()),
            model_used: model_used.map(|s| s.to_string()),
        };
        self.push_event(event).await;

        // 更新统计数据
        self.update_stats(success, latency_ms).await;

        // 如果启用了数据库，持久化到数据库
        if let Some(ref db) = self.db {
            if let Err(e) = db
                .log_event(
                    "api_request",
                    Some(&self.user_id),
                    Some(&self.session_id),
                    Some(endpoint),
                    Some(latency_ms as i64),
                    Some(success),
                    model_used,
                    None,
                    None,
                    None,
                    self.uptime_ms(),
                )
                .await
            {
                warn!("Failed to record telemetry event to database: {}", e);
            }
        }

        // 使用 tracing 输出结构化日志
        tracing::info!(
            target: "telemetry",
            endpoint = %endpoint,
            latency_ms = %latency_ms,
            success = %success,
            "API request completed"
        );
    }

    /// 更新统计数据
    async fn update_stats(&self, success: bool, latency_ms: u64) {
        let mut stats = self.stats.lock().await;
        stats.total_requests += 1;
        if success {
            stats.successful_requests += 1;
        } else {
            stats.failed_requests += 1;
        }
        // 计算移动平均延迟
        let total = stats.total_requests as f64;
        let current_avg = stats.avg_latency_ms;
        stats.avg_latency_ms = current_avg + ((latency_ms as f64 - current_avg) / total);
    }

    /// 记录错误
    pub async fn log_error(&self, error_code: &str, error_message: &str, context: serde_json::Value) {
        let event = TelemetryEvent::Error {
            error_code: error_code.to_string(),
            error_message: error_message.to_string(),
            context,
        };
        self.push_event(event).await;

        tracing::error!(
            target: "telemetry",
            error_code = %error_code,
            error_message = %error_message,
            "Error occurred"
        );
    }

    /// 记录用户操作
    pub async fn log_action(&self, action: &str, metadata: HashMap<String, serde_json::Value>) {
        let event = TelemetryEvent::UserAction {
            action: action.to_string(),
            user_id: Some(self.user_id.clone()),
            metadata,
        };
        self.push_event(event).await;
    }

    async fn push_event(&self, event: TelemetryEvent) {
        // 1. 先写入 WAL（预写日志）
        if let Some(ref wal_path) = self.wal_path {
            if let Err(e) = self.append_to_wal(wal_path, &event).await {
                warn!("WAL write failed: {}", e);
            }
        }

        // 2. 再写入内存
        let mut events = self.events.lock().await;
        events.push(event);
    }

    /// 追加事件到 WAL 文件
    async fn append_to_wal(&self, path: &PathBuf, event: &TelemetryEvent) -> std::io::Result<()> {
        // 确保目录存在
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // 追加模式打开文件
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await?;

        let mut writer = BufWriter::new(file);

        // 写入 JSONL 格式（每行一个 JSON）
        let json = serde_json::to_string(event)?;
        writer.write_all(json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        Ok(())
    }

    /// 从 WAL 恢复未完成的事件
    fn recover_from_wal(&self) {
        if let Some(ref wal_path) = self.wal_path {
            if !wal_path.exists() {
                return;
            }

            match std::fs::read_to_string(wal_path) {
                Ok(content) => {
                    // 使用 futures 阻塞获取锁
                    let mut events_guard = futures::executor::block_on(self.events.lock());
                    
                    for line in content.lines() {
                        if line.trim().is_empty() {
                            continue;
                        }
                        match serde_json::from_str::<TelemetryEvent>(line) {
                            Ok(event) => events_guard.push(event),
                            Err(e) => warn!("WAL recovery failed for line: {}", e),
                        }
                    }
                    info!("WAL recovered: {} events", events_guard.len());
                }
                Err(e) => warn!("WAL read failed: {}", e),
            }
        }
    }

    /// 获取用户 ID
    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    /// 获取会话 ID
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// 获取运行时长（毫秒）
    pub fn uptime_ms(&self) -> u64 {
        current_timestamp_ms() - self.start_time
    }

    /// 获取统计数据
    pub async fn get_stats(&self) -> TelemetryStats {
        // 如果启用了数据库，从数据库获取统计
        if let Some(ref db) = self.db {
            match db.get_stats().await {
                Ok(stats) => {
                    return stats;
                }
                Err(e) => {
                    warn!("Failed to get stats from database: {}", e);
                    // Fallback to memory stats
                }
            }
        }

        // 从内存获取统计
        self.stats.lock().await.clone()
    }

    /// 清空 WAL 文件（在数据成功持久化后调用）
    pub async fn clear_wal(&self) {
        if let Some(ref wal_path) = self.wal_path {
            if wal_path.exists() {
                match tokio::fs::remove_file(wal_path).await {
                    Ok(_) => info!("WAL cleared: {}", wal_path.display()),
                    Err(e) => warn!("WAL clear failed: {}", e),
                }
            }
        }
    }

    /// 导出所有事件（用于持久化或发送）
    pub async fn export_events(&self) -> Vec<TelemetryExport> {
        let events = self.events.lock().await;
        events.iter().map(|e| TelemetryExport {
            event: e.clone(),
            user_id: self.user_id.clone(),
            session_id: self.session_id.clone(),
            timestamp: current_timestamp_ms(),
            uptime_ms: self.uptime_ms(),
        }).collect()
    }

    /// 清空已记录的事件
    pub async fn clear_events(&self) {
        let mut events = self.events.lock().await;
        events.clear();
    }
}

/// 导出的遥测数据结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryExport {
    pub event: TelemetryEvent,
    pub user_id: String,
    pub session_id: String,
    pub timestamp: u64,
    pub uptime_ms: u64,
}

impl TelemetryExport {
    /// 转换为 JSON 字符串
    pub fn to_json(&self) -> String {
        json!(self).to_string()
    }
}

/// 生成匿名 ID
fn generate_anonymous_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    // 系统时钟回退概率极低，unwrap 是合理的
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    format!("anon_{}", timestamp)
}

/// 生成会话 ID
fn generate_session_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    // 系统时钟回退概率极低，unwrap 是合理的
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    format!("sess_{}", timestamp)
}

/// 获取当前时间戳（毫秒）
fn current_timestamp_ms() -> u64 {
    // 系统时钟回退概率极低，unwrap 是合理的
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// 全局遥测记录器（可选，用于应用级别共享）
pub struct GlobalTelemetry {
    recorder: Option<Arc<TelemetryRecorder>>,
}

impl GlobalTelemetry {
    pub fn new() -> Self {
        Self { recorder: None }
    }

    pub fn init(&mut self, user_id: Option<String>) {
        self.recorder = Some(Arc::new(TelemetryRecorder::new(user_id)));
    }

    pub fn get(&self) -> Option<Arc<TelemetryRecorder>> {
        self.recorder.clone()
    }

    pub fn is_initialized(&self) -> bool {
        self.recorder.is_some()
    }
}

impl Default for GlobalTelemetry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_telemetry_recorder() {
        let recorder = TelemetryRecorder::new(Some("test_user".to_string()));
        
        assert_eq!(recorder.user_id(), "test_user");
        assert!(recorder.session_id().starts_with("sess_"));
        
        // 记录请求
        recorder.log_request("/api/test", 100, true, Some("test-model")).await;
        
        // 记录错误
        recorder.log_error("TEST_ERR", "Test error message", json!({"key": "value"})).await;
        
        // 验证事件数量
        let events = recorder.export_events().await;
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_id_generation() {
        let id1 = generate_anonymous_id();
        let id2 = generate_anonymous_id();
        
        assert!(id1.starts_with("anon_"));
        assert!(id2.starts_with("anon_"));
        // 由于时间戳不同，ID 应该不同（除非在极短时间内调用）
    }
}
