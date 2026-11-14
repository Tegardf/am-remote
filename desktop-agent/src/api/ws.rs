//! `GET /ws?token=...`: live updates.
//!
//! Server -> client messages are `{ "type": "<event>", "data": { ... } }`: `state`, `apple_music` and
//! `volume` (all three are sent on connect, in that order, and then on every change; `volume` only
//! with an audio session), plus `{ "type": "pong" }` for the client's
//! `{ "type": "ping" }`. Unknown client messages are ignored. The socket closes when its token is
//! revoked, and when the client has been silent for too long (it should ping every 15 s).

use std::time::Duration;

use axum::extract::ws::rejection::WebSocketUpgradeRejection;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Extension, State};
use axum::response::Response;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::error::RecvError;
use tokio::time::Instant;

use super::{ApiError, AppState, AuthToken};
use crate::media::{MediaEvent, PlayerState, VolumeState};

/// A client that sends nothing for this long is gone (the app pings every 15 s).
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(45);

/// A client that stops reading must not freeze its socket task (and with it the idle timeout and
/// the revocation check): a send that takes this long ends the connection.
const SEND_TIMEOUT: Duration = Duration::from_secs(10);

/// Everything the server sends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ServerMessage {
    State(PlayerState),
    Volume(VolumeState),
    AppleMusic { running: bool },
    Pong,
}

impl From<MediaEvent> for ServerMessage {
    fn from(event: MediaEvent) -> Self {
        match event {
            MediaEvent::State(state) => Self::State(state),
            MediaEvent::Volume(volume) => Self::Volume(volume),
            MediaEvent::AppleMusic { running } => Self::AppleMusic { running },
        }
    }
}

/// Everything the client may send (anything else is ignored).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Ping,
}

/// Parses a text frame; `None` for anything that is not a known client message.
fn parse_client_message(text: &str) -> Option<ClientMessage> {
    serde_json::from_str(text).ok()
}

pub async fn ws(
    State(state): State<AppState>,
    Extension(AuthToken(token)): Extension<AuthToken>,
    upgrade: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
) -> Result<Response, ApiError> {
    // A plain GET (no upgrade headers) must get the JSON error body, not axum's plain text.
    let upgrade =
        upgrade.map_err(|_| ApiError::bad_request("this endpoint needs a WebSocket upgrade"))?;
    Ok(upgrade.on_upgrade(move |socket| run(socket, state, token)))
}

async fn send(socket: &mut WebSocket, message: &ServerMessage) -> Result<(), ()> {
    let text = serde_json::to_string(message).map_err(|_| ())?;
    match tokio::time::timeout(SEND_TIMEOUT, socket.send(Message::text(text))).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(_)) => Err(()),
        Err(_) => {
            tracing::debug!("closing a WebSocket that stopped reading");
            Err(())
        }
    }
}

/// Says goodbye, but never waits long for a client that is not listening.
async fn hang_up(socket: &mut WebSocket) {
    let _ = tokio::time::timeout(SEND_TIMEOUT, socket.send(Message::Close(None))).await;
}

/// Everything a client needs to catch up: the current state, whether Apple Music runs, and the
/// volume (only when there is an audio session). In this order (contract C-002). Sent on connect,
/// and again after a lag, where any event type may have been skipped.
async fn snapshot_messages(state: &AppState) -> Vec<ServerMessage> {
    let mut messages = vec![
        ServerMessage::State(state.media.state().await),
        ServerMessage::AppleMusic {
            running: state.media.apple_music_running().await,
        },
    ];
    if let Ok(volume) = state.media.volume().await {
        messages.push(ServerMessage::Volume(volume));
    }
    messages
}

/// Sends every message in order; false if the client is gone.
async fn send_all(socket: &mut WebSocket, messages: &[ServerMessage]) -> bool {
    for message in messages {
        if send(socket, message).await.is_err() {
            return false;
        }
    }
    true
}

async fn run(mut socket: WebSocket, state: AppState, token: String) {
    // Subscribe before reading the state, so a change in between is not lost.
    let mut events = state.media.subscribe();
    let mut revoked = state.config.subscribe_revocations();
    // Revoked between the auth check and now? Nobody told us, so look once.
    if !state.config.is_authorized(&token) {
        hang_up(&mut socket).await;
        return;
    }
    if !send_all(&mut socket, &snapshot_messages(&state).await).await {
        return;
    }

    let idle_timeout = state.ws_idle_timeout;
    let idle = tokio::time::sleep(idle_timeout);
    tokio::pin!(idle);
    loop {
        tokio::select! {
            event = events.recv() => match event {
                Ok(event) => {
                    if send(&mut socket, &event.into()).await.is_err() {
                        break;
                    }
                }
                Err(RecvError::Lagged(_)) => {
                    // Fell behind: skip the missed events and resend where things stand now.
                    if !send_all(&mut socket, &snapshot_messages(&state).await).await {
                        break;
                    }
                }
                Err(RecvError::Closed) => break,
            },
            revocation = revoked.recv() => match revocation {
                Err(RecvError::Closed) => break,
                // Someone was revoked; if it was this device, hang up at once.
                Ok(()) | Err(RecvError::Lagged(_)) => {
                    if !state.config.is_authorized(&token) {
                        hang_up(&mut socket).await;
                        break;
                    }
                }
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    idle.as_mut().reset(Instant::now() + idle_timeout);
                    if parse_client_message(text.as_str()) == Some(ClientMessage::Ping)
                        && send(&mut socket, &ServerMessage::Pong).await.is_err()
                    {
                        break;
                    }
                }
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                // Binary, ping and pong frames: protocol-level keep-alives are answered by axum.
                Some(Ok(_)) => idle.as_mut().reset(Instant::now() + idle_timeout),
            },
            () = &mut idle => {
                tracing::debug!("closing a silent WebSocket");
                hang_up(&mut socket).await;
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_is_recognised_and_everything_else_is_ignored() {
        assert_eq!(
            parse_client_message(r#"{"type":"ping"}"#),
            Some(ClientMessage::Ping)
        );
        assert_eq!(
            parse_client_message(r#"{"type":"ping","extra":1}"#),
            Some(ClientMessage::Ping)
        );
        assert_eq!(parse_client_message(r#"{"type":"hello"}"#), None);
        assert_eq!(parse_client_message("not json"), None);
        assert_eq!(parse_client_message(r#"{"data":1}"#), None);
        assert_eq!(parse_client_message(""), None);
    }

    #[tokio::test]
    async fn a_lagging_client_is_resynchronised_with_every_kind_of_message() {
        use crate::config::ConfigStore;
        use crate::media::{FakeBackend, PlaybackStatus};
        use std::sync::Arc;

        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(ConfigStore::open(dir.path().join("config.json")).unwrap());
        let media = Arc::new(FakeBackend::new(true));
        let current = PlayerState {
            status: PlaybackStatus::Paused,
            title: "Now".into(),
            ..PlayerState::empty(4)
        };
        media.set_state(current.clone());
        let state =
            AppState::new(store, media, Arc::new(crate::search::FakeSearch::default())).unwrap();

        assert_eq!(
            snapshot_messages(&state).await,
            vec![
                ServerMessage::State(current),
                ServerMessage::AppleMusic { running: true },
                ServerMessage::Volume(VolumeState {
                    level: 50,
                    muted: false
                }),
            ]
        );
    }

    #[test]
    fn pong_has_no_data_field() {
        assert_eq!(
            serde_json::to_value(ServerMessage::Pong).unwrap(),
            serde_json::json!({ "type": "pong" })
        );
    }

    #[test]
    fn events_become_the_matching_messages() {
        let message: ServerMessage = MediaEvent::AppleMusic { running: true }.into();
        assert_eq!(
            serde_json::to_value(message).unwrap(),
            serde_json::json!({ "type": "apple_music", "data": { "running": true } })
        );
    }
}
