use serde::Serialize;

use crate::{
    agents::context::ContextRole,
    conversation::{ConversationAuthor, ConversationEntry},
};

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ContextMessage {
    pub(crate) role: ContextRole,
    pub(crate) content: String,
}

impl ContextMessage {
    pub(crate) fn system(content: String) -> Self {
        Self {
            role: ContextRole::System,
            content,
        }
    }

    pub(crate) fn user(content: String) -> Self {
        Self {
            role: ContextRole::User,
            content,
        }
    }

    pub(crate) fn from_conversation(entry: &ConversationEntry) -> Self {
        Self {
            role: match entry.author {
                ConversationAuthor::Human => ContextRole::User,
                ConversationAuthor::Ai => ContextRole::Assistant,
            },
            content: entry.content.clone(),
        }
    }
}
