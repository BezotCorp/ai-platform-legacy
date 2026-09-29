use serde::Serialize;

use crate::conversation::ConversationEntry;

#[derive(Debug, Serialize)]
pub(crate) struct ArchivedMessage {
    pub sequence: i64,
    pub entry: ConversationEntry,
    pub archived_at: i64,
}
