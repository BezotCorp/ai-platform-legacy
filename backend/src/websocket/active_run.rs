use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
use tokio::{sync::mpsc, task::JoinHandle, time};
use tokio_util::sync::CancellationToken;

use crate::{
    event::Event,
    sessions::SessionStore,
    websocket::{ConnectionContext, RunExecution, RunRequest, STOP_DISCONNECTED, STOP_USER},
};

pub(crate) struct ActiveRun {
    request_id: String,
    session_id: Option<String>,
    cancel: CancellationToken,
    handle: JoinHandle<()>,
    stop_reason: Arc<AtomicU8>,
}

impl ActiveRun {
    pub(crate) fn is_running(&self) -> bool {
        !self.handle.is_finished()
    }

    pub(crate) fn matches_running(&self, request_id: &str) -> bool {
        self.request_id == request_id && self.is_running()
    }

    pub(crate) fn cancel_by_user(&self) {
        self.stop_reason.store(STOP_USER, Ordering::Release);
        self.cancel.cancel();
    }

    pub(crate) fn start(
        request: RunRequest,
        context: &ConnectionContext,
        outbound: mpsc::Sender<Event>,
        stop_reason: Arc<AtomicU8>,
    ) -> Self {
        let session_id = request.session_id.clone();
        let request_id = request.request_id.clone();
        let cancel = CancellationToken::new();
        let execution = RunExecution {
            client: context.client.clone(),
            gpu: context.gpu.clone(),
            project_root: context.project_root.clone(),
            approvals: context.approvals.clone(),
            approve_reads: context.approve_reads,
            writes: context.writes.clone(),
            memory: context.memory.clone(),
            sessions: context.sessions.clone(),
            configurations: context.configurations.clone(),
            stop_reason: stop_reason.clone(),
        };
        let task_cancel = cancel.clone();
        let handle = tokio::spawn(async move {
            execution.execute(request, outbound, task_cancel).await;
        });
        Self {
            request_id,
            session_id,
            cancel,
            handle,
            stop_reason,
        }
    }

    pub(crate) async fn stop_after_disconnect(self, sessions: Option<&SessionStore>) {
        let Self {
            request_id,
            session_id,
            cancel,
            mut handle,
            stop_reason,
        } = self;
        let _ =
            stop_reason.compare_exchange(0, STOP_DISCONNECTED, Ordering::AcqRel, Ordering::Acquire);
        cancel.cancel();
        if time::timeout(std::time::Duration::from_secs(5), &mut handle)
            .await
            .is_err()
        {
            handle.abort();
            let _ = handle.await;
            if let (Some(session_id), Some(store)) = (session_id, sessions) {
                let cancelled = stop_reason.load(Ordering::Acquire) == STOP_USER;
                let status = if cancelled {
                    "cancelled"
                } else {
                    "interrupted"
                };
                let _ = store
                    .fail_run(
                        session_id,
                        request_id,
                        status,
                        "Connexion WebSocket interrompue".to_owned(),
                    )
                    .await;
            }
        }
    }
}
