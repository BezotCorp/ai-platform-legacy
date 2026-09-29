mod conversation_author;
mod conversation_entry;

use anyhow::{Result, bail};

pub(crate) use conversation_author::ConversationAuthor;
pub(crate) use conversation_entry::ConversationEntry;

pub(crate) const MAX_ENTRIES: usize = 32;
pub(crate) const MAX_ENTRY_BYTES: usize = 32_768;
pub(crate) const MAX_HISTORY_BYTES: usize = 131_072;

pub(crate) fn validate(entries: &[ConversationEntry]) -> Result<()> {
    if entries.len() > MAX_ENTRIES {
        bail!("Historique trop long");
    }

    let mut bytes = 0usize;

    for entry in entries {
        if entry.content.trim().is_empty() || entry.content.len() > MAX_ENTRY_BYTES {
            bail!("Contenu de conversation invalide");
        }

        bytes = bytes
            .checked_add(entry.content.len())
            .ok_or_else(|| anyhow::anyhow!("Conversation trop volumineuse"))?;
    }

    if bytes > MAX_HISTORY_BYTES {
        bail!("Conversation trop volumineuse");
    }

    Ok(())
}
