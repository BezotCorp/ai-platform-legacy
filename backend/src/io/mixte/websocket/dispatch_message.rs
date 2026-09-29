use serde_json::Value;
use tokio::sync::oneshot;

pub(crate) enum DispatchMessage {
    Execute(Value),
    Drain(oneshot::Sender<()>),
}
