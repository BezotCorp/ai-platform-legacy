use axum::extract::ws::{Message as WsMessage, WebSocket};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::sync::{Arc, atomic::AtomicU8};
use tokio::sync::{OwnedRwLockReadGuard, mpsc, oneshot};

use crate::{
    event::Event,
    tools::ToolApprovalGate,
    websocket::{
        ActiveRun, ConnectionContext, DispatchMessage, EventWriter, RunRequest, ServerState,
        authentication::authenticate, command_dispatch::dispatch,
    },
};
pub(crate) async fn serve(
    mut socket: WebSocket,
    state: ServerState,
    connection_guard: OwnedRwLockReadGuard<()>,
) {
    if !authenticate(&mut socket, &state.token, &state.shutdown).await {
        return;
    }
    let context = ConnectionContext {
        client: state.client,
        gpu: state.gpu,
        project_root: state.project_root,
        writes: state.writes,
        approve_reads: state.approve_reads,
        memory: state.memory,
        sessions: state.sessions,
        configurations: state.configurations,
        approvals: ToolApprovalGate::new(),
        shutdown: state.shutdown,
    };
    let (sink, mut stream) = socket.split();
    let writer = EventWriter::spawn(sink, context.shutdown.clone());
    let tx = writer.outbound.clone();
    let _ = tx.send(Event::new("authenticated", "", json!({}))).await;
    let mut active = None;
    let (commands, mut received_commands) = mpsc::channel::<DispatchMessage>(64);
    let worker_context = context.clone();
    let worker_events = tx.clone();
    let worker_closed = writer.closed.clone();
    let worker = tokio::spawn(async move {
        loop {
            let next = tokio::select! {
                () = worker_context.shutdown.cancelled() => break,
                () = worker_closed.cancelled() => break,
                next = received_commands.recv() => next,
            };
            match next {
                Some(DispatchMessage::Execute(value)) => {
                    if !dispatch(value, &worker_context, &None, &worker_events).await {
                        break;
                    }
                }
                Some(DispatchMessage::Drain(acknowledge)) => {
                    let _ = acknowledge.send(());
                }
                None => break,
            }
        }
    });
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
            let (acknowledge, completed) = oneshot::channel();
            let queued = tokio::select! {
                () = context.shutdown.cancelled() => false,
                () = writer.closed.cancelled() => false,
                result = commands.send(DispatchMessage::Drain(acknowledge)) => result.is_ok(),
            };
            if !queued {
                break;
            }
            let drained = tokio::select! {
                () = context.shutdown.cancelled() => false,
                () = writer.closed.cancelled() => false,
                result = completed => result.is_ok(),
            };
            if !drained {
                break;
            }
            start_run(value, &context, tx.clone(), &mut active).await;
        } else if matches!(
            value.get("type").and_then(Value::as_str),
            Some("run.cancel" | "approval.resolve")
        ) {
            let keep_open = tokio::select! {
                () = context.shutdown.cancelled() => false,
                () = writer.closed.cancelled() => false,
                keep_open = dispatch(value, &context, &active, &tx) => keep_open,
            };
            if !keep_open {
                break;
            }
        } else {
            let queued = tokio::select! {
                () = context.shutdown.cancelled() => false,
                () = writer.closed.cancelled() => false,
                result = commands.send(DispatchMessage::Execute(value)) => result.is_ok(),
            };
            if !queued {
                break;
            }
        }
    }
    drop(commands);
    worker.abort();
    let _ = worker.await;
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
