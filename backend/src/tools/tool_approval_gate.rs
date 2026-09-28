use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::{Result, bail};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::{
    select,
    sync::{Mutex, oneshot},
    time,
};

use crate::{
    event::Event,
    tools::{ApprovalRequest, pending_approval::PendingApproval},
};

#[derive(Clone, Default)]
pub(crate) struct ToolApprovalGate {
    pending: Arc<Mutex<HashMap<(String, String), PendingApproval>>>,
}

impl ToolApprovalGate {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn preview_sha256(preview: &Value) -> Result<String> {
        let encoded = serde_json::to_vec(preview)?;
        Ok(Sha256::digest(encoded)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect())
    }

    pub(crate) async fn cancel_all(&self) {
        let pending = {
            let mut pending = self.pending.lock().await;
            pending
                .drain()
                .map(|(_, request)| request.response)
                .collect::<Vec<_>>()
        };
        for response in pending {
            let _ = response.send(false);
        }
    }

    pub(crate) async fn resolve(
        &self,
        request_id: &str,
        call_id: &str,
        approved: bool,
        preview_sha256: Option<&str>,
    ) -> bool {
        let key = (request_id.to_owned(), call_id.to_owned());
        let mut pending = self.pending.lock().await;
        let Some(request) = pending.get(&key) else {
            return false;
        };
        if approved && request.expected_sha256.as_deref() != preview_sha256 {
            return false;
        }
        let Some(request) = pending.remove(&key) else {
            return false;
        };
        drop(pending);
        request.response.send(approved).is_ok()
    }

    pub(crate) async fn request(&self, request: ApprovalRequest<'_>) -> Result<()> {
        let key = (request.request_id.to_owned(), request.call_id.to_owned());
        let (response, receiver) = oneshot::channel();

        {
            let mut pending = self.pending.lock().await;
            if pending.contains_key(&key) {
                bail!("Identifiant d'autorisation dupliqué");
            }
            pending.insert(
                key.clone(),
                PendingApproval {
                    expected_sha256: request.preview_sha256.map(str::to_owned),
                    response,
                },
            );
        }

        let notification = Event::new(
            "approval.required",
            request.request_id,
            json!({
                "agent_id": request.agent_id,
                "call_id": request.call_id,
                "tool": request.tool,
                "arguments": request.arguments,
                "preview_sha256": request.preview_sha256,
            }),
        );

        let result = async {
            select! {
                () = request.cancel.cancelled() => bail!("Exécution annulée"),
                delivered = request.outbound.send(notification) => { delivered?; }
            }

            let decision = select! {
                () = request.cancel.cancelled() => bail!("Exécution annulée"),
                result = time::timeout(Duration::from_secs(120), receiver) => result??,
            };

            if !decision {
                bail!("Autorisation refusée");
            }
            Ok(())
        }
        .await;

        self.pending.lock().await.remove(&key);
        result
    }
}
