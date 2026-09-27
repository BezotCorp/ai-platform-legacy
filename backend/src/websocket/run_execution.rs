use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
};

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

pub(crate) const STOP_USER: u8 = 1;
pub(crate) const STOP_DISCONNECTED: u8 = 2;

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
    pub stop_reason: Arc<AtomicU8>,
}

impl RunExecution {
    async fn terminate(
        &self,
        session_id: Option<&str>,
        request_id: &str,
        session_revision: Option<i64>,
        status: &'static str,
        error: String,
        outbound: &mpsc::Sender<Event>,
    ) {
        let stored = if let (Some(session_id), Some(store)) = (session_id, self.sessions.as_ref()) {
            store.fail_run(
                session_id.to_owned(),
                request_id.to_owned(),
                status,
                error.clone(),
            ).await
        } else {
            Ok(())
        };
        let (kind, message) = match stored {
            Ok(()) => (match status {
                "cancelled" => "run.cancelled",
                "interrupted" => "run.interrupted",
                _ => "run.failed",
            }, error),
            Err(storage_error) => (
                "run.failed",
                format!("{error}; impossible d'enregistrer l'état : {storage_error}"),
            ),
        };
        let _ = outbound.send(Event::new(
            kind,
            request_id,
            json!({
                "error": message,
                "session_revision": session_revision,
            }),
        )).await;
    }

    fn cancellation(&self) -> (&'static str, &'static str) {
        if self.stop_reason.load(Ordering::Acquire) == STOP_DISCONNECTED {
            ("interrupted", "Connexion perdue ou backend arrêté")
        } else {
            ("cancelled", "Exécution annulée")
        }
    }

    pub(crate) async fn execute(
        self,
        request: RunRequest,
        outbound: mpsc::Sender<Event>,
        cancel: CancellationToken,
    ) {
        let request_id = request.request_id.clone();
        let session_id = request.session_id.clone();

        // La réservation SQLite précède l'attente du GPU et l'annonce run.queued.
        // Une demande liée conserve son prompt même si le client se déconnecte.
        let resolved: Result<_> = if let Some(session_id) = &session_id {
            match self.sessions.as_ref() {
                Some(store) => store.queue_run(
                    session_id.clone(),
                    request.expected_revision.unwrap_or_default(),
                    request_id.clone(),
                    request.messages[0].clone(),
                ).await,
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
                            if request.configuration_revision
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
            Ok(value) => value,
            Err(error) => {
                let _ = outbound.send(Event::new(
                    "run.failed",
                    &request_id,
                    json!({ "error": error.to_string() }),
                )).await;
                return;
            }
        };
        let revision = session_id.as_ref().map(|_| session_revision);
        let _ = outbound.send(Event::new(
            "run.queued",
            &request_id,
            json!({
                "session_id": session_id,
                "session_revision": revision,
            }),
        )).await;

        let permit = tokio::select! {
            () = cancel.cancelled() => {
                let (status, reason) = self.cancellation();
                self.terminate(
                    session_id.as_deref(), &request_id, revision,
                    status, reason.to_owned(), &outbound,
                ).await;
                return;
            }
            result = self.gpu.clone().acquire_owned() => {
                match result {
                    Ok(permit) => permit,
                    Err(error) => {
                        self.terminate(
                            session_id.as_deref(), &request_id, revision,
                            "failed", error.to_string(), &outbound,
                        ).await;
                        return;
                    }
                }
            }
        };

        // La déconnexion peut arriver pendant l'attente du sémaphore.
        if cancel.is_cancelled() {
            let (status, reason) = self.cancellation();
            self.terminate(
                session_id.as_deref(), &request_id, revision,
                status, reason.to_owned(), &outbound,
            ).await;
            return;
        }
        if let (Some(session_id), Some(store)) = (&session_id, &self.sessions) {
            if let Err(error) = store.start_run(
                session_id.clone(), request_id.clone(),
            ).await {
                self.terminate(
                    Some(session_id), &request_id, revision,
                    "failed", error.to_string(), &outbound,
                ).await;
                return;
            }
        }
        if cancel.is_cancelled() {
            let (status, reason) = self.cancellation();
            self.terminate(
                session_id.as_deref(), &request_id, revision,
                status, reason.to_owned(), &outbound,
            ).await;
            return;
        }
        if let Some(session_id) = &session_id {
            let _ = outbound.send(Event::new(
                "session.run.started",
                &request_id,
                json!({
                    "session_id": session_id,
                    "session_revision": session_revision,
                }),
            )).await;
        }
        let _ = outbound.send(Event::new(
            "run.started", &request_id, json!({}),
        )).await;

        // Différer run.completed jusqu'à la validation de la transaction SQLite.
        let (run_tx, mut run_rx) = mpsc::channel::<Event>(128);
        let forward_tx = outbound.clone();
        let forward = tokio::spawn(async move {
            let mut final_answer = None;
            while let Some(event) = run_rx.recv().await {
                if event.kind == "run.completed" {
                    final_answer = event.data.get("result")
                        .and_then(Value::as_str).map(str::to_owned);
                    continue;
                }
                if forward_tx.send(event).await.is_err() {
                    // Continuer à consommer les événements pour ne pas bloquer
                    // l'agent sur une connexion déjà perdue.
                    continue;
                }
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
        ).await;
        drop(run_tx);
        let answer = forward.await.unwrap_or(None);

        if cancel.is_cancelled() {
            let (status, reason) = self.cancellation();
            self.terminate(
                session_id.as_deref(), &request_id, revision,
                status, reason.to_owned(), &outbound,
            ).await;
        } else {
            match result.and_then(|()| {
                answer.ok_or_else(|| anyhow!("Exécution terminée sans réponse finale"))
            }) {
                Ok(answer) => {
                    let stored = match (session_id.as_ref(), self.sessions.as_ref()) {
                        (Some(session_id), Some(store)) => store.complete_run(
                            session_id.clone(), request_id.clone(), answer.clone(),
                        ).await.map(Some),
                        (None, _) => Ok(None),
                        _ => Err(anyhow!("Session indisponible")),
                    };
                    match stored {
                        Ok(session_revision) => {
                            let _ = outbound.send(Event::new(
                                "run.completed",
                                &request_id,
                                json!({
                                    "result": answer,
                                    "session_revision": session_revision,
                                }),
                            )).await;
                        }
                        Err(error) => {
                            self.terminate(
                                session_id.as_deref(), &request_id, revision,
                                "failed", error.to_string(), &outbound,
                            ).await;
                        }
                    }
                }
                Err(error) => {
                    self.terminate(
                        session_id.as_deref(), &request_id, revision,
                        "failed", error.to_string(), &outbound,
                    ).await;
                }
            }
        }
        drop(permit);
    }
}
