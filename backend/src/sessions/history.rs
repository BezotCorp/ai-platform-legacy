use anyhow::{Result, bail};
use serde::Serialize;

use crate::sessions::{Message, Session};

#[derive(Clone, Debug, Serialize)]
pub(crate) struct History {
    pub(crate) session: Session,
    pub(crate) messages: Vec<Message>,
}

impl History {
    pub(crate) const MAX_MESSAGES: usize = 32;
    pub(crate) const MAX_MESSAGE_BYTES: usize = 32_768;
    pub(crate) const MAX_HISTORY_BYTES: usize = 131_072;

    pub(crate) fn validate(messages: &[Message]) -> Result<()> {
        if messages.len() > Self::MAX_MESSAGES {
            bail!("Historique trop long");
        }
        let mut bytes = 0usize;
        for message in messages {
            if !matches!(message.role.as_str(), "user" | "assistant") {
                bail!("Rôle de message non autorisé");
            }
            if message.content.trim().is_empty() || message.content.len() > Self::MAX_MESSAGE_BYTES
            {
                bail!("Contenu de message invalide");
            }
            bytes = bytes
                .checked_add(message.content.len())
                .ok_or_else(|| anyhow::anyhow!("Conversation trop volumineuse"))?;
        }
        if bytes > Self::MAX_HISTORY_BYTES {
            bail!("Conversation trop volumineuse");
        }
        Ok(())
    }
}
