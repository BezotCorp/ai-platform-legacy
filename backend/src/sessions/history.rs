use serde::Serialize;

use crate::{
    conversation::ConversationEntry,
    sessions::Session,
};

#[derive(Clone, Debug, Serialize)]
pub(crate) struct History {
    pub(crate) session: Session,
    pub(crate) entries: Vec<ConversationEntry>,
}
