use std::sync::Arc;
use tokio::sync::{Mutex, RwLock, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::{
    agents::MemoryStore, configurations::ConfigurationStore, providers::Client,
    sessions::SessionStore,
};

#[derive(Clone)]
pub(crate) struct ServerState {
    pub(crate) shutdown: CancellationToken,
    pub(crate) connections: Arc<RwLock<()>>,
    pub(crate) client: Client,
    pub(crate) token: Arc<str>,
    pub(crate) origin: Arc<str>,
    pub(crate) gpu: Arc<Semaphore>,
    pub(crate) project_root: Arc<std::path::PathBuf>,
    pub(crate) writes: Arc<Mutex<()>>,
    pub(crate) approve_reads: bool,
    pub(crate) memory: Option<MemoryStore>,
    pub(crate) sessions: Option<SessionStore>,
    pub(crate) configurations: Option<ConfigurationStore>,
}
