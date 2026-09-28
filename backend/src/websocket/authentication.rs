use axum::extract::ws::{Message as WsMessage, WebSocket};
use std::{sync::Arc, time::Duration};
use tokio::time;
use tokio_util::sync::CancellationToken;

use crate::websocket::Command;

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

pub(crate) async fn authenticate(
    socket: &mut WebSocket,
    expected_token: &Arc<str>,
    shutdown: &CancellationToken,
) -> bool {
    let first = tokio::select! {
        () = shutdown.cancelled() => return false,
        received = time::timeout(Duration::from_secs(5), socket.recv()) => received,
    };
    let Ok(Some(Ok(WsMessage::Text(text)))) = first else {
        return false;
    };
    let Ok(Command::Authenticate { token }) = serde_json::from_str::<Command>(&text) else {
        return false;
    };
    match_token(&token, expected_token)
}
