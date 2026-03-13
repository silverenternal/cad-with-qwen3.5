//! 命令对象模块 - 请求 DTO

/// 图纸分析命令
#[derive(Debug, Clone)]
pub struct AnalyzeDrawingCommand {
    pub user_id: String,
    pub drawing_type: String,
    pub image_data: Vec<u8>,
    pub question: Option<String>,
}

impl AnalyzeDrawingCommand {
    pub fn new(
        user_id: impl Into<String>,
        drawing_type: impl Into<String>,
        image_data: Vec<u8>,
        question: Option<String>,
    ) -> Self {
        Self {
            user_id: user_id.into(),
            drawing_type: drawing_type.into(),
            image_data,
            question,
        }
    }
}

/// API Key 生成命令
#[derive(Debug, Clone)]
pub struct GenerateApiKeyCommand {
    pub description: Option<String>,
    pub expires_in_days: Option<u32>,
}

impl GenerateApiKeyCommand {
    pub fn new(description: Option<String>, expires_in_days: Option<u32>) -> Self {
        Self {
            description,
            expires_in_days,
        }
    }
}

/// API Key 轮换命令
#[derive(Debug, Clone)]
pub struct RotateApiKeyCommand {
    pub old_key: String,
    pub revoke_old: bool,
    pub expires_in_days: Option<u32>,
}

impl RotateApiKeyCommand {
    pub fn new(
        old_key: impl Into<String>,
        revoke_old: bool,
        expires_in_days: Option<u32>,
    ) -> Self {
        Self {
            old_key: old_key.into(),
            revoke_old,
            expires_in_days,
        }
    }
}

/// 配额设置命令
#[derive(Debug, Clone)]
pub struct SetQuotaCommand {
    pub user_id: String,
    pub daily_limit: u32,
}

impl SetQuotaCommand {
    pub fn new(user_id: impl Into<String>, daily_limit: u32) -> Self {
        Self {
            user_id: user_id.into(),
            daily_limit,
        }
    }
}
