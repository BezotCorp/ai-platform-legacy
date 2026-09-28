use serde::Serialize;

use crate::agents::{ExecutionMode, ExecutionModeSummary};

#[derive(Debug, Serialize)]
pub(crate) struct SavedConfiguration {
    pub id: String,
    pub title: String,
    pub description: String,
    pub revision: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub summary: ExecutionModeSummary,
    pub mode: ExecutionMode,
}
