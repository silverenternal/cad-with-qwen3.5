//! 对话管理器模块 - 简化版

use crate::infrastructure::external::{ChatRequest, Message};

/// 截断信息结构
pub struct TruncateInfo {
    pub removed_messages: usize,
}

/// 对话管理器
pub struct DialogManager {
    messages: Vec<Message>,
    max_tokens: usize,
    max_rounds: usize,
    current_token_count: usize,
    model: String,
}

impl DialogManager {
    /// 创建对话管理器
    pub fn new(model: &str, max_tokens: usize, max_rounds: usize) -> Self {
        Self {
            messages: Vec::new(),
            max_tokens,
            max_rounds,
            current_token_count: 0,
            model: model.to_string(),
        }
    }

    /// 添加系统消息
    pub fn add_system(&mut self, content: String) {
        self.messages.insert(0, Message {
            role: "system".to_string(),
            content,
            images: None,
        });
    }

    /// 获取系统提示词
    pub fn system_prompt(&self) -> &str {
        self.messages.first()
            .filter(|m| m.role == "system")
            .map(|m| m.content.as_str())
            .unwrap_or("")
    }

    /// 添加用户消息（不带图片）
    pub fn add_user(&mut self, content: String) -> Option<TruncateInfo> {
        self.add_user_with_images(content, Vec::new())
    }

    /// 添加用户消息（带图片）
    pub fn add_user_with_images(&mut self, content: String, images: Vec<String>) -> Option<TruncateInfo> {
        let token_estimate = estimate_tokens(&content) + estimate_image_tokens(images.len());
        
        let msg = if images.is_empty() {
            Message::user(content)
        } else {
            Message::user_with_images(content, images)
        };
        
        self.messages.push(msg);
        self.current_token_count += token_estimate;
        self.truncate()
    }

    /// 添加 AI 响应
    pub fn add_assistant(&mut self, content: String) -> Option<TruncateInfo> {
        let token_estimate = estimate_tokens(&content);
        self.messages.push(Message::assistant(content));
        self.current_token_count += token_estimate;
        self.truncate()
    }

    /// 清空对话
    pub fn clear(&mut self) {
        self.messages.clear();
        self.current_token_count = 0;
    }

    /// 获取统计信息
    pub fn stats(&self) -> DialogStats {
        DialogStats {
            round_count: self.messages.len() / 2,
            token_count: self.current_token_count,
            max_tokens: self.max_tokens,
        }
    }

    /// 获取消息数量
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// 构建 API 聊天请求
    pub fn build_request(&self) -> ChatRequest {
        ChatRequest::new(self.model.clone(), self.messages.clone())
    }

    /// 获取对话历史（用于导出）
    pub fn get_history(&self) -> &[Message] {
        &self.messages
    }

    /// 截断对话历史
    fn truncate(&mut self) -> Option<TruncateInfo> {
        let mut removed_messages = 0;
        // 使用 70% 作为截断阈值（留足 buffer，避免近似值导致超限）
        let target_tokens = (self.max_tokens as f64 * 0.7) as usize;

        while self.current_token_count > target_tokens && !self.messages.is_empty() {
            if let Some(msg) = self.messages.first() {
                self.current_token_count -= estimate_tokens(&msg.content);
            }
            self.messages.remove(0);
            removed_messages += 1;
        }

        // 轮次限制
        let max_messages = self.max_rounds * 2;
        if self.messages.len() > max_messages {
            let remove_count = self.messages.len() - max_messages;
            for _ in 0..remove_count {
                if let Some(msg) = self.messages.first() {
                    self.current_token_count -= estimate_tokens(&msg.content);
                }
                self.messages.remove(0);
                removed_messages += 1;
            }
        }

        if removed_messages > 0 {
            Some(TruncateInfo { removed_messages })
        } else {
            None
        }
    }
}

/// 对话统计信息
pub struct DialogStats {
    pub round_count: usize,
    pub token_count: usize,
    pub max_tokens: usize,
}

impl std::fmt::Display for DialogStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} 轮对话，{}/{} tokens", self.round_count, self.token_count, self.max_tokens)
    }
}

/// 估算文本的 token 数量（粗略估算）
///
/// # 注意
/// 这是基于字符统计的经验公式，仅供参考：
/// - 英文/数字：约 3.5 字符/token
/// - 中文：约 1.3 字符/token
///
/// # Buffer
/// 实际 token 数量以模型 API 返回为准
/// 建议在截断时使用 70% 阈值预留 buffer
pub fn estimate_tokens(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    let ascii_chars = text.chars().filter(|c| c.is_ascii()).count();
    let non_ascii_chars = text.chars().filter(|c| !c.is_ascii()).count();
    ((ascii_chars as f64 / 3.5).ceil() as usize) + ((non_ascii_chars as f64 / 1.3).ceil() as usize)
}

/// 计算图片的 token 占用（粗略估算）
/// 每张图约 500-1000 tokens，取中间值 750
pub fn estimate_image_tokens(image_count: usize) -> usize {
    image_count * 750
}
