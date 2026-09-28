use serde_json::Value;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::event::Event;

pub(crate) struct ApprovalRequest<'a> {
    pub request_id: &'a str,
    pub agent_id: &'a str,
    pub call_id: &'a str,
    pub tool: &'a str,
    pub arguments: &'a Value,
    pub preview_sha256: Option<&'a str>,
    pub outbound: &'a mpsc::Sender<Event>,
    pub cancel: &'a CancellationToken,
}
