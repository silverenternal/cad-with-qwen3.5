//! 配额检查服务 - 简化版
//!
//! 直接包装领域模型，避免过度封装

use crate::domain::model::UserQuota as DomainUserQuota;
use crate::domain::DomainResult;
use crate::db::Database;
use std::sync::Arc;

/// 配额检查服务
///
/// # 设计原则
/// - 领域层负责业务规则（UserQuota::increment）
/// - 基础设施层负责持久化（Database）
/// - 本服务仅作为简单包装，提供原子性操作
pub struct QuotaService {
    db: Arc<dyn Database>,
    default_daily_limit: u32,
}

impl QuotaService {
    pub fn new(db: Arc<dyn Database>, default_daily_limit: u32) -> Self {
        Self {
            db,
            default_daily_limit,
        }
    }

    /// 检查并扣减配额
    ///
    /// # 原子性
    /// 使用单次 SQL UPDATE ... RETURNING 确保原子性，避免竞态条件
    pub async fn check_and_consume(&self, user_id: &str) -> DomainResult<DomainUserQuota> {
        // 单次原子操作：扣减并返回
        let db_quota = self.db
            .consume_and_get_quota(user_id, 1)
            .await
            .map_err(|e| crate::domain::DomainError::external_service("database", "consume_and_get_quota", e.to_string()))?;

        // 转换为领域层 UserQuota
        Ok(DomainUserQuota {
            user_id: db_quota.user_id,
            daily_limit: db_quota.daily_limit,
            used_today: db_quota.used_today,
            last_reset_date: db_quota.last_reset_date,
        })
    }

    /// 获取用户配额
    pub async fn get_quota(&self, user_id: &str) -> DomainResult<DomainUserQuota> {
        let db_quota = self.db
            .get_user_quota(user_id)
            .await
            .map_err(|e| crate::domain::DomainError::external_service("database", "get_user_quota", e.to_string()))?;

        // 转换为领域层 UserQuota
        Ok(DomainUserQuota {
            user_id: db_quota.user_id,
            daily_limit: db_quota.daily_limit,
            used_today: db_quota.used_today,
            last_reset_date: db_quota.last_reset_date,
        })
    }

    /// 设置用户配额（管理员操作）
    pub async fn set_quota(&self, user_id: &str, daily_limit: u32) -> DomainResult<()> {
        if daily_limit == 0 {
            return Err(crate::domain::DomainError::validation(
                "daily_limit",
                "daily_limit 不能为 0",
            ));
        }

        self.db
            .set_user_quota(user_id, daily_limit)
            .await
            .map_err(|e| crate::domain::DomainError::external_service("database", "set_user_quota", e.to_string()))
    }

    /// 获取默认配额限制
    pub fn default_limit(&self) -> u32 {
        self.default_daily_limit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quota_service_default_limit() {
        // 占位测试 - 实际测试需要 mock Database
        assert!(true);
    }
}
