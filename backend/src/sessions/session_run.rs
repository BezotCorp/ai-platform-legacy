use rusqlite::{Result, Row};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct SessionRun {
    pub request_id: String,
    pub status: String,
    pub prompt: String,
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl SessionRun {
    pub(crate) fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            request_id: row.get(0)?,
            status: row.get(1)?,
            prompt: row.get(2)?,
            error: row.get(3)?,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
        })
    }
}
