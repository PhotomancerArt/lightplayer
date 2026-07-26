//! [`AgentTranscript`]: the in-memory conversation state (v1: no
//! persistence).

use crate::provider::model_provider::{ChatMessage, ChatRole, ContentBlock, TokenUsage};

/// Messages plus running usage totals for one agent session.
#[derive(Clone, Debug, Default)]
pub struct AgentTranscript {
    pub messages: Vec<ChatMessage>,
    pub usage_total: TokenUsage,
}

impl AgentTranscript {
    /// Append a plain user text message.
    pub fn push_user_text(&mut self, text: impl Into<String>) {
        self.messages.push(ChatMessage::user_text(text));
    }

    /// Append an assistant message (skipped when `content` is empty).
    pub fn push_assistant(&mut self, content: Vec<ContentBlock>) {
        if !content.is_empty() {
            self.messages.push(ChatMessage {
                role: ChatRole::Assistant,
                content,
            });
        }
    }

    /// Append a user message carrying tool results.
    pub fn push_tool_results(&mut self, results: Vec<ContentBlock>) {
        if !results.is_empty() {
            self.messages.push(ChatMessage {
                role: ChatRole::User,
                content: results,
            });
        }
    }
}
