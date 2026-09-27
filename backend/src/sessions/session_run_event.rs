use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize)]
pub(crate) struct SessionRunEvent {
    pub sequence: i64,
    pub kind: String,
    pub data: Value,
    pub created_at: i64,
}
