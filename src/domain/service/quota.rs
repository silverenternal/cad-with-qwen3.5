//! 配额服务 trait

use crate::domain::model::UserQuota;
use crate::domain::DomainResult;

/// 配额服务 trait - 定义配额管理的核心业务逻辑
#[async_trait::async_trait]
pub trait QuotaService: Send + Sync {
    /// 获取用户配额
    async fn get_quota(&self, user_id: &str) -> DomainResult<UserQuota>;
    
    /// 检查并增加配额使用
    /// 
    /// 这个方法应该原子性地完成：
    /// 1. 检查配额是否充足
    /// 2. 如果充足，增加使用计数
    /// 3. 返回检查结果
    async fn check_and_increment(&self, user_id: &str) -> DomainResult<()>;
    
    /// 设置用户配额
    async fn set_quota(&self, user_id: &str, daily_limit: u32) -> DomainResult<()>;
    
    /// 重置用户配额（用于测试或管理）
    async fn reset_quota(&self, user_id: &str) -> DomainResult<()>;
}
