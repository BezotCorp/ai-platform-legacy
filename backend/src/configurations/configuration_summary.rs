use rusqlite::{Result, Row};
use serde::Serialize;

use crate::agents::ExecutionModeSummary;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ConfigurationSummary {
    pub id: String,
    pub title: String,
    pub description: String,
    pub revision: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub mode: ExecutionModeSummary,
}

impl ConfigurationSummary {
    pub(crate) fn from_row(row: &Row<'_>, mode: ExecutionModeSummary) -> Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            title: row.get(1)?,
            description: row.get(2)?,
            revision: row.get(3)?,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
            mode,
        })
    }
}
