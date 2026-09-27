use std::{path::PathBuf, sync::Arc};

use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use tokio::sync::{Mutex, Semaphore, mpsc};
use tokio_util::sync::CancellationToken;

use crate::{
    agents::{AgentExecution, MemoryStore},
    configurations::ConfigurationStore,
    event::Event,
    providers::Client,
    sessions::SessionStore,
    tools::ToolApprovalGate,
    websocket::RunRequest,
};

pub(crate) struct RunExecution {
    pub client: Client,
    pub gpu: Arc<Semaphore>,
    pub project_root: Arc<PathBuf>,
    pub approvals: ToolApprovalGate,
    pub approve_reads: bool,
    pub writes: Arc<Mutex<()>>,
    pub memory: Option<MemoryStore>,
    pub sessions: Option<SessionStore>,
    pub configurations: Option<ConfigurationStore>,
}

impl RunExecution {
    pub(crate) async fn execute(
        self,
        request: RunRequest,
        outbound: mpsc::Sender<Event>,
        cancel: CancellationToken,
    ) {
        let request_id = request.request_id.clone();
        let _ = outbound
            .send(Event::new("run.queued", &request_id, json!({})))
            .await;
        let permit = tokio::select! {
            () = cancel.cancelled() => {
                let _ = outbound
                    .send(Event::new(
                        "run.cancelled",
                        &request_id,
                        json!({
                            "error": "Annulé avant démarrage",
                        }),
                    ))
                    .await;
                return;
            }
            result = self.gpu.acquire_owned() => {
                match result {
                    Ok(permit) => permit,
                    Err(_) => return,
                }
            }
        };
        let session_id = request.session_id.clone();
        let resolved: Result<_> = if let Some(session_id) = &session_id {
            match self.sessions.as_ref() {
                Some(store) => {
                    store
                        .begin_run(
                            session_id.clone(),
                            request.expected_revision.unwrap_or_default(),
                            request_id.clone(),
                            request.messages[0].clone(),
                        )
                        .await
                }
                None => Err(anyhow!("Persistance des sessions désactivée")),
            }
        } else if let Some(mode) = request.mode {
            mode.validate().map(|mode| (request.messages, mode, 0))
        } else {
            match self.configurations.as_ref() {
                Some(store) => {
                    let configuration_id = request.configuration_id.unwrap_or_default();
                    match store.load(configuration_id).await {
                        Ok(Some(configuration)) => {
                            if request
                                .configuration_revision
                                .is_some_and(|revision| revision != configuration.revision)
                            {
                                Err(anyhow!("Révision de configuration obsolète"))
                            } else {
                                Ok((request.messages, configuration.mode, 0))
                            }
                        }
                        Ok(None) => Err(anyhow!("Configuration introuvable")),
                        Err(error) => Err(error),
                    }
                }
                None => Err(anyhow!("Persistance des configurations désactivée")),
            }
        };
        let (history, mode, session_revision) = match resolved {
            Ok(resolved) => resolved,
            Err(error) => {
                let _ = outbound
                    .send(Event::new(
                        "run.failed",
                        &request_id,
                        json!({
                            "error": error.to_string(),
                        }),
                    ))
                    .await;
                return;
            }
        };
        if let Some(session_id) = &session_id {
            let _ = outbound
                .send(Event::new(
                    "session.run.started",
                    &request_id,
                    json!({
                        "session_id": session_id,
                        "session_revision": session_revision,
                    }),
                ))
                .await;
        }
        let _ = outbound
            .send(Event::new("run.started", &request_id, json!({})))
            .await;
        // Retenir run.completed jusqu'à la confirmation
        // de l'enregistrement de la réponse.
        let (run_tx, mut run_rx) = mpsc::channel::<Event>(128);
        let forward_tx = outbound.clone();
        let forward = tokio::spawn(async move {
            let mut final_answer = None;
            while let Some(event) = run_rx.recv().await {
                if event.kind == "run.completed" {
                    final_answer = event
                        .data
                        .get("result")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    continue;
                }
                let _ = forward_tx.send(event).await;
            }
            final_answer
        });
        let result = AgentExecution::run(
            &self.client,
            &mode,
            &history,
            &request_id,
            &run_tx,
            &cancel,
            &self.project_root,
            &self.approvals,
            self.approve_reads,
            &self.writes,
            self.memory.as_ref(),
        )
        .await;
        drop(run_tx);
        let answer = forward.await.unwrap_or(None);
        let outcome = match result {
            Ok(()) if !cancel.is_cancelled() => {
                answer.ok_or_else(|| anyhow!("Exécution terminée sans réponse finale"))
            }
            Ok(()) => Err(anyhow!("Exécution annulée")),
            Err(error) => Err(error),
        };
        match outcome {
            Ok(answer) => {
                let persisted = match (session_id.as_ref(), self.sessions.as_ref()) {
                    (Some(session_id), Some(store)) => store
                        .complete_run(session_id.clone(), request_id.clone(), answer.clone())
                        .await
                        .map(Some),
                    (None, _) => Ok(None),
                    _ => Err(anyhow!("Session indisponible")),
                };
                match persisted {
                    Ok(revision) => {
                        let _ = outbound
                            .send(Event::new(
                                "run.completed",
                                &request_id,
                                json!({
                                    "result": answer,
                                    "session_revision": revision,
                                }),
                            ))
                            .await;
                    }
                    Err(error) => {
                        if let (Some(session_id), Some(store)) =
                            (session_id.as_ref(), self.sessions.as_ref())
                        {
                            let _ = store
                                .fail_run(
                                    session_id.clone(),
                                    request_id.clone(),
                                    false,
                                    error.to_string(),
                                )
                                .await;
                        }
                        let _ = outbound
                            .send(Event::new(
                                "run.failed",
                                &request_id,
                                json!({
                                    "error": error.to_string(),
                                }),
                            ))
                            .await;
                    }
                }
            }

            Err(error) => {
                let cancelled = cancel.is_cancelled();
                let mut reason = error.to_string();
                if let (Some(session_id), Some(store)) =
                    (session_id.as_ref(), self.sessions.as_ref())
                    && let Err(storage_error) = store
                        .fail_run(
                            session_id.clone(),
                            request_id.clone(),
                            cancelled,
                            reason.clone(),
                        )
                        .await
                {
                    reason = format!("{reason}; journalisation : {storage_error}");
                }
                let event = if cancelled {
                    "run.cancelled"
                } else {
                    "run.failed"
                };
                let _ = outbound
                    .send(Event::new(
                        event,
                        &request_id,
                        json!({
                            "error": reason,
                            "session_revision": session_id
                                .as_ref()
                                .map(|_| session_revision),
                        }),
                    ))
                    .await;
            }
        }
        drop(permit);
    }
}
