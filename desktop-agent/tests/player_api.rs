//! MVP 2: `/player/state`, `/player/artwork` and `/player/seek` through the real router with the
//! fake `MediaBackend`.

mod common;

use axum::http::{Method, StatusCode};
use serde_json::json;

use am_remote_agent::media::{Artwork, MediaBackend, PlaybackStatus, PlayerState};
use common::*;

fn playing() -> PlayerState {
    PlayerState {
        status: PlaybackStatus::Playing,
        title: "Example Song".into(),
        artist: "Example Artist".into(),
        album: "Example Album".into(),
        duration_ms: 215_000,
        position_ms: 42_000,
        artwork_rev: 7,
        playlist_name: None,
    }
}

fn jpeg_bytes() -> Vec<u8> {
    vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F']
}

async fn get(h: &Harness, token: &str, path: &str) -> Reply {
    send(&h.app, Method::GET, path, Some(token), None).await
}

async fn seek(h: &Harness, token: &str, body: &str) -> Reply {
    send(
        &h.app,
        Method::POST,
        "/api/v1/player/seek",
        Some(token),
        Some(body),
    )
    .await
}

// ---------- state ----------

/// Compares the raw JSON, so a missing, renamed or extra field fails here. `playlist_name` must be
/// present as `null`.
#[tokio::test]
async fn state_is_exactly_the_sample_shape() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_state(playing());

    let reply = get(&h, &token, "/api/v1/player/state").await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(
        reply.json::<serde_json::Value>(),
        json!({
            "status": "playing",
            "title": "Example Song",
            "artist": "Example Artist",
            "album": "Example Album",
            "duration_ms": 215000,
            "position_ms": 42000,
            "artwork_rev": 7,
            "playlist_name": null
        })
    );
}

#[tokio::test]
async fn state_is_the_empty_state_when_apple_music_is_not_running() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_state(playing());
    h.media.set_running(false);

    let reply = get(&h, &token, "/api/v1/player/state").await;
    assert_eq!(
        reply.status,
        StatusCode::OK,
        "never an error: clients poll this"
    );
    let state: PlayerState = reply.json();
    assert_eq!(state.status, PlaybackStatus::Stopped);
    assert_eq!(state.title, "");
    assert_eq!(state.duration_ms, 0);
    assert_eq!(state.position_ms, 0);
    assert_eq!(state.playlist_name, None);
}

#[tokio::test]
async fn state_artwork_and_seek_need_a_token() {
    let h = harness();
    for (method, path) in [
        (Method::GET, "/api/v1/player/state"),
        (Method::GET, "/api/v1/player/artwork"),
        (Method::POST, "/api/v1/player/seek"),
    ] {
        let reply = send(&h.app, method.clone(), path, None, Some("{}")).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{method} {path}");
    }
}

// ---------- artwork ----------

#[tokio::test]
async fn artwork_is_served_as_the_image_it_is() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_artwork(Some(Artwork::from_bytes(jpeg_bytes())));

    let reply = get(&h, &token, "/api/v1/player/artwork").await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.content_type.as_deref(), Some("image/jpeg"));
    assert_eq!(reply.body, jpeg_bytes());
}

#[tokio::test]
async fn no_artwork_is_a_404_with_the_error_body() {
    let h = harness();
    let token = paired_token(&h).await;

    let reply = get(&h, &token, "/api/v1/player/artwork").await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
    assert_eq!(reply.error_code(), "NOT_FOUND");
}

#[tokio::test]
async fn artwork_is_gone_when_apple_music_quits() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_artwork(Some(Artwork::from_bytes(jpeg_bytes())));
    h.media.set_running(false);

    let reply = get(&h, &token, "/api/v1/player/artwork").await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
}

// ---------- seek ----------

#[tokio::test]
async fn seek_is_204_and_reaches_the_backend() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_state(playing());

    let reply = seek(&h, &token, r#"{"position_ms":60000}"#).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
    assert!(reply.body.is_empty());
    assert_eq!(h.media.seeks(), vec![60_000]);
}

#[tokio::test]
async fn seek_accepts_the_ends_of_the_track() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_state(playing());

    assert_eq!(
        seek(&h, &token, r#"{"position_ms":0}"#).await.status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        seek(&h, &token, r#"{"position_ms":215000}"#).await.status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(h.media.seeks(), vec![0, 215_000]);
}

/// Contract C-003: a duration of 0 means unknown, and then any position of 0 or more is accepted.
#[tokio::test]
async fn seek_accepts_any_position_when_the_duration_is_unknown() {
    let h = harness();
    let token = paired_token(&h).await;
    assert_eq!(h.media.state().await.duration_ms, 0);

    let reply = seek(&h, &token, r#"{"position_ms":999999}"#).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
    assert_eq!(h.media.seeks(), vec![999_999]);
    // A negative position is still wrong.
    let negative = seek(&h, &token, r#"{"position_ms":-1}"#).await;
    assert_eq!(negative.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn seek_out_of_range_or_malformed_is_400_and_nothing_is_sent() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_state(playing());

    for body in [
        r#"{"position_ms":215001}"#,
        r#"{"position_ms":-1}"#,
        r#"{"position_ms":99999999999}"#,
        r#"{"position_ms":1.5}"#,
        r#"{"position_ms":"soon"}"#,
        "{}",
        "not json",
    ] {
        let reply = seek(&h, &token, body).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "body {body}");
        assert_eq!(reply.error_code(), "BAD_REQUEST", "body {body}");
    }
    assert!(h.media.seeks().is_empty());
}

#[tokio::test]
async fn seek_is_409_when_apple_music_is_not_running() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_running(false);

    let reply = seek(&h, &token, r#"{"position_ms":1000}"#).await;
    assert_eq!(reply.status, StatusCode::CONFLICT);
    assert_eq!(reply.error_code(), "APPLE_MUSIC_NOT_RUNNING");
}

#[tokio::test]
async fn seek_is_501_not_supported_when_apple_music_refuses() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_state(playing());
    h.media.set_seek_supported(false);

    let reply = seek(&h, &token, r#"{"position_ms":1000}"#).await;
    assert_eq!(reply.status, StatusCode::NOT_IMPLEMENTED);
    assert_eq!(reply.error_code(), "NOT_SUPPORTED");
}
