use std::{path::Path, sync::Arc};

use tokio::sync::{Mutex, mpsc};
use tokio_util::sync::CancellationToken;

use crate::{io::Event, tools::ToolApprovalGate};

pub(crate) struct ToolCallContext<'a> {
    pub project_root: &'a Path,
    pub approvals: &'a ToolApprovalGate,
    pub approve_reads: bool,
    pub writes: &'a Arc<Mutex<()>>,
    pub request_id: &'a str,
    pub agent_id: &'a str,
    pub call_id: &'a str,
    pub outbound: &'a mpsc::Sender<Event>,
    pub cancel: &'a CancellationToken,
}
