use serde::Serialize;

use crate::{agents::ExecutionMode, sessions::History};

#[derive(Debug, Serialize)]
pub(crate) struct BoundSession {
    pub history: History,
    pub configuration_id: String,
    pub configuration_revision: i64,
    pub mode: ExecutionMode,
}
