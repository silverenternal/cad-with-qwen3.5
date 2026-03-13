//! 用户和配额领域模型

use chrono::{DateTime, Utc};

/// 用户实体
#[derive(Debug, Clone)]
pub struct User {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub is_active: bool,
}

impl User {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            created_at: Utc::now(),
            is_active: true,
        }
    }
    
    pub fn deactivate(&mut self) {
        self.is_active = false;
    }
    
    pub fn activate(&mut self) {
        self.is_active = true;
    }
}

/// 用户配额实体
#[derive(Debug, Clone)]
pub struct UserQuota {
    pub user_id: String,
    pub daily_limit: u32,
    pub used_today: u32,
    pub last_reset_date: String,
}

impl UserQuota {
    pub fn new(user_id: impl Into<String>, daily_limit: u32) -> Self {
        Self {
            user_id: user_id.into(),
            daily_limit,
            used_today: 0,
            last_reset_date: Utc::now().format("%Y-%m-%d").to_string(),
        }
    }
    
    /// 检查是否需要重置今日计数
    pub fn check_and_reset_if_needed(&mut self) {
        let today = Utc::now().format("%Y-%m-%d").to_string();
        if self.last_reset_date != today {
            self.used_today = 0;
            self.last_reset_date = today;
        }
    }
    
    /// 剩余可用次数
    pub fn remaining(&self) -> u32 {
        self.daily_limit.saturating_sub(self.used_today)
    }
    
    /// 是否已超出配额
    pub fn is_exceeded(&self) -> bool {
        self.used_today >= self.daily_limit
    }
    
    /// 增加使用计数
    /// 
    /// # Returns
    /// - `Ok(())` 如果增加成功
    /// - `Err(DomainError::QuotaExceeded)` 如果已超出配额
    pub fn increment(&mut self) -> Result<(), crate::domain::DomainError> {
        self.check_and_reset_if_needed();
        
        if self.is_exceeded() {
            return Err(crate::domain::DomainError::QuotaExceeded {
                current: self.used_today,
                limit: self.daily_limit,
            });
        }
        
        self.used_today += 1;
        Ok(())
    }
}

/// 配额降级策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaPolicy {
    /// 拒绝服务
    Reject,
    /// 使用内存模式（数据不持久化）
    MemoryMode,
}

impl Default for QuotaPolicy {
    fn default() -> Self {
        Self::MemoryMode
    }
}

impl std::str::FromStr for QuotaPolicy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "reject" => Ok(Self::Reject),
            "memory" | "memory_mode" => Ok(Self::MemoryMode),
            _ => Err(format!("Invalid quota policy: {}", s)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_quota_new() {
        let quota = UserQuota::new("user123", 100);
        assert_eq!(quota.user_id, "user123");
        assert_eq!(quota.daily_limit, 100);
        assert_eq!(quota.used_today, 0);
        assert_eq!(quota.remaining(), 100);
    }

    #[test]
    fn test_user_quota_increment_success() {
        let mut quota = UserQuota::new("user123", 100);
        
        // 第一次增加应该成功
        assert!(quota.increment().is_ok());
        assert_eq!(quota.used_today, 1);
        assert_eq!(quota.remaining(), 99);
        
        // 增加到 100 次
        for _ in 1..100 {
            quota.increment().unwrap();
        }
        assert_eq!(quota.used_today, 100);
        assert_eq!(quota.remaining(), 0);
    }

    #[test]
    fn test_user_quota_increment_exceeded() {
        let mut quota = UserQuota::new("user123", 5);
        
        // 增加 5 次
        for _ in 0..5 {
            assert!(quota.increment().is_ok());
        }
        
        // 第 6 次应该失败
        let result = quota.increment();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), crate::domain::DomainError::QuotaExceeded { .. }));
    }

    #[test]
    fn test_user_quota_check_and_reset() {
        let mut quota = UserQuota {
            user_id: "user123".to_string(),
            daily_limit: 100,
            used_today: 50,
            last_reset_date: "2020-01-01".to_string(), // 过去的日期
        };
        
        quota.check_and_reset_if_needed();
        assert_eq!(quota.used_today, 0);
        assert_eq!(quota.last_reset_date, Utc::now().format("%Y-%m-%d").to_string());
    }

    #[test]
    fn test_user_quota_is_exceeded() {
        let mut quota = UserQuota::new("user123", 100);
        assert!(!quota.is_exceeded());
        
        quota.used_today = 99;
        assert!(!quota.is_exceeded());
        
        quota.used_today = 100;
        assert!(quota.is_exceeded());
        
        quota.used_today = 150;
        assert!(quota.is_exceeded());
    }

    #[test]
    fn test_user_quota_remaining() {
        let mut quota = UserQuota::new("user123", 100);
        assert_eq!(quota.remaining(), 100);
        
        quota.used_today = 30;
        assert_eq!(quota.remaining(), 70);
        
        quota.used_today = 100;
        assert_eq!(quota.remaining(), 0);
        
        quota.used_today = 150;
        assert_eq!(quota.remaining(), 0); // saturating_sub
    }

    #[test]
    fn test_user_new_and_activate() {
        let mut user = User::new("user123");
        assert_eq!(user.id, "user123");
        assert!(user.is_active);
        
        user.deactivate();
        assert!(!user.is_active);
        
        user.activate();
        assert!(user.is_active);
    }

    #[test]
    fn test_quota_policy_from_str() {
        assert_eq!("reject".parse::<QuotaPolicy>().unwrap(), QuotaPolicy::Reject);
        assert_eq!("memory".parse::<QuotaPolicy>().unwrap(), QuotaPolicy::MemoryMode);
        assert_eq!("memory_mode".parse::<QuotaPolicy>().unwrap(), QuotaPolicy::MemoryMode);
        assert_eq!("REJECT".parse::<QuotaPolicy>().unwrap(), QuotaPolicy::Reject);
        
        assert!("invalid".parse::<QuotaPolicy>().is_err());
        assert!("".parse::<QuotaPolicy>().is_err());
    }

    #[test]
    fn test_quota_policy_default() {
        assert_eq!(QuotaPolicy::default(), QuotaPolicy::MemoryMode);
    }

    #[test]
    fn test_user_quota_increment_boundary_zero_limit() {
        // 边界测试：配额为 0 时，第一次增加就应该失败
        let mut quota = UserQuota::new("user123", 0);
        let result = quota.increment();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), crate::domain::DomainError::QuotaExceeded { .. }));
    }

    #[test]
    fn test_user_quota_increment_boundary_exact_limit() {
        // 边界测试：正好达到配额限制
        let mut quota = UserQuota::new("user123", 1);
        
        // 第一次应该成功
        assert!(quota.increment().is_ok());
        assert_eq!(quota.used_today, 1);
        
        // 第二次应该失败
        assert!(quota.increment().is_err());
    }

    #[test]
    fn test_user_quota_increment_large_limit() {
        // 边界测试：大配额限制
        let mut quota = UserQuota::new("user123", 10000);
        
        // 增加 9999 次应该都成功
        for i in 0..9999 {
            assert!(quota.increment().is_ok(), "Failed at iteration {}", i);
        }
        assert_eq!(quota.used_today, 9999);
        assert_eq!(quota.remaining(), 1);
        
        // 第 10000 次应该成功
        assert!(quota.increment().is_ok());
        assert_eq!(quota.used_today, 10000);
        assert_eq!(quota.remaining(), 0);
        
        // 第 10001 次应该失败
        assert!(quota.increment().is_err());
    }

    #[test]
    fn test_user_quota_concurrent_increment_simulation() {
        // 模拟并发场景：多次连续增加
        let mut quota = UserQuota::new("user123", 100);
        let mut success_count = 0;
        
        // 模拟 150 次并发请求
        for _ in 0..150 {
            if quota.increment().is_ok() {
                success_count += 1;
            }
        }
        
        // 应该正好有 100 次成功
        assert_eq!(success_count, 100);
        assert_eq!(quota.used_today, 100);
        assert!(quota.is_exceeded());
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_user_quota_increment_proptest(daily_limit in 1u32..1000) {
            let mut quota = UserQuota::new("user123", daily_limit);
            
            // 尝试增加 daily_limit + 10 次
            let mut success_count = 0;
            for _ in 0..(daily_limit + 10) {
                if quota.increment().is_ok() {
                    success_count += 1;
                }
            }
            
            // 成功次数应该正好等于 daily_limit
            prop_assert_eq!(success_count, daily_limit);
            prop_assert_eq!(quota.used_today, daily_limit);
            prop_assert!(quota.is_exceeded());
        }

        #[test]
        fn test_user_quota_remaining_proptest(
            daily_limit in 1u32..1000,
            used in 0u32..2000
        ) {
            let mut quota = UserQuota::new("user123", daily_limit);
            quota.used_today = used;
            
            let expected_remaining = daily_limit.saturating_sub(used);
            prop_assert_eq!(quota.remaining(), expected_remaining);
            
            // is_exceeded 应该与 remaining == 0 一致
            prop_assert_eq!(quota.is_exceeded(), expected_remaining == 0);
        }

        #[test]
        fn test_user_quota_date_reset_proptest(
            daily_limit in 1u32..1000,
            used_today in 0u32..2000,
            days_ago in 0u32..365
        ) {
            let past_date = (Utc::now() - chrono::Duration::days(days_ago as i64))
                .format("%Y-%m-%d")
                .to_string();
            
            let mut quota = UserQuota {
                user_id: "user123".to_string(),
                daily_limit,
                used_today,
                last_reset_date: past_date,
            };
            
            // 调用 check_and_reset_if_needed 应该重置计数
            quota.check_and_reset_if_needed();
            
            // 只要 days_ago > 0，计数应该重置为 0
            if days_ago > 0 {
                prop_assert_eq!(quota.used_today, 0);
            } else {
                // days_ago == 0，计数不变
                prop_assert_eq!(quota.used_today, used_today);
            }
        }
    }
}
