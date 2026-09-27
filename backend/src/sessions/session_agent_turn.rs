use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct SessionAgentTurn {
    pub sequence: i64,
    pub agent_id: String,
    pub layer: i64,
    pub answer: String,
    pub created_at: i64,
}
