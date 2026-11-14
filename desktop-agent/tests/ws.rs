//! MVP 2: the WebSocket, with a real server and a real client on localhost (no Apple Music).

mod common;

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::{Error as WsError, Message};
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

use am_remote_agent::media::{FakeBackend, MediaBackend, PlaybackStatus, PlayerState};
use common::*;

type Client = WebSocketStream<MaybeTlsStream<TcpStream>>;

const WAIT: Duration = Duration::from_secs(3);

fn playing(title: &str, artwork_rev: u64) -> PlayerState {
    PlayerState {
        status: PlaybackStatus::Playing,
        title: title.into(),
        artist: "Example Artist".into(),
        album: "Example Album".into(),
        duration_ms: 215_000,
        position_ms: 42_000,
        artwork_rev,
        playlist_name: None,
    }
}

async fn connect(addr: std::net::SocketAddr, token: &str) -> Result<Client, WsError> {
    let url = format!("ws://{addr}/api/v1/ws?token={token}");
    connect_async(url).await.map(|(client, _)| client)
}

/// The next text message as JSON; panics on a timeout, a close or a non-text frame.
async fn next_json(client: &mut Client) -> Value {
    let message = tokio::time::timeout(WAIT, client.next())
        .await
        .expect("no message within 3 s")
        .expect("the socket closed")
        .expect("socket error");
    match message {
        Message::Text(text) => serde_json::from_str(text.as_str()).unwrap(),
        other => panic!("expected a text message, got {other:?}"),
    }
}

/// Reads the messages a client gets on connect when Apple Music runs with an audio session:
/// `state`, `apple_music`, `volume` (contract C-002).
async fn skip_connect_burst(client: &mut Client) {
    for expected in ["state", "apple_music", "volume"] {
        assert_eq!(next_json(client).await["type"], expected);
    }
}

async fn send_text(client: &mut Client, value: Value) {
    client.send(Message::text(value.to_string())).await.unwrap();
}

/// True if the server ends the connection (a close frame, or the stream ending) within 3 s.
async fn is_closed_soon(client: &mut Client) -> bool {
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        match tokio::time::timeout_at(deadline, client.next()).await {
            Err(_) => return false,
            Ok(None) | Ok(Some(Err(_))) | Ok(Some(Ok(Message::Close(_)))) => return true,
            Ok(Some(Ok(_))) => {}
        }
    }
}

#[tokio::test]
async fn state_arrives_first_on_connect() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_state(playing("Example Song", 7));
    let addr = serve(&h).await;

    let mut client = connect(addr, &token).await.unwrap();
    let first = next_json(&mut client).await;
    assert_eq!(first["type"], "state");
    assert_eq!(first["data"]["title"], "Example Song");
    assert_eq!(first["data"]["artwork_rev"], 7);
    assert!(first["data"]["playlist_name"].is_null());
}

#[tokio::test]
async fn connect_sends_state_then_apple_music_then_volume() {
    let h = harness();
    let token = paired_token(&h).await;
    let addr = serve(&h).await;

    let mut client = connect(addr, &token).await.unwrap();
    assert_eq!(next_json(&mut client).await["type"], "state");
    assert_eq!(
        next_json(&mut client).await,
        json!({ "type": "apple_music", "data": { "running": true } })
    );
    assert_eq!(
        next_json(&mut client).await,
        json!({ "type": "volume", "data": { "level": 50, "muted": false } })
    );
    // Nothing else follows: the next message is the answer to our ping.
    send_text(&mut client, json!({ "type": "ping" })).await;
    assert_eq!(next_json(&mut client).await, json!({ "type": "pong" }));
}

#[tokio::test]
async fn state_on_connect_is_the_empty_state_when_apple_music_is_not_running() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_running(false);
    let addr = serve(&h).await;

    let mut client = connect(addr, &token).await.unwrap();
    let first = next_json(&mut client).await;
    assert_eq!(first["type"], "state");
    assert_eq!(first["data"]["status"], "stopped");
    assert_eq!(first["data"]["title"], "");
    // No audio session without Apple Music: `apple_music` follows, and there is no `volume`.
    assert_eq!(
        next_json(&mut client).await,
        json!({ "type": "apple_music", "data": { "running": false } })
    );
    send_text(&mut client, json!({ "type": "ping" })).await;
    assert_eq!(next_json(&mut client).await, json!({ "type": "pong" }));
}

#[tokio::test]
async fn a_backend_change_produces_a_new_state() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_state(playing("First Song", 1));
    let addr = serve(&h).await;
    let mut client = connect(addr, &token).await.unwrap();
    assert_eq!(next_json(&mut client).await["data"]["title"], "First Song");
    next_json(&mut client).await; // apple_music
    next_json(&mut client).await; // volume

    h.media.set_state(playing("Second Song", 2));
    let update = next_json(&mut client).await;
    assert_eq!(update["type"], "state");
    assert_eq!(update["data"]["title"], "Second Song");
    assert_eq!(update["data"]["artwork_rev"], 2);
}

#[tokio::test]
async fn volume_and_running_changes_are_pushed() {
    let h = harness();
    let token = paired_token(&h).await;
    let addr = serve(&h).await;
    let mut client = connect(addr, &token).await.unwrap();
    skip_connect_burst(&mut client).await;

    h.media.set_volume(Some(20), Some(true)).await.unwrap();
    assert_eq!(
        next_json(&mut client).await,
        json!({ "type": "volume", "data": { "level": 20, "muted": true } })
    );

    h.media.set_running(false);
    assert_eq!(
        next_json(&mut client).await,
        json!({ "type": "apple_music", "data": { "running": false } })
    );
    h.media.set_running(true);
    assert_eq!(
        next_json(&mut client).await,
        json!({ "type": "apple_music", "data": { "running": true } })
    );
}

#[tokio::test]
async fn every_client_gets_every_change() {
    let h = harness();
    let token = paired_token(&h).await;
    let addr = serve(&h).await;
    let mut a = connect(addr, &token).await.unwrap();
    let mut b = connect(addr, &token).await.unwrap();
    skip_connect_burst(&mut a).await;
    skip_connect_burst(&mut b).await;

    h.media.set_state(playing("Shared", 3));
    assert_eq!(next_json(&mut a).await["data"]["title"], "Shared");
    assert_eq!(next_json(&mut b).await["data"]["title"], "Shared");
}

#[tokio::test]
async fn ping_gets_pong_and_unknown_messages_are_ignored() {
    let h = harness();
    let token = paired_token(&h).await;
    let addr = serve(&h).await;
    let mut client = connect(addr, &token).await.unwrap();
    skip_connect_burst(&mut client).await;

    send_text(&mut client, json!({ "type": "ping" })).await;
    assert_eq!(next_json(&mut client).await, json!({ "type": "pong" }));

    // Nonsense is ignored (no reply, no disconnect): the next ping is still answered first.
    send_text(&mut client, json!({ "type": "hello" })).await;
    client.send(Message::text("not json")).await.unwrap();
    send_text(&mut client, json!({ "type": "ping" })).await;
    assert_eq!(next_json(&mut client).await, json!({ "type": "pong" }));
}

#[tokio::test]
async fn a_bad_or_missing_token_is_refused_with_the_json_401() {
    let h = harness();
    let addr = serve(&h).await;

    for url in [
        format!("ws://{addr}/api/v1/ws?token=nope"),
        format!("ws://{addr}/api/v1/ws"),
    ] {
        match connect_async(url.as_str()).await {
            Err(WsError::Http(response)) => {
                assert_eq!(response.status(), 401, "{url}");
                let body: Value =
                    serde_json::from_slice(response.body().as_deref().unwrap()).unwrap();
                assert_eq!(body["error"]["code"], "UNAUTHORIZED");
            }
            other => panic!("{url}: expected a 401 refusal, got {:?}", other.map(|_| ())),
        }
    }
}

#[tokio::test]
async fn revoking_the_token_closes_the_socket() {
    let h = harness();
    let token = paired_token(&h).await;
    let addr = serve(&h).await;
    let mut client = connect(addr, &token).await.unwrap();
    skip_connect_burst(&mut client).await;

    assert!(h.state.config.revoke(&token).unwrap());
    assert!(is_closed_soon(&mut client).await, "the socket must close");
    assert!(
        connect(addr, &token).await.is_err(),
        "and the token no longer connects"
    );
}

#[tokio::test]
async fn a_silent_client_is_disconnected_after_the_idle_timeout() {
    let h = harness_with(FakeBackend::new(true), Some(Duration::from_millis(300)));
    let token = paired_token(&h).await;
    let addr = serve(&h).await;
    let mut client = connect(addr, &token).await.unwrap();
    skip_connect_burst(&mut client).await;

    assert!(
        is_closed_soon(&mut client).await,
        "a client that sends nothing is dropped"
    );
}

#[tokio::test]
async fn pings_keep_a_connection_open_past_the_idle_timeout() {
    let h = harness_with(FakeBackend::new(true), Some(Duration::from_millis(400)));
    let token = paired_token(&h).await;
    let addr = serve(&h).await;
    let mut client = connect(addr, &token).await.unwrap();
    skip_connect_burst(&mut client).await;

    // Five pings 200 ms apart: a full second, well past the 400 ms limit.
    for _ in 0..5 {
        tokio::time::sleep(Duration::from_millis(200)).await;
        send_text(&mut client, json!({ "type": "ping" })).await;
        assert_eq!(next_json(&mut client).await, json!({ "type": "pong" }));
    }
}

/// The test runtime has one thread, so the server task cannot run while these events are
/// published: with a buffer of 2 it falls behind for sure.
#[tokio::test]
async fn a_client_that_lags_is_resynchronised_with_state_apple_music_and_volume() {
    let h = harness_with(FakeBackend::with_event_buffer(true, 2), None);
    let token = paired_token(&h).await;
    let addr = serve(&h).await;
    let mut client = connect(addr, &token).await.unwrap();
    skip_connect_burst(&mut client).await;

    for n in 0..10 {
        h.media.set_state(playing(&format!("Song {n}"), n));
    }

    let state = next_json(&mut client).await;
    assert_eq!(state["type"], "state");
    assert_eq!(state["data"]["title"], "Song 9", "the state as it is now");
    assert_eq!(
        next_json(&mut client).await,
        json!({ "type": "apple_music", "data": { "running": true } })
    );
    assert_eq!(
        next_json(&mut client).await,
        json!({ "type": "volume", "data": { "level": 50, "muted": false } })
    );
}

#[tokio::test]
async fn revoking_another_device_keeps_this_socket_open() {
    let h = harness();
    let token = paired_token(&h).await;
    let other = paired_token(&h).await;
    let addr = serve(&h).await;
    let mut client = connect(addr, &token).await.unwrap();
    skip_connect_burst(&mut client).await;

    assert!(h.state.config.revoke(&other).unwrap());
    send_text(&mut client, json!({ "type": "ping" })).await;
    assert_eq!(next_json(&mut client).await, json!({ "type": "pong" }));
}

#[tokio::test]
async fn a_client_that_closes_does_not_disturb_the_others() {
    let h = harness();
    let token = paired_token(&h).await;
    let addr = serve(&h).await;
    let mut leaver = connect(addr, &token).await.unwrap();
    let mut stayer = connect(addr, &token).await.unwrap();
    skip_connect_burst(&mut leaver).await;
    skip_connect_burst(&mut stayer).await;

    leaver.close(None).await.unwrap();
    drop(leaver);
    h.media.set_state(playing("After", 9));
    assert_eq!(next_json(&mut stayer).await["data"]["title"], "After");
}
