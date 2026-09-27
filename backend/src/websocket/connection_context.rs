use std::{path::PathBuf, sync::Arc};
use tokio::sync::{Mutex, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::{
    agents::MemoryStore, configurations::ConfigurationStore, providers::Client,
    sessions::SessionStore, tools::ToolApprovalGate,
};

pub(crate) struct ConnectionContext {
    pub(crate) client: Client,
    pub(crate) gpu: Arc<Semaphore>,
    pub(crate) project_root: Arc<PathBuf>,
    pub(crate) writes: Arc<Mutex<()>>,
    pub(crate) approve_reads: bool,
    pub(crate) memory: Option<MemoryStore>,
    pub(crate) sessions: Option<SessionStore>,
    pub(crate) configurations: Option<ConfigurationStore>,
    pub(crate) approvals: ToolApprovalGate,
    pub(crate) shutdown: CancellationToken,
}
