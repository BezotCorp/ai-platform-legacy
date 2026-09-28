use tokio::sync::oneshot;

pub(crate) struct PendingApproval {
    pub expected_sha256: Option<String>,
    pub response: oneshot::Sender<bool>,
}
