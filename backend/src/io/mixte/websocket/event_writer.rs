use axum::extract::ws::{Message as WsMessage, WebSocket};
use futures_util::{SinkExt, stream::SplitSink};
use std::time::Duration;
use tokio::{sync::mpsc, task::JoinHandle, time};
use tokio_util::sync::CancellationToken;

use crate::io::Event;

pub(crate) struct EventWriter {
    pub(crate) outbound: mpsc::Sender<Event>,
    pub(crate) closed: CancellationToken,
    handle: JoinHandle<()>,
}

impl EventWriter {
    pub(crate) fn spawn(
        mut sink: SplitSink<WebSocket, WsMessage>,
        shutdown: CancellationToken,
    ) -> Self {
        let (outbound, mut rx) = mpsc::channel::<Event>(128);
        let closed = CancellationToken::new();
        let closed_task = closed.clone();
        let handle = tokio::spawn(async move {
            loop {
                let event = tokio::select! {
                    () = shutdown.cancelled() => break,
                    event = rx.recv() => match event {
                        Some(event) => event,
                        None => break,
                    },
                };
                let Ok(encoded) = serde_json::to_string(&event) else {
                    break;
                };
                let sent = tokio::select! {
                    () = shutdown.cancelled() => break,
                    sent = time::timeout(
                        Duration::from_secs(5),
                        sink.send(WsMessage::Text(encoded.into())),
                    ) => sent,
                };
                if !matches!(sent, Ok(Ok(()))) {
                    break;
                }
            }
            closed_task.cancel();
        });
        Self {
            outbound,
            closed,
            handle,
        }
    }

    pub(crate) async fn stop(self) {
        drop(self.outbound);
        self.handle.abort();
        let _ = self.handle.await;
    }
}
