use std::{path::Path, sync::Arc};

use tokio::sync::{Mutex, mpsc};
use tokio_util::sync::CancellationToken;

use crate::{
    agents::MemoryStore,
    event::Event,
    providers::Client,
    tools::ToolApprovalGate,
};

pub(crate) struct AgentServices<'a> {
    pub client: &'a Client,
    pub request_id: &'a str,
    pub outbound: &'a mpsc::Sender<Event>,
    pub cancel: &'a CancellationToken,
    pub project_root: &'a Path,
    pub approvals: &'a ToolApprovalGate,
    pub approve_reads: bool,
    pub writes: &'a Arc<Mutex<()>>,
    pub memory: Option<&'a MemoryStore>,
}
