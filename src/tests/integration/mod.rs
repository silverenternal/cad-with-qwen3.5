//! 集成测试 - 真实场景测试
//!
//! 包含：
//! - 并发配额消耗测试
//! - 路径遍历攻击测试
//! - 边界条件测试

#[cfg(test)]
mod path_traversal_tests;

#[cfg(test)]
mod quota_concurrency_tests;
