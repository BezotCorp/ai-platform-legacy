use axum::extract::ws::{Message as WsMessage, WebSocket};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::{
    sync::{Mutex, Semaphore, mpsc},
    task::JoinHandle,
    time,
};
use tokio_util::sync::CancellationToken;

use crate::{
    agents::MemoryStore,
    configurations::ConfigurationStore,
    event::Event,
    providers::Client,
    sessions::SessionStore,
    tools::ToolApprovalGate,
    websocket::{Command, RunExecution, RunRequest, models::list},
};

fn match_token(provided: &str, expected: &str) -> bool {
    let left = provided.as_bytes();
    let right = expected.as_bytes();
    if left.len() != right.len() {
        return false;
    }
    let mut different = 0u8;
    for (&a, &b) in left.iter().zip(right) {
        different |= a ^ b;
    }
    different == 0
}

pub(crate) async fn serve(
    mut socket: WebSocket,
    client: Client,
    expected_token: Arc<str>,
    gpu: Arc<Semaphore>,
    project_root: Arc<PathBuf>,
    writes: Arc<Mutex<()>>,
    approve_reads: bool,
    memory: Option<MemoryStore>,
    sessions: Option<SessionStore>,
    configurations: Option<ConfigurationStore>,
    shutdown: CancellationToken,
) {
    let approvals = ToolApprovalGate::new();
    let first = tokio::time::timeout(Duration::from_secs(5), socket.recv()).await;
    let Ok(Some(Ok(WsMessage::Text(text)))) = first else {
        return;
    };
    let Ok(Command::Authenticate { token }) = serde_json::from_str::<Command>(&text) else {
        return;
    };
    if !match_token(&token, &expected_token) {
        return;
    }
    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = mpsc::channel::<Event>(128);
    let writer = tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            let Ok(encoded) = serde_json::to_string(&event) else {
                break;
            };
            if sink.send(WsMessage::Text(encoded.into())).await.is_err() {
                break;
            }
        }
    });
    let _ = tx.send(Event::new("authenticated", "", json!({}))).await;
    let mut active: Option<(String, CancellationToken, JoinHandle<()>, Option<String>)> = None;
    loop {
        let frame = tokio::select! {
            () = shutdown.cancelled() => break,
            frame = stream.next() => frame,
        };
        let Some(frame) = frame else {
            break;
        };
        let Ok(WsMessage::Text(text)) = frame else {
            break;
        };
        if text.len() > 256 * 1024 {
            let _ = tx
                .send(Event::new(
                    "error",
                    "",
                    json!({
                        "error": "Message trop volumineux",
                    }),
                ))
                .await;
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            let _ = tx
                .send(Event::new(
                    "error",
                    "",
                    json!({
                        "error": "JSON invalide",
                    }),
                ))
                .await;
            continue;
        };
        if value.get("type").and_then(Value::as_str) == Some("run.start") {
            if active
                .as_ref()
                .is_some_and(|(_, _, handle, _)| !handle.is_finished())
            {
                let _ = tx
                    .send(Event::new(
                        "error",
                        "",
                        json!({
                            "error": "Exécution déjà en cours",
                        }),
                    ))
                    .await;
                continue;
            }
            let request: RunRequest = match serde_json::from_value(value) {
                Ok(request) => request,
                Err(error) => {
                    let _ = tx
                        .send(Event::new(
                            "error",
                            "",
                            json!({
                                "error": error.to_string(),
                            }),
                        ))
                        .await;
                    continue;
                }
            };
            if let Err(error) = request.validate() {
                let _ = tx
                    .send(Event::new(
                        "error",
                        &request.request_id,
                        json!({
                            "error": error.to_string(),
                        }),
                    ))
                    .await;
                continue;
            }
            let session_id = request.session_id.clone();
            let request_id = request.request_id.clone();
            let cancel = CancellationToken::new();
            let execution = RunExecution {
                client: client.clone(),
                gpu: gpu.clone(),
                project_root: project_root.clone(),
                approvals: approvals.clone(),
                approve_reads,
                writes: writes.clone(),
                memory: memory.clone(),
                sessions: sessions.clone(),
                configurations: configurations.clone(),
            };
            let outbound = tx.clone();
            let task_cancel = cancel.clone();
            let handle = tokio::spawn(async move {
                execution.execute(request, outbound, task_cancel).await;
            });
            active = Some((request_id, cancel, handle, session_id));
        } else {
            match serde_json::from_value::<Command>(value) {
                Ok(Command::ModelsList { request_id }) => {
                    let event = list(&client, &request_id).await.unwrap_or_else(|error| {
                        Event::new(
                            "error",
                            &request_id,
                            json!({
                                "error":
                                    error.to_string(),
                            }),
                        )
                    });
                    if tx.send(event).await.is_err() {
                        break;
                    }
                }
                Ok(Command::ConfigurationSave {
                    request_id,
                    configuration_id,
                    expected_revision,
                    mode,
                }) => {
                    let result = match configurations.as_ref() {
                        Some(store) => store.save(configuration_id, expected_revision, mode).await,

                        None => Err(anyhow::anyhow!("Persistance des configurations désactivée")),
                    };
                    let event = match result {
                        Ok(configuration) => Event::new(
                            "configuration.saved",
                            &request_id,
                            json!({
                                "configuration": configuration,
                            }),
                        ),
                        Err(error) => Event::new(
                            "configuration.failed",
                            &request_id,
                            json!({
                                "error": error.to_string(),
                            }),
                        ),
                    };
                    if tx.send(event).await.is_err() {
                        break;
                    }
                }
                Ok(Command::ConfigurationLoad {
                    request_id,
                    configuration_id,
                }) => {
                    let result = match configurations.as_ref() {
                        Some(store) => store.load(configuration_id).await,

                        None => Err(anyhow::anyhow!("Persistance des configurations désactivée")),
                    };
                    let event = match result {
                        Ok(Some(configuration)) => Event::new(
                            "configuration.loaded",
                            &request_id,
                            json!({
                                "configuration": configuration,
                            }),
                        ),
                        Ok(None) => Event::new(
                            "configuration.failed",
                            &request_id,
                            json!({
                                "error": "Configuration introuvable",
                            }),
                        ),
                        Err(error) => Event::new(
                            "configuration.failed",
                            &request_id,
                            json!({
                                "error": error.to_string(),
                            }),
                        ),
                    };
                    if tx.send(event).await.is_err() {
                        break;
                    }
                }
                Ok(Command::ConfigurationList { request_id }) => {
                    let result = match configurations.as_ref() {
                        Some(store) => store.list().await,
                        None => Err(anyhow::anyhow!("Persistance des configurations désactivée")),
                    };
                    let event = match result {
                        Ok(configurations) => Event::new(
                            "configuration.list",
                            &request_id,
                            json!({
                                "configurations": configurations,
                            }),
                        ),
                        Err(error) => Event::new(
                            "configuration.failed",
                            &request_id,
                            json!({
                                "error": error.to_string(),
                            }),
                        ),
                    };
                    if tx.send(event).await.is_err() {
                        break;
                    }
                }
                Ok(Command::ConfigurationDelete {
                    request_id,
                    configuration_id,
                    expected_revision,
                }) => {
                    let result = match configurations.as_ref() {
                        Some(store) => store.delete(configuration_id, expected_revision).await,
                        None => Err(anyhow::anyhow!("Persistance des configurations désactivée")),
                    };
                    let event = match result {
                        Ok(true) => Event::new("configuration.deleted", &request_id, json!({})),
                        Ok(false) => Event::new(
                            "configuration.failed",
                            &request_id,
                            json!({
                                "error":
                                    "Configuration absente ou révision modifiée",
                            }),
                        ),
                        Err(error) => Event::new(
                            "configuration.failed",
                            &request_id,
                            json!({
                                "error": error.to_string(),
                            }),
                        ),
                    };
                    if tx.send(event).await.is_err() {
                        break;
                    }
                }
                Ok(Command::SessionBind {
                    request_id,
                    session_id,
                    expected_revision,
                    configuration_id,
                    configuration_revision,
                }) => {
                    let result: anyhow::Result<_> = async {
                        let configurations = configurations.as_ref().ok_or_else(|| {
                            anyhow::anyhow!("Persistance des configurations désactivée")
                        })?;
                        let sessions = sessions.as_ref().ok_or_else(|| {
                            anyhow::anyhow!("Persistance des sessions désactivée")
                        })?;
                        let configuration = configurations
                            .load(configuration_id)
                            .await?
                            .ok_or_else(|| anyhow::anyhow!("Configuration introuvable"))?;
                        if configuration_revision
                            .is_some_and(|revision| revision != configuration.revision)
                        {
                            anyhow::bail!("Révision de configuration obsolète");
                        }
                        sessions
                            .bind(session_id, expected_revision, configuration)
                            .await
                    }
                    .await;
                    let event = match result {
                        Ok(session) => {
                            Event::new("session.bound", &request_id, json!({"session": session}))
                        }
                        Err(error) => Event::new(
                            "session.failed",
                            &request_id,
                            json!({
                                "error": error.to_string(),
                            }),
                        ),
                    };
                    if tx.send(event).await.is_err() {
                        break;
                    }
                }
                Ok(Command::SessionResume {
                    request_id,
                    session_id,
                }) => {
                    let result = match sessions.as_ref() {
                        Some(store) => store.resume(session_id).await,
                        None => Err(anyhow::anyhow!("Persistance des sessions désactivée")),
                    };
                    let event = match result {
                        Ok(Some(session)) => {
                            Event::new("session.resumed", &request_id, json!({"session": session}))
                        }
                        Ok(None) => Event::new(
                            "session.failed",
                            &request_id,
                            json!({
                                "error": "Session introuvable",
                            }),
                        ),
                        Err(error) => Event::new(
                            "session.failed",
                            &request_id,
                            json!({
                                "error": error.to_string(),
                            }),
                        ),
                    };
                    if tx.send(event).await.is_err() {
                        break;
                    }
                }
                Ok(Command::SessionArchive {
                    request_id,
                    session_id,
                    before_sequence,
                }) => {
                    let result = match sessions.as_ref() {
                        Some(store) => store.archive(session_id, before_sequence).await,
                        None => Err(anyhow::anyhow!("Persistance des sessions désactivée")),
                    };
                    let event = match result {
                        Ok(messages) => Event::new(
                            "session.archive",
                            &request_id,
                            json!({ "messages": messages }),
                        ),
                        Err(error) => Event::new(
                            "session.failed",
                            &request_id,
                            json!({ "error": error.to_string() }),
                        ),
                    };
                    if tx.send(event).await.is_err() {
                        break;
                    }
                }
                Ok(Command::SessionRuns {
                    request_id,
                    session_id,
                }) => {
                    let result = match sessions.as_ref() {
                        Some(store) => store.runs(session_id).await,

                        None => Err(anyhow::anyhow!("Persistance des sessions désactivée")),
                    };
                    let event = match result {
                        Ok(runs) => Event::new("session.runs", &request_id, json!({"runs": runs})),
                        Err(error) => Event::new(
                            "session.failed",
                            &request_id,
                            json!({
                                "error": error.to_string(),
                            }),
                        ),
                    };
                    if tx.send(event).await.is_err() {
                        break;
                    }
                }
                Ok(Command::SessionSave {
                    request_id,
                    session_id,
                    expected_revision,
                    messages,
                }) => {
                    let result = match sessions.as_ref() {
                        Some(store) => store.save(session_id, expected_revision, messages).await,

                        None => Err(anyhow::anyhow!("Persistance des sessions désactivée")),
                    };
                    let event = match result {
                        Ok(session) => {
                            Event::new("session.saved", &request_id, json!({ "session": session }))
                        }
                        Err(error) => Event::new(
                            "session.failed",
                            &request_id,
                            json!({
                                "error": error.to_string(),
                            }),
                        ),
                    };
                    if tx.send(event).await.is_err() {
                        break;
                    }
                }
                Ok(Command::SessionLoad {
                    request_id,
                    session_id,
                }) => {
                    let result = match sessions.as_ref() {
                        Some(store) => store.load(session_id).await,
                        None => Err(anyhow::anyhow!("Persistance des sessions désactivée")),
                    };
                    let event = match result {
                        Ok(Some(history)) => {
                            Event::new("session.loaded", &request_id, json!({ "history": history }))
                        }
                        Ok(None) => Event::new(
                            "session.failed",
                            &request_id,
                            json!({
                                "error": "Session introuvable",
                            }),
                        ),
                        Err(error) => Event::new(
                            "session.failed",
                            &request_id,
                            json!({
                                "error": error.to_string(),
                            }),
                        ),
                    };
                    if tx.send(event).await.is_err() {
                        break;
                    }
                }
                Ok(Command::SessionList { request_id }) => {
                    let result = match sessions.as_ref() {
                        Some(store) => store.list().await,

                        None => Err(anyhow::anyhow!("Persistance des sessions désactivée")),
                    };
                    let event = match result {
                        Ok(items) => {
                            Event::new("session.list", &request_id, json!({ "sessions": items }))
                        }
                        Err(error) => Event::new(
                            "session.failed",
                            &request_id,
                            json!({
                                "error": error.to_string(),
                            }),
                        ),
                    };
                    if tx.send(event).await.is_err() {
                        break;
                    }
                }
                Ok(Command::SessionDelete {
                    request_id,
                    session_id,
                    expected_revision,
                }) => {
                    let result = match sessions.as_ref() {
                        Some(store) => store.delete(session_id, expected_revision).await,
                        None => Err(anyhow::anyhow!("Persistance des sessions désactivée")),
                    };
                    let event = match result {
                        Ok(true) => Event::new("session.deleted", &request_id, json!({})),
                        Ok(false) => Event::new(
                            "session.failed",
                            &request_id,
                            json!({
                                "error":
                                    "Session absente ou révision modifiée",
                            }),
                        ),
                        Err(error) => Event::new(
                            "session.failed",
                            &request_id,
                            json!({
                                "error": error.to_string(),
                            }),
                        ),
                    };
                    if tx.send(event).await.is_err() {
                        break;
                    }
                }
                Ok(Command::RunCancel { request_id }) => match &active {
                    Some((id, cancel, handle, _)) if id == &request_id && !handle.is_finished() => {
                        cancel.cancel();
                    }
                    _ => {
                        let _ = tx
                            .send(Event::new(
                                "error",
                                &request_id,
                                json!({
                                    "error":
                                        "Aucune exécution active correspondante",
                                }),
                            ))
                            .await;
                    }
                },
                Ok(Command::ApprovalResolve {
                    request_id,
                    call_id,
                    approved,
                    preview_sha256,
                }) => {
                    let belongs_to_run = active.as_ref().is_some_and(|(id, _, handle, _)| {
                        id == &request_id && !handle.is_finished()
                    });
                    let resolved = if belongs_to_run {
                        approvals
                            .resolve(&request_id, &call_id, approved, preview_sha256.as_deref())
                            .await
                    } else {
                        false
                    };
                    let _ = tx
                        .send(Event::new(
                            "approval.resolved",
                            &request_id,
                            json!({
                                "call_id": call_id,
                                "accepted": resolved,
                                "approved": if resolved {
                                    Some(approved)
                                } else {
                                    None
                                },
                            }),
                        ))
                        .await;
                }
                _ => {
                    let _ = tx
                        .send(Event::new(
                            "error",
                            "",
                            json!({
                                "error": "Commande inconnue",
                            }),
                        ))
                        .await;
                }
            }
        }
    }
    if let Some((request_id, token, mut handle, session_id)) = active {
        token.cancel();
        if time::timeout(Duration::from_secs(5), &mut handle)
            .await
            .is_err()
        {
            handle.abort();
            if let (Some(session_id), Some(store)) = (session_id, sessions.as_ref()) {
                let _ = store
                    .fail_run(
                        session_id,
                        request_id,
                        true,
                        "Connexion WebSocket interrompue".to_owned(),
                    )
                    .await;
            }
        }
    }
    drop(tx);
    writer.abort();
}
