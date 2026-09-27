use serde::Serialize;

use crate::sessions::Message;

#[derive(Debug, Serialize)]
pub(crate) struct ArchivedMessage {
    pub sequence: i64,
    pub message: Message,
    pub archived_at: i64,
}
