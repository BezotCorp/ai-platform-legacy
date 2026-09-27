use axum::extract::ws::{Message as WsMessage, WebSocket};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicU8},
};
use tokio::sync::{Mutex, OwnedRwLockReadGuard, Semaphore, mpsc};
use tokio_util::sync::CancellationToken;

use crate::{
    agents::MemoryStore,
    configurations::ConfigurationStore,
    event::Event,
    providers::Client,
    sessions::SessionStore,
    tools::ToolApprovalGate,
    websocket::{
        ActiveRun, ConnectionContext, RunRequest, authentication::authenticate,
        command_dispatch::dispatch, event_writer::EventWriter,
    },
};
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
    connection_guard: OwnedRwLockReadGuard<()>,
) {
    if !authenticate(&mut socket, &expected_token, &shutdown).await {
        return;
    }
    let context = ConnectionContext {
        client,
        gpu,
        project_root,
        writes,
        approve_reads,
        memory,
        sessions,
        configurations,
        approvals: ToolApprovalGate::new(),
        shutdown,
    };
    let (sink, mut stream) = socket.split();
    let writer = EventWriter::spawn(sink, context.shutdown.clone());
    let tx = writer.outbound.clone();
    let _ = tx.send(Event::new("authenticated", "", json!({}))).await;
    let mut active = None;

    loop {
        let frame = tokio::select! {
            () = context.shutdown.cancelled() => break,
            () = writer.closed.cancelled() => break,
            frame = stream.next() => frame,
        };
        let Some(frame) = frame else {
            break;
        };
        let Ok(WsMessage::Text(text)) = frame else {
            break;
        };
        let Some(value) = decode_command_text(&text, &tx).await else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) == Some("run.start") {
            if active.as_ref().is_some_and(ActiveRun::is_running) {
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
            start_run(value, &context, tx.clone(), &mut active).await;
        } else {
            // Les lectures, écritures et appels Ollama peuvent attendre :
            // l'arrêt du backend doit pouvoir annuler la commande en cours.
            let keep_open = tokio::select! {
                () = context.shutdown.cancelled() => false,
                () = writer.closed.cancelled() => false,
                keep_open = dispatch(value, &context, &active, &tx) => keep_open,
            };
            if !keep_open {
                break;
            }
        }
    }
    cleanup_connection(active, &context, writer, connection_guard).await;
}

async fn decode_command_text(text: &str, tx: &mpsc::Sender<Event>) -> Option<Value> {
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
        return None;
    }
    match serde_json::from_str::<Value>(text) {
        Ok(value) => Some(value),
        Err(_) => {
            let _ = tx
                .send(Event::new(
                    "error",
                    "",
                    json!({
                        "error": "JSON invalide",
                    }),
                ))
                .await;
            None
        }
    }
}

async fn start_run(
    value: Value,
    context: &ConnectionContext,
    tx: mpsc::Sender<Event>,
    active: &mut Option<ActiveRun>,
) {
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
            return;
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
        return;
    }
    let stop_reason = Arc::new(AtomicU8::new(0));
    *active = Some(ActiveRun::start(request, context, tx, stop_reason));
}

async fn cleanup_connection(
    active: Option<ActiveRun>,
    context: &ConnectionContext,
    writer: EventWriter,
    connection_guard: OwnedRwLockReadGuard<()>,
) {
    // Révoquer immédiatement toutes les approbations encore en attente.
    // Une décision reçue après la déconnexion ne peut plus autoriser une écriture.
    context.approvals.cancel_all().await;
    if let Some(active) = active {
        active
            .stop_after_disconnect(context.sessions.as_ref())
            .await;
    }
    writer.stop().await;
    drop(connection_guard);
}
