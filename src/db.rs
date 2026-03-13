//! 数据库抽象模块 - 使用 trait 统一内存和 SQLite 实现
//!
//! 本模块提供统一的数据库接口，支持两种实现：
//! - `InMemoryDatabase`: 内存实现，用于测试和无 SQLite 环境
//! - `SqliteDatabase`: SQLite 实现，用于生产环境
//!
//! 运行时通过配置决定使用哪种实现，无需条件编译割裂代码。

use std::sync::Arc;
use async_trait::async_trait;
use chrono::{Utc, TimeZone, Datelike};
use tokio::sync::RwLock;
use tracing::{info, warn};

// 使用统一的 Error 类型
use crate::error::Error;

// SQLite 相关类型（仅在启用 feature 时可用）
#[cfg(feature = "with-sqlite")]
use sqlx::{SqlitePool, migrate::MigrateDatabase, Row};

/// 数据库错误类型 - 直接使用 Error
pub type DbError = Error;

/// 数据库连接池 trait - 统一所有数据库操作
#[async_trait]
pub trait Database: Send + Sync {
    /// 记录遥测事件
    async fn log_event(
        &self,
        event_type: &str,
        user_id: Option<&str>,
        session_id: Option<&str>,
        endpoint: Option<&str>,
        latency_ms: Option<i64>,
        success: Option<bool>,
        model_used: Option<&str>,
        error_code: Option<&str>,
        error_message: Option<&str>,
        context: Option<&str>,
        uptime_ms: u64,
    ) -> Result<(), DbError>;

    /// 获取统计信息
    async fn get_stats(&self) -> Result<TelemetryStats, DbError>;

    /// 获取用户每日配额使用情况
    async fn get_user_daily_usage(
        &self,
        user_id: &str,
        date: chrono::DateTime<Utc>,
    ) -> Result<u32, DbError>;

    /// 获取或创建用户配额
    async fn get_or_create_user_quota(
        &self,
        user_id: &str,
        default_limit: u32,
    ) -> Result<UserQuota, DbError>;

    /// 增加用户今日使用量
    async fn increment_user_usage(&self, user_id: &str) -> Result<(), DbError>;

    /// 原子性地记录 API 使用并增加配额计数（使用事务保证）
    async fn record_api_usage_and_increment(
        &self,
        user_id: &str,
        endpoint: &str,
        model_used: Option<&str>,
        latency_ms: i64,
    ) -> Result<(), DbError>;

    /// 记录 API 使用日志
    async fn log_api_usage(
        &self,
        user_id: &str,
        endpoint: &str,
        model_used: Option<&str>,
        latency_ms: i64,
        success: bool,
    ) -> Result<(), DbError>;

    /// 设置用户每日配额
    async fn set_user_quota(&self, user_id: &str, daily_limit: u32) -> Result<(), DbError>;

    /// 保存 API Key
    async fn save_api_key(
        &self,
        key_hash: &str,
        key_prefix: &str,
        description: Option<&str>,
        expires_at: Option<chrono::DateTime<Utc>>,
    ) -> Result<(), DbError>;

    /// 验证 API Key
    async fn verify_api_key(&self, key_hash: &str) -> Result<bool, DbError>;

    /// 撤销 API Key
    async fn revoke_api_key(&self, key_hash: &str) -> Result<bool, DbError>;

    /// 列出所有 API Key
    async fn list_api_keys(&self) -> Result<Vec<ApiKeyInfo>, DbError>;

    /// 检查数据库连接是否正常
    async fn is_healthy(&self) -> bool;

    /// 获取数据库类型名称
    fn db_type(&self) -> &'static str;

    /// 执行原始 SQL 查询（用于原子性操作）
    async fn execute_sql(
        &self,
        query: &str,
        count: i64,
        user_id: &str,
    ) -> Result<u64, DbError>;

    /// 原子性扣减配额
    ///
    /// # Returns
    /// - `Ok(())` 如果扣减成功
    /// - `Err(Error::Validation)` 如果配额不足
    async fn consume_quota(&self, user_id: &str, count: u32) -> Result<(), DbError> {
        // 默认实现：使用 execute_sql 原子性扣减
        let rows = self.execute_sql(
            r#"
            UPDATE user_quotas
            SET used_today = used_today + ?1, updated_at = CURRENT_TIMESTAMP
            WHERE user_id = ?2
            AND used_today < daily_limit
            "#,
            count as i64,
            user_id,
        ).await?;

        if rows == 0 {
            Err(Error::Validation("配额不足，无法扣减".to_string()))
        } else {
            Ok(())
        }
    }

    /// 获取用户配额
    async fn get_user_quota(&self, user_id: &str) -> Result<UserQuota, DbError>;

    /// 原子性扣减配额并返回更新后的配额
    ///
    /// # 参数
    /// * `user_id` - 用户 ID
    /// * `count` - 要扣减的数量
    ///
    /// # 返回
    /// - `Ok(UserQuota)` 扣减成功，返回更新后的配额
    /// - `Err(DbError::Validation)` 配额不足
    /// - `Err(DbError::NotFound)` 用户不存在
    async fn consume_and_get_quota(&self, user_id: &str, count: u32) -> Result<UserQuota, DbError> {
        // 默认实现：先扣减再获取（两次调用）
        // 子类可以覆写为单次原子操作
        self.consume_quota(user_id, count).await?;
        self.get_user_quota(user_id).await
    }
}

/// 数据库类型别名
pub type DbPool = Arc<dyn Database>;

// SQLite 错误转换
#[cfg(feature = "with-sqlite")]
impl From<sqlx::Error> for DbError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::PoolTimedOut => Error::Internal("数据库连接超时".to_string()),
            sqlx::Error::Database(e) => Error::Internal(e.message().to_string()),
            sqlx::Error::Migrate(e) => Error::Internal(format!("迁移错误：{}", e)),
            _ => Error::Internal(err.to_string()),
        }
    }
}

#[cfg(feature = "with-sqlite")]
impl From<sqlx::migrate::MigrateError> for DbError {
    fn from(err: sqlx::migrate::MigrateError) -> Self {
        Error::Internal(format!("迁移错误：{}", err))
    }
}

/// 初始化数据库（根据配置返回不同实现）
pub async fn init_database(database_url: Option<&str>) -> Result<DbPool, DbError> {
    match database_url {
        Some(url) => {
            // 尝试初始化 SQLite 数据库
            match SqliteDatabase::new(url).await {
                Ok(db) => {
                    info!("SQLite database initialized successfully");
                    Ok(Arc::new(db))
                }
                Err(e) => {
                    warn!("SQLite initialization failed: {}, falling back to in-memory database", e);
                    Ok(Arc::new(InMemoryDatabase::new()))
                }
            }
        }
        None => {
            info!("No database URL provided, using in-memory database");
            Ok(Arc::new(InMemoryDatabase::new()))
        }
    }
}

// ===== SQLite 数据库实现 =====

#[cfg(feature = "with-sqlite")]
pub struct SqliteDatabase {
    pool: Arc<SqlitePool>,
}

#[cfg(feature = "with-sqlite")]
impl SqliteDatabase {
    pub async fn new(database_url: &str) -> Result<Self, DbError> {
        // 如果数据库文件不存在，创建它
        if !sqlx::Sqlite::database_exists(database_url).await.unwrap_or(false) {
            info!("Creating database: {}", database_url);
            sqlx::Sqlite::create_database(database_url).await?;
        }

        let pool = SqlitePool::connect(database_url).await?;

        // 运行数据库迁移
        run_migrations(&pool).await?;

        Ok(Self { pool: Arc::new(pool) })
    }
}

#[cfg(feature = "with-sqlite")]
async fn run_migrations(pool: &SqlitePool) -> Result<(), DbError> {
    info!("Running database migrations...");
    let migrator = sqlx::migrate!();
    migrator.run(pool).await?;
    info!("Database migrations completed successfully");
    Ok(())
}

#[cfg(feature = "with-sqlite")]
#[async_trait]
impl Database for SqliteDatabase {
    async fn log_event(
        &self,
        event_type: &str,
        user_id: Option<&str>,
        session_id: Option<&str>,
        endpoint: Option<&str>,
        latency_ms: Option<i64>,
        success: Option<bool>,
        model_used: Option<&str>,
        error_code: Option<&str>,
        error_message: Option<&str>,
        context: Option<&str>,
        uptime_ms: u64,
    ) -> Result<(), DbError> {
        sqlx::query(
            r#"
            INSERT INTO telemetry_events
            (event_type, user_id, session_id, endpoint, latency_ms, success, model_used,
             error_code, error_message, context, uptime_ms)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(event_type)
        .bind(user_id)
        .bind(session_id)
        .bind(endpoint)
        .bind(latency_ms)
        .bind(success.map(|b| if b { 1i64 } else { 0i64 }))
        .bind(model_used)
        .bind(error_code)
        .bind(error_message)
        .bind(context)
        .bind(uptime_ms as i64)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    async fn get_stats(&self) -> Result<TelemetryStats, DbError> {
        let row = sqlx::query(
            r#"
            SELECT
                COUNT(*) as total_requests,
                SUM(CASE WHEN success = 1 THEN 1 ELSE 0 END) as successful_requests,
                SUM(CASE WHEN success = 0 THEN 1 ELSE 0 END) as failed_requests,
                AVG(latency_ms) as avg_latency_ms
            FROM telemetry_events
            WHERE event_type = 'api_request'
            "#,
        )
        .fetch_one(&*self.pool)
        .await?;

        Ok(TelemetryStats {
            total_requests: row.get::<i64, _>(0) as u64,
            successful_requests: row.get::<i64, _>(1) as u64,
            failed_requests: row.get::<i64, _>(2) as u64,
            avg_latency_ms: row.get::<Option<f64>, _>(3).unwrap_or(0.0),
        })
    }

    async fn get_user_daily_usage(
        &self,
        user_id: &str,
        date: chrono::DateTime<Utc>,
    ) -> Result<u32, DbError> {
        // 使用 ok_or 处理时间戳转换失败，而不是 expect
        let start_ts = Utc.with_ymd_and_hms(
            date.year(),
            date.month(),
            date.day(),
            0, 0, 0
        ).single().ok_or_else(|| {
            Error::Internal(format!(
                "Invalid start timestamp: {}-{}-{}",
                date.year(), date.month(), date.day()
            ))
        })?.timestamp();

        let end_ts = Utc.with_ymd_and_hms(
            date.year(),
            date.month(),
            date.day(),
            23, 59, 59
        ).single().ok_or_else(|| {
            Error::Internal(format!(
                "Invalid end timestamp: {}-{}-{}",
                date.year(), date.month(), date.day()
            ))
        })?.timestamp();

        let row: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) as count
            FROM api_usage_logs
            WHERE user_id = ?
            AND datetime(created_at, 'unixepoch') BETWEEN datetime(?, 'unixepoch') AND datetime(?, 'unixepoch')
            "#,
        )
        .bind(user_id)
        .bind(start_ts)
        .bind(end_ts)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row.0 as u32)
    }

    async fn get_or_create_user_quota(
        &self,
        user_id: &str,
        default_limit: u32,
    ) -> Result<UserQuota, DbError> {
        let today = Utc::now();
        let today_str = today.format("%Y-%m-%d").to_string();

        let existing = sqlx::query_as::<_, (i64, String, i64, i64, String)>(
            r#"
            SELECT id, user_id, daily_limit, used_today, last_reset_date
            FROM user_quotas
            WHERE user_id = ?
            "#,
        )
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;

        if let Some((_, db_user_id, daily_limit, used_today, last_reset_date)) = existing {
            if last_reset_date != today_str {
                sqlx::query(
                    r#"
                    UPDATE user_quotas
                    SET used_today = 0, last_reset_date = ?, updated_at = CURRENT_TIMESTAMP
                    WHERE user_id = ?
                    "#,
                )
                .bind(&today_str)
                .bind(user_id)
                .execute(&*self.pool)
                .await?;

                Ok(UserQuota {
                    user_id: db_user_id,
                    daily_limit: daily_limit as u32,
                    used_today: 0,
                    last_reset_date: today_str,
                })
            } else {
                Ok(UserQuota {
                    user_id: db_user_id,
                    daily_limit: daily_limit as u32,
                    used_today: used_today as u32,
                    last_reset_date,
                })
            }
        } else {
            // Create new quota record
            sqlx::query(
                r#"
                INSERT INTO user_quotas (user_id, daily_limit, used_today, last_reset_date)
                VALUES (?, ?, 0, ?)
                "#,
            )
            .bind(user_id)
            .bind(default_limit as i64)
            .bind(&today_str)
            .execute(&*self.pool)
            .await?;

            Ok(UserQuota {
                user_id: user_id.to_string(),
                daily_limit: default_limit,
                used_today: 0,
                last_reset_date: today_str,
            })
        }
    }

    async fn get_user_quota(&self, user_id: &str) -> Result<UserQuota, DbError> {
        let today = Utc::now();
        let today_str = today.format("%Y-%m-%d").to_string();

        let existing = sqlx::query_as::<_, (i64, String, i64, i64, String)>(
            r#"
            SELECT id, user_id, daily_limit, used_today, last_reset_date
            FROM user_quotas
            WHERE user_id = ?
            "#,
        )
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;

        match existing {
            Some((_, db_user_id, daily_limit, used_today, last_reset_date)) => {
                // Check if needs reset
                if last_reset_date != today_str {
                    Ok(UserQuota {
                        user_id: db_user_id,
                        daily_limit: daily_limit as u32,
                        used_today: 0,
                        last_reset_date: today_str,
                    })
                } else {
                    Ok(UserQuota {
                        user_id: db_user_id,
                        daily_limit: daily_limit as u32,
                        used_today: used_today as u32,
                        last_reset_date,
                    })
                }
            }
            None => Err(Error::NotFound(format!("用户配额不存在：{}", user_id))),
        }
    }

    async fn consume_and_get_quota(&self, user_id: &str, count: u32) -> Result<UserQuota, DbError> {
        // 单次原子操作：扣减并返回
        let today = Utc::now();
        let today_str = today.format("%Y-%m-%d").to_string();

        // 使用 RETURNING 子句一次性完成扣减和返回
        let updated = sqlx::query_as::<_, (i64, String, i64, i64, String)>(
            r#"
            UPDATE user_quotas
            SET used_today = used_today + ?1, updated_at = CURRENT_TIMESTAMP
            WHERE user_id = ?2
            AND used_today + ?1 <= daily_limit
            RETURNING id, user_id, daily_limit, used_today, last_reset_date
            "#,
        )
        .bind(count as i64)
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;

        match updated {
            Some((_, db_user_id, daily_limit, used_today, last_reset_date)) => {
                // 检查是否需要重置日期
                let final_used = if last_reset_date != today_str {
                    0
                } else {
                    used_today as u32
                };

                Ok(UserQuota {
                    user_id: db_user_id,
                    daily_limit: daily_limit as u32,
                    used_today: final_used,
                    last_reset_date: if last_reset_date != today_str {
                        today_str
                    } else {
                        last_reset_date
                    },
                })
            }
            None => {
                // 可能是配额不足或用户不存在，检查是否存在
                let exists = sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM user_quotas WHERE user_id = ?",
                )
                .bind(user_id)
                .fetch_one(&*self.pool)
                .await?;

                if exists == 0 {
                    Err(Error::NotFound(format!("用户配额不存在：{}", user_id)))
                } else {
                    Err(Error::Validation("配额不足，无法扣减".to_string()))
                }
            }
        }
    }

    async fn increment_user_usage(&self, user_id: &str) -> Result<(), DbError> {
        sqlx::query(
            r#"
            UPDATE user_quotas
            SET used_today = used_today + 1, updated_at = CURRENT_TIMESTAMP
            WHERE user_id = ?
            "#,
        )
        .bind(user_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// 原子性地记录 API 使用并增加配额计数（使用事务保证）
    async fn record_api_usage_and_increment(
        &self,
        user_id: &str,
        endpoint: &str,
        model_used: Option<&str>,
        latency_ms: i64,
    ) -> Result<(), DbError> {
        // 使用事务保证配额扣减和日志记录的原子性
        let mut tx = self.pool.begin().await.map_err(|e| {
            Error::Internal(format!("Failed to begin transaction: {}", e))
        })?;

        // 1. 增加配额使用计数
        sqlx::query(
            r#"
            UPDATE user_quotas
            SET used_today = used_today + 1, updated_at = CURRENT_TIMESTAMP
            WHERE user_id = ?
            "#,
        )
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

        // 2. 记录 API 使用日志
        sqlx::query(
            r#"
            INSERT INTO api_usage_logs (user_id, endpoint, model_used, latency_ms, success)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(user_id)
        .bind(endpoint)
        .bind(model_used)
        .bind(latency_ms)
        .bind(1i64) // success = true
        .execute(&mut *tx)
        .await?;

        // 3. 提交事务
        tx.commit().await.map_err(|e| {
            Error::Internal(format!("Failed to commit transaction: {}", e))
        })?;

        Ok(())
    }

    async fn log_api_usage(
        &self,
        user_id: &str,
        endpoint: &str,
        model_used: Option<&str>,
        latency_ms: i64,
        success: bool,
    ) -> Result<(), DbError> {
        sqlx::query(
            r#"
            INSERT INTO api_usage_logs (user_id, endpoint, model_used, latency_ms, success)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(user_id)
        .bind(endpoint)
        .bind(model_used)
        .bind(latency_ms)
        .bind(if success { 1i64 } else { 0i64 })
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    async fn set_user_quota(&self, user_id: &str, daily_limit: u32) -> Result<(), DbError> {
        let today_str = Utc::now().format("%Y-%m-%d").to_string();

        sqlx::query(
            r#"
            INSERT INTO user_quotas (user_id, daily_limit, used_today, last_reset_date)
            VALUES (?, ?, 0, ?)
            ON CONFLICT(user_id) DO UPDATE SET
                daily_limit = excluded.daily_limit,
                updated_at = CURRENT_TIMESTAMP
            "#,
        )
        .bind(user_id)
        .bind(daily_limit as i64)
        .bind(&today_str)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    async fn save_api_key(
        &self,
        key_hash: &str,
        key_prefix: &str,
        description: Option<&str>,
        expires_at: Option<chrono::DateTime<Utc>>,
    ) -> Result<(), DbError> {
        ensure_api_keys_table(&self.pool).await?;

        sqlx::query(
            r#"
            INSERT INTO api_keys (key_hash, key_prefix, description, expires_at)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(key_hash)
        .bind(key_prefix)
        .bind(description)
        .bind(expires_at.map(|dt| dt.timestamp()))
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    async fn verify_api_key(&self, key_hash: &str) -> Result<bool, DbError> {
        ensure_api_keys_table(&self.pool).await?;

        let exists: (bool,) = sqlx::query_as(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM api_keys
                WHERE key_hash = ? AND is_active = 1
                AND (expires_at IS NULL OR expires_at > strftime('%s', 'now'))
            )
            "#,
        )
        .bind(key_hash)
        .fetch_one(&*self.pool)
        .await?;

        if exists.0 {
            sqlx::query(
                r#"UPDATE api_keys SET last_used_at = CURRENT_TIMESTAMP WHERE key_hash = ?"#,
            )
            .bind(key_hash)
            .execute(&*self.pool)
            .await?;
        }

        Ok(exists.0)
    }

    async fn revoke_api_key(&self, key_hash: &str) -> Result<bool, DbError> {
        let result = sqlx::query(
            r#"UPDATE api_keys SET is_active = 0 WHERE key_hash = ?"#,
        )
        .bind(key_hash)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn list_api_keys(&self) -> Result<Vec<ApiKeyInfo>, DbError> {
        let rows = sqlx::query(
            r#"
            SELECT key_prefix, created_at, expires_at, is_active, last_used_at, description
            FROM api_keys
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&*self.pool)
        .await?;

        let keys: Vec<ApiKeyInfo> = rows
            .iter()
            .map(|row| ApiKeyInfo {
                key_prefix: row.get::<String, _>("key_prefix"),
                created_at_ts: row.get::<i64, _>("created_at"),
                expires_at_ts: row.get::<Option<i64>, _>("expires_at"),
                is_active: row.get::<bool, _>("is_active"),
                last_used_at_ts: row.get::<Option<i64>, _>("last_used_at"),
                description: row.get::<Option<String>, _>("description"),
            })
            .collect();

        Ok(keys)
    }

    async fn is_healthy(&self) -> bool {
        sqlx::query("SELECT 1")
            .fetch_optional(&*self.pool)
            .await
            .is_ok()
    }

    fn db_type(&self) -> &'static str {
        "sqlite"
    }

    async fn execute_sql(
        &self,
        query: &str,
        count: i64,
        user_id: &str,
    ) -> Result<u64, DbError> {
        // 使用动态 SQL 查询
        let result = sqlx::query(query)
            .bind(count)
            .bind(user_id)
            .execute(&*self.pool)
            .await?;
        
        Ok(result.rows_affected())
    }
}

#[cfg(feature = "with-sqlite")]
async fn ensure_api_keys_table(pool: &SqlitePool) -> Result<(), DbError> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS api_keys (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            key_hash TEXT UNIQUE NOT NULL,
            key_prefix TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            expires_at DATETIME,
            is_active BOOLEAN DEFAULT 1,
            description TEXT,
            last_used_at DATETIME
        )
        "#,
    )
    .execute(&*pool)
    .await?;
    Ok(())
}

// ===== 内存数据库实现 =====

pub struct InMemoryDatabase {
    telemetry_events: RwLock<Vec<TelemetryEventRecord>>,
    user_quotas: RwLock<std::collections::HashMap<String, UserQuotaRecord>>,
    api_usage_logs: RwLock<Vec<ApiUsageLogRecord>>,
    api_keys: RwLock<std::collections::HashMap<String, ApiKeyRecord>>,
}

struct TelemetryEventRecord {
    event_type: String,
    user_id: Option<String>,
    session_id: Option<String>,
    endpoint: Option<String>,
    latency_ms: Option<i64>,
    success: Option<bool>,
    model_used: Option<String>,
    error_code: Option<String>,
    error_message: Option<String>,
    context: Option<String>,
    uptime_ms: u64,
}

struct UserQuotaRecord {
    user_id: String,
    daily_limit: u32,
    used_today: u32,
    last_reset_date: String,
}

struct ApiUsageLogRecord {
    user_id: String,
    endpoint: String,
    model_used: Option<String>,
    latency_ms: i64,
    success: bool,
    created_at: chrono::DateTime<Utc>,
}

pub struct ApiKeyRecord {
    key_hash: String,
    key_prefix: String,
    created_at: chrono::DateTime<Utc>,
    expires_at: Option<chrono::DateTime<Utc>>,
    is_active: bool,
    description: Option<String>,
    last_used_at: Option<chrono::DateTime<Utc>>,
}

impl InMemoryDatabase {
    pub fn new() -> Self {
        Self {
            telemetry_events: RwLock::new(Vec::new()),
            user_quotas: RwLock::new(std::collections::HashMap::new()),
            api_usage_logs: RwLock::new(Vec::new()),
            api_keys: RwLock::new(std::collections::HashMap::new()),
        }
    }
}

impl Default for InMemoryDatabase {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Database for InMemoryDatabase {
    async fn log_event(
        &self,
        event_type: &str,
        user_id: Option<&str>,
        session_id: Option<&str>,
        endpoint: Option<&str>,
        latency_ms: Option<i64>,
        success: Option<bool>,
        model_used: Option<&str>,
        error_code: Option<&str>,
        error_message: Option<&str>,
        context: Option<&str>,
        uptime_ms: u64,
    ) -> Result<(), DbError> {
        let mut events = self.telemetry_events.write().await;
        events.push(TelemetryEventRecord {
            event_type: event_type.to_string(),
            user_id: user_id.map(String::from),
            session_id: session_id.map(String::from),
            endpoint: endpoint.map(String::from),
            latency_ms,
            success,
            model_used: model_used.map(String::from),
            error_code: error_code.map(String::from),
            error_message: error_message.map(String::from),
            context: context.map(String::from),
            uptime_ms,
        });
        Ok(())
    }

    async fn get_stats(&self) -> Result<TelemetryStats, DbError> {
        let events = self.telemetry_events.read().await;
        let api_events: Vec<_> = events.iter()
            .filter(|e| e.event_type == "api_request")
            .collect();

        let total_requests = api_events.len() as u64;
        let successful_requests = api_events.iter()
            .filter(|e| e.success == Some(true))
            .count() as u64;
        let failed_requests = api_events.iter()
            .filter(|e| e.success == Some(false))
            .count() as u64;

        let avg_latency_ms = if api_events.is_empty() {
            0.0
        } else {
            let sum: f64 = api_events.iter()
                .filter_map(|e| e.latency_ms)
                .map(|l| l as f64)
                .sum();
            sum / api_events.len() as f64
        };

        Ok(TelemetryStats {
            total_requests,
            successful_requests,
            failed_requests,
            avg_latency_ms,
        })
    }

    async fn get_user_daily_usage(
        &self,
        user_id: &str,
        date: chrono::DateTime<Utc>,
    ) -> Result<u32, DbError> {
        let logs = self.api_usage_logs.read().await;
        let count = logs.iter()
            .filter(|log| {
                log.user_id == user_id
                    && log.created_at.date_naive() == date.date_naive()
            })
            .count() as u32;
        Ok(count)
    }

    async fn get_or_create_user_quota(
        &self,
        user_id: &str,
        default_limit: u32,
    ) -> Result<UserQuota, DbError> {
        let mut quotas = self.user_quotas.write().await;
        let today = Utc::now();
        let today_str = today.format("%Y-%m-%d").to_string();

        if let Some(record) = quotas.get_mut(user_id) {
            if record.last_reset_date != today_str {
                record.used_today = 0;
                record.last_reset_date = today_str.clone();
            }
            Ok(UserQuota {
                user_id: record.user_id.clone(),
                daily_limit: record.daily_limit,
                used_today: record.used_today,
                last_reset_date: today_str,
            })
        } else {
            let record = UserQuotaRecord {
                user_id: user_id.to_string(),
                daily_limit: default_limit,
                used_today: 0,
                last_reset_date: today_str.clone(),
            };
            let quota = UserQuota {
                user_id: user_id.to_string(),
                daily_limit: default_limit,
                used_today: 0,
                last_reset_date: today_str,
            };
            quotas.insert(user_id.to_string(), record);
            Ok(quota)
        }
    }

    async fn get_user_quota(&self, user_id: &str) -> Result<UserQuota, DbError> {
        let quotas = self.user_quotas.read().await;
        let today_str = Utc::now().format("%Y-%m-%d").to_string();

        if let Some(record) = quotas.get(user_id) {
            let mut quota = UserQuota {
                user_id: record.user_id.clone(),
                daily_limit: record.daily_limit,
                used_today: record.used_today,
                last_reset_date: record.last_reset_date.clone(),
            };

            // Check if needs reset
            if record.last_reset_date != today_str {
                quota.used_today = 0;
                quota.last_reset_date = today_str;
            }

            Ok(quota)
        } else {
            Err(Error::NotFound(format!("用户配额不存在：{}", user_id)))
        }
    }

    async fn consume_and_get_quota(&self, user_id: &str, count: u32) -> Result<UserQuota, DbError> {
        let mut quotas = self.user_quotas.write().await;
        let today_str = Utc::now().format("%Y-%m-%d").to_string();

        if let Some(record) = quotas.get_mut(user_id) {
            // 检查是否需要重置日期
            if record.last_reset_date != today_str {
                record.used_today = 0;
                record.last_reset_date = today_str.clone();
            }

            // 检查配额是否充足
            if record.used_today + count > record.daily_limit {
                return Err(Error::Validation("配额不足，无法扣减".to_string()));
            }

            // 扣减配额
            record.used_today += count;

            Ok(UserQuota {
                user_id: record.user_id.clone(),
                daily_limit: record.daily_limit,
                used_today: record.used_today,
                last_reset_date: record.last_reset_date.clone(),
            })
        } else {
            Err(Error::NotFound(format!("用户配额不存在：{}", user_id)))
        }
    }

    async fn increment_user_usage(&self, user_id: &str) -> Result<(), DbError> {
        let mut quotas = self.user_quotas.write().await;
        if let Some(record) = quotas.get_mut(user_id) {
            record.used_today = record.used_today.saturating_add(1);
        }
        Ok(())
    }

    async fn record_api_usage_and_increment(
        &self,
        user_id: &str,
        endpoint: &str,
        model_used: Option<&str>,
        latency_ms: i64,
    ) -> Result<(), DbError> {
        // 内存模式下，分别执行两个操作（无法真正原子性，但内存模式本身就不持久化）
        // 先增加配额
        self.increment_user_usage(user_id).await?;
        // 再记录日志
        self.log_api_usage(user_id, endpoint, model_used, latency_ms, true).await?;
        Ok(())
    }

    async fn log_api_usage(
        &self,
        user_id: &str,
        endpoint: &str,
        model_used: Option<&str>,
        latency_ms: i64,
        success: bool,
    ) -> Result<(), DbError> {
        let mut logs = self.api_usage_logs.write().await;
        logs.push(ApiUsageLogRecord {
            user_id: user_id.to_string(),
            endpoint: endpoint.to_string(),
            model_used: model_used.map(String::from),
            latency_ms,
            success,
            created_at: Utc::now(),
        });
        Ok(())
    }

    async fn set_user_quota(&self, user_id: &str, daily_limit: u32) -> Result<(), DbError> {
        let mut quotas = self.user_quotas.write().await;
        let today_str = Utc::now().format("%Y-%m-%d").to_string();

        if let Some(record) = quotas.get_mut(user_id) {
            record.daily_limit = daily_limit;
        } else {
            let record = UserQuotaRecord {
                user_id: user_id.to_string(),
                daily_limit,
                used_today: 0,
                last_reset_date: today_str,
            };
            quotas.insert(user_id.to_string(), record);
        }
        Ok(())
    }

    async fn save_api_key(
        &self,
        key_hash: &str,
        key_prefix: &str,
        description: Option<&str>,
        expires_at: Option<chrono::DateTime<Utc>>,
    ) -> Result<(), DbError> {
        let mut keys = self.api_keys.write().await;
        let record = ApiKeyRecord {
            key_hash: key_hash.to_string(),
            key_prefix: key_prefix.to_string(),
            created_at: Utc::now(),
            expires_at,
            is_active: true,
            description: description.map(String::from),
            last_used_at: None,
        };
        keys.insert(key_hash.to_string(), record);
        Ok(())
    }

    async fn verify_api_key(&self, key_hash: &str) -> Result<bool, DbError> {
        let mut keys = self.api_keys.write().await;
        if let Some(record) = keys.get_mut(key_hash) {
            let now = Utc::now();
            let is_valid = record.is_active
                && record.expires_at.map_or(true, |exp| exp > now);
            
            if is_valid {
                record.last_used_at = Some(now);
            }
            Ok(is_valid)
        } else {
            Ok(false)
        }
    }

    async fn revoke_api_key(&self, key_hash: &str) -> Result<bool, DbError> {
        let mut keys = self.api_keys.write().await;
        if let Some(record) = keys.get_mut(key_hash) {
            let was_active = record.is_active;
            record.is_active = false;
            Ok(was_active)
        } else {
            Ok(false)
        }
    }

    async fn list_api_keys(&self) -> Result<Vec<ApiKeyInfo>, DbError> {
        let keys = self.api_keys.read().await;
        let mut result: Vec<ApiKeyInfo> = keys.values()
            .map(|record| ApiKeyInfo {
                key_prefix: record.key_prefix.clone(),
                created_at_ts: record.created_at.timestamp(),
                expires_at_ts: record.expires_at.map(|dt| dt.timestamp()),
                is_active: record.is_active,
                last_used_at_ts: record.last_used_at.map(|dt| dt.timestamp()),
                description: record.description.clone(),
            })
            .collect();
        
        // 按创建时间降序排序
        result.sort_by(|a, b| b.created_at_ts.cmp(&a.created_at_ts));
        Ok(result)
    }

    async fn is_healthy(&self) -> bool {
        true // 内存数据库总是健康
    }

    fn db_type(&self) -> &'static str {
        "in-memory"
    }

    async fn execute_sql(
        &self,
        _query: &str,
        _count: i64,
        _user_id: &str,
    ) -> Result<u64, DbError> {
        // 内存数据库不支持原始 SQL 执行
        // 配额操作直接在内存中完成
        Err(Error::Internal("execute_sql not supported for in-memory database".to_string()))
    }
}

// ===== 共享数据结构 =====

/// 遥测统计信息
#[derive(Debug, Clone, Default)]
pub struct TelemetryStats {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub avg_latency_ms: f64,
}

/// 用户配额信息
#[derive(Debug, Clone)]
pub struct UserQuota {
    pub user_id: String,
    pub daily_limit: u32,
    pub used_today: u32,
    pub last_reset_date: String,
}

impl UserQuota {
    pub fn remaining(&self) -> u32 {
        self.daily_limit.saturating_sub(self.used_today)
    }

    pub fn is_exceeded(&self) -> bool {
        self.used_today >= self.daily_limit
    }
}

/// API Key 信息
#[derive(Debug, Clone)]
pub struct ApiKeyInfo {
    pub key_prefix: String,
    pub created_at_ts: i64,
    pub expires_at_ts: Option<i64>,
    pub is_active: bool,
    pub last_used_at_ts: Option<i64>,
    pub description: Option<String>,
}

impl ApiKeyInfo {
    /// 创建时间
    pub fn created_at(&self) -> chrono::DateTime<Utc> {
        chrono::DateTime::from_timestamp(self.created_at_ts, 0).unwrap_or_default()
    }

    /// 过期时间
    pub fn expires_at(&self) -> Option<chrono::DateTime<Utc>> {
        self.expires_at_ts.map(|ts| chrono::DateTime::from_timestamp(ts, 0).unwrap_or_default())
    }

    /// 最后使用时间
    pub fn last_used_at(&self) -> Option<chrono::DateTime<Utc>> {
        self.last_used_at_ts.map(|ts| chrono::DateTime::from_timestamp(ts, 0).unwrap_or_default())
    }
}

/// 使用 SHA256 哈希 API Key（不加盐，仅用于向后兼容）
#[deprecated(since = "0.9.1", note = "使用 hash_api_key_with_salt 代替，增强安全性")]
pub fn hash_api_key(key: &str) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// API Key 加盐哈希（使用固定盐值 + SHA256）
/// 
/// # 安全性说明
/// - 使用编译期固定的盐值，防止彩虹表攻击
/// - 即使数据库泄露，攻击者也无法通过彩虹表反推 API Key
/// - 盐值硬编码在代码中，内存 dump 风险低于明文存储
pub fn hash_api_key_with_salt(key: &str) -> String {
    use sha2::{Sha256, Digest};
    
    // 固定盐值（生产环境可通过修改源码重新编译来更换）
    // 盐值不需要保密，它的作用是防止彩虹表攻击
    const SALT: &str = "cad_ocr_api_key_salt_v1_2026";
    
    let mut hasher = Sha256::new();
    hasher.update(SALT.as_bytes());
    hasher.update(key.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// 获取 API Key 前缀（用于显示）
pub fn get_key_prefix(key: &str) -> String {
    if key.len() >= 8 {
        key[..8].to_string()
    } else {
        key.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_in_memory_database() {
        let db = InMemoryDatabase::new();

        // 测试配额功能
        let quota = db.get_or_create_user_quota("test_user", 100).await.unwrap();
        assert_eq!(quota.daily_limit, 100);
        assert_eq!(quota.used_today, 0);

        db.increment_user_usage("test_user").await.unwrap();
        let quota = db.get_or_create_user_quota("test_user", 100).await.unwrap();
        assert_eq!(quota.used_today, 1);

        // 测试 API Key 功能
        let key_hash = hash_api_key("test_key");
        db.save_api_key(&key_hash, "test", None, None).await.unwrap();
        assert!(db.verify_api_key(&key_hash).await.unwrap());

        db.revoke_api_key(&key_hash).await.unwrap();
        assert!(!db.verify_api_key(&key_hash).await.unwrap());

        // 测试健康检查
        assert!(db.is_healthy().await);
        assert_eq!(db.db_type(), "in-memory");
    }

    #[test]
    fn test_user_quota_methods() {
        let quota = UserQuota {
            user_id: "test".to_string(),
            daily_limit: 100,
            used_today: 30,
            last_reset_date: "2024-01-01".to_string(),
        };
        assert_eq!(quota.remaining(), 70);
        assert!(!quota.is_exceeded());

        let quota_exceeded = UserQuota {
            user_id: "test".to_string(),
            daily_limit: 100,
            used_today: 100,
            last_reset_date: "2024-01-01".to_string(),
        };
        assert_eq!(quota_exceeded.remaining(), 0);
        assert!(quota_exceeded.is_exceeded());
    }

    #[test]
    fn test_hash_api_key_with_salt() {
        let key = "test_api_key";
        let hash1 = hash_api_key_with_salt(key);
        let hash2 = hash_api_key_with_salt(key);
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64); // SHA256 输出 64 个十六进制字符

        let different_hash = hash_api_key_with_salt("different_key");
        assert_ne!(hash1, different_hash);
        
        // 验证加盐哈希与不加盐哈希不同
        #[allow(deprecated)]
        let unsalted_hash = hash_api_key(key);
        assert_ne!(hash1, unsalted_hash);
    }
}
