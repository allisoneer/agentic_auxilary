//! Chat state types

use serde::{Deserialize, Serialize};

/// A chat message
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// Unique message ID
    pub id: String,
    /// Role: "user" or "assistant"
    pub role: String,
    /// Message content
    pub content: String,
    /// Unix timestamp of creation
    pub created_at: i64,
}

/// A conversation
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Conversation {
    /// Unique conversation ID
    pub id: String,
    /// Conversation title
    pub title: String,
    /// Model used for this conversation
    pub model: String,
    /// Unix timestamp of creation
    pub created_at: i64,
    /// Unix timestamp of last update
    pub updated_at: i64,
}

/// Current chat state
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ChatState {
    /// Messages in the current conversation
    pub messages: Vec<Message>,
    /// Whether a response is currently streaming
    pub is_streaming: bool,
    /// Current model
    pub model: String,
    /// Current conversation ID
    pub conversation_id: Option<String>,
}

/// Mutations that can be applied to chat state
#[derive(Debug, Clone)]
pub enum ChatStateMutation {
    /// Set streaming status
    SetIsStreaming(bool),
    /// Set the model
    SetModel(String),
    /// Set the conversation ID
    SetConversationId(Option<String>),
    /// Push a new message
    PushMessage(Message),
    /// Update the last message
    UpdateLastMessage(Message),
    /// Set all messages
    SetMessages(Vec<Message>),
    /// Clear all messages
    ClearMessages,
}

impl ChatStateMutation {
    /// Apply this mutation to the given state
    pub fn apply(self, state: &mut ChatState) {
        match self {
            Self::SetIsStreaming(b) => state.is_streaming = b,
            Self::SetModel(m) => state.model = m,
            Self::SetConversationId(id) => state.conversation_id = id,
            Self::PushMessage(m) => state.messages.push(m),
            Self::UpdateLastMessage(m) => {
                if let Some(last) = state.messages.last_mut() {
                    *last = m;
                }
            }
            Self::SetMessages(msgs) => state.messages = msgs,
            Self::ClearMessages => state.messages.clear(),
        }
    }
}
