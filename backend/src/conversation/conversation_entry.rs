use serde::{Deserialize, Serialize};

use crate::conversation::ConversationAuthor;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConversationEntry {
    #[serde(alias = "role")]
    pub(crate) author: ConversationAuthor,
    pub(crate) content: String,
}

impl ConversationEntry {
    pub(crate) fn human(content: String) -> Self {
        Self {
            author: ConversationAuthor::Human,
            content,
        }
    }

    pub(crate) fn ai(content: String) -> Self {
        Self {
            author: ConversationAuthor::Ai,
            content,
        }
    }
}
