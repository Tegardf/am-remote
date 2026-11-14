//! MVP 1: player commands and volume through the real router with the fake `MediaBackend`.

mod common;

use axum::http::{Method, StatusCode};
use serde_json::json;

use am_remote_agent::api::ErrorBody;
use am_remote_agent::media::{MediaBackend, Transport, VolumeState};
use common::*;

const COMMANDS: [(&str, Transport); 5] = [
    ("/api/v1/player/play", Transport::Play),
    ("/api/v1/player/pause", Transport::Pause),
    ("/api/v1/player/toggle", Transport::Toggle),
    ("/api/v1/player/next", Transport::Next),
    ("/api/v1/player/previous", Transport::Previous),
];

async fn post_volume(h: &Harness, token: &str, body: &str) -> Reply {
    send(
        &h.app,
        Method::POST,
        "/api/v1/volume",
        Some(token),
        Some(body),
    )
    .await
}

async fn get_volume(h: &Harness, token: &str) -> Reply {
    send(&h.app, Method::GET, "/api/v1/volume", Some(token), None).await
}

// ---------- player commands ----------

#[tokio::test]
async fn every_command_is_204_with_an_empty_body_and_reaches_the_backend() {
    let h = harness();
    let token = paired_token(&h).await;

    for (path, _) in COMMANDS {
        let reply = send(&h.app, Method::POST, path, Some(&token), None).await;
        assert_eq!(reply.status, StatusCode::NO_CONTENT, "{path}");
        assert!(reply.body.is_empty(), "{path} has no body");
    }
    let expected: Vec<Transport> = COMMANDS.iter().map(|(_, t)| *t).collect();
    assert_eq!(h.media.commands(), expected);
}

#[tokio::test]
async fn commands_are_409_when_apple_music_is_not_running() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_running(false);

    for (path, _) in COMMANDS {
        let reply = send(&h.app, Method::POST, path, Some(&token), None).await;
        assert_eq!(reply.status, StatusCode::CONFLICT, "{path}");
        assert_eq!(reply.error_code(), "APPLE_MUSIC_NOT_RUNNING", "{path}");
    }
    assert!(
        h.media.commands().is_empty(),
        "nothing was sent to the backend"
    );
}

#[tokio::test]
async fn commands_need_a_token_and_the_post_method() {
    let h = harness();
    let token = paired_token(&h).await;

    for (path, _) in COMMANDS {
        let unauthorized = send(&h.app, Method::POST, path, None, None).await;
        assert_eq!(unauthorized.status, StatusCode::UNAUTHORIZED, "{path}");
        assert_eq!(unauthorized.error_code(), "UNAUTHORIZED");

        let wrong_method = send(&h.app, Method::GET, path, Some(&token), None).await;
        assert_eq!(
            wrong_method.status,
            StatusCode::METHOD_NOT_ALLOWED,
            "{path}"
        );
    }
    assert!(h.media.commands().is_empty());
}

#[tokio::test]
async fn commands_still_work_before_the_first_playback_audio_session() {
    // Transport goes through SMTC; only volume needs the Core Audio session.
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_audio_session(false);

    let reply = send(
        &h.app,
        Method::POST,
        "/api/v1/player/toggle",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
}

// ---------- volume ----------

#[tokio::test]
async fn get_volume_returns_level_and_muted() {
    let h = harness();
    let token = paired_token(&h).await;

    let reply = get_volume(&h, &token).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(
        reply.json::<VolumeState>(),
        VolumeState {
            level: 50,
            muted: false
        }
    );
}

#[tokio::test]
async fn post_level_changes_only_the_level_and_returns_the_new_state() {
    let h = harness();
    let token = paired_token(&h).await;
    post_volume(&h, &token, r#"{"muted":true}"#).await;

    let reply = post_volume(&h, &token, r#"{"level":20}"#).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(
        reply.json::<VolumeState>(),
        VolumeState {
            level: 20,
            muted: true
        }
    );
    assert_eq!(get_volume(&h, &token).await.json::<VolumeState>().level, 20);
}

#[tokio::test]
async fn post_muted_only_leaves_the_level_unchanged() {
    let h = harness();
    let token = paired_token(&h).await;
    post_volume(&h, &token, r#"{"level":73}"#).await;

    let muted = post_volume(&h, &token, r#"{"muted":true}"#).await;
    assert_eq!(
        muted.json::<VolumeState>(),
        VolumeState {
            level: 73,
            muted: true
        }
    );
    let unmuted = post_volume(&h, &token, r#"{"muted":false}"#).await;
    assert_eq!(
        unmuted.json::<VolumeState>(),
        VolumeState {
            level: 73,
            muted: false
        }
    );
}

#[tokio::test]
async fn post_both_fields_and_the_range_ends() {
    let h = harness();
    let token = paired_token(&h).await;

    let both = post_volume(&h, &token, r#"{"level":55,"muted":false}"#).await;
    assert_eq!(
        both.json::<VolumeState>(),
        VolumeState {
            level: 55,
            muted: false
        }
    );
    let zero = post_volume(&h, &token, r#"{"level":0}"#).await;
    assert_eq!(zero.json::<VolumeState>().level, 0);
    let full = post_volume(&h, &token, r#"{"level":100}"#).await;
    assert_eq!(full.json::<VolumeState>().level, 100);
}

#[tokio::test]
async fn post_with_no_fields_changes_nothing() {
    let h = harness();
    let token = paired_token(&h).await;
    let reply = post_volume(&h, &token, "{}").await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(
        reply.json::<VolumeState>(),
        VolumeState {
            level: 50,
            muted: false
        }
    );
}

#[tokio::test]
async fn out_of_range_or_malformed_levels_are_400_and_change_nothing() {
    let h = harness();
    let token = paired_token(&h).await;

    for body in [
        r#"{"level":101}"#,
        r#"{"level":-1}"#,
        r#"{"level":300}"#,
        r#"{"level":50.5}"#,
        r#"{"level":"loud"}"#,
        r#"{"level":null,"muted":"yes"}"#,
        "not json",
    ] {
        let reply = post_volume(&h, &token, body).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "body {body}");
        assert_eq!(reply.error_code(), "BAD_REQUEST", "body {body}");
    }
    assert_eq!(get_volume(&h, &token).await.json::<VolumeState>().level, 50);
}

#[tokio::test]
async fn volume_is_409_when_apple_music_is_not_running() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_running(false);

    let read = get_volume(&h, &token).await;
    assert_eq!(read.status, StatusCode::CONFLICT);
    assert_eq!(read.error_code(), "APPLE_MUSIC_NOT_RUNNING");
    let write = post_volume(&h, &token, r#"{"level":10}"#).await;
    assert_eq!(write.status, StatusCode::CONFLICT);
    assert_eq!(write.error_code(), "APPLE_MUSIC_NOT_RUNNING");
}

/// Contract C-004, order of checks: an invalid volume or seek body is `400` even when Apple Music
/// is closed; a valid one is the `409`. (The seek upper limit needs the duration, so it comes after.)
#[tokio::test]
async fn invalid_volume_and_seek_bodies_are_400_even_when_apple_music_is_closed() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_running(false);

    for body in [r#"{"level":101}"#, r#"{"level":-1}"#, r#"{"muted":"yes"}"#] {
        let reply = post_volume(&h, &token, body).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(reply.error_code(), "BAD_REQUEST", "{body}");
    }
    let seek = |body: &'static str| {
        send(
            &h.app,
            Method::POST,
            "/api/v1/player/seek",
            Some(&token),
            Some(body),
        )
    };
    for body in [
        r#"{"position_ms":-5}"#,
        r#"{"position_ms":"soon"}"#,
        r#"{}"#,
    ] {
        let reply = seek(body).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{body}");
    }
    assert_eq!(
        post_volume(&h, &token, r#"{"level":10}"#).await.status,
        StatusCode::CONFLICT
    );
    assert_eq!(
        seek(r#"{"position_ms":1000}"#).await.status,
        StatusCode::CONFLICT
    );
}

/// Contract C-003: a media session without an audio session yet is `409 NO_AUDIO_SESSION`, not
/// `APPLE_MUSIC_NOT_RUNNING`, so a client keeps the playback buttons and disables only the slider.
#[tokio::test]
async fn volume_without_an_audio_session_is_409_no_audio_session() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_audio_session(false);

    let read = get_volume(&h, &token).await;
    assert_eq!(read.status, StatusCode::CONFLICT);
    assert_eq!(read.error_code(), "NO_AUDIO_SESSION");
    let body: ErrorBody = read.json();
    assert_eq!(body.error.message, "Apple Music has no audio session yet");
    let write = post_volume(&h, &token, r#"{"muted":true}"#).await;
    assert_eq!(write.status, StatusCode::CONFLICT);
    assert_eq!(write.error_code(), "NO_AUDIO_SESSION");
}

/// ... and playback commands still work in that state.
#[tokio::test]
async fn transport_still_works_without_an_audio_session() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_audio_session(false);

    let reply = send(
        &h.app,
        Method::POST,
        "/api/v1/player/pause",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn volume_needs_a_token() {
    let h = harness();
    let get = send(&h.app, Method::GET, "/api/v1/volume", None, None).await;
    assert_eq!(get.status, StatusCode::UNAUTHORIZED);
    let body = json!({ "level": 10 }).to_string();
    let post = send(&h.app, Method::POST, "/api/v1/volume", None, Some(&body)).await;
    assert_eq!(post.status, StatusCode::UNAUTHORIZED);
}

// ---------- error mapping and exact response shapes ----------

#[tokio::test]
async fn a_backend_failure_is_a_json_500_that_hides_the_details() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_failing(true);

    let replies = [
        send(
            &h.app,
            Method::POST,
            "/api/v1/player/play",
            Some(&token),
            None,
        )
        .await,
        get_volume(&h, &token).await,
        post_volume(&h, &token, r#"{"level":10}"#).await,
    ];
    for reply in replies {
        assert_eq!(reply.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(reply.error_code(), "INTERNAL");
        let text = String::from_utf8(reply.body).unwrap();
        assert!(
            !text.contains("simulated"),
            "internal details stay in the log: {text}"
        );
    }
}

/// Explicit `null` counts as "not given" (logged as a contract request).
#[tokio::test]
async fn null_level_is_treated_as_omitted() {
    let h = harness();
    let token = paired_token(&h).await;
    let reply = post_volume(&h, &token, r#"{"level":null}"#).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(
        reply.json::<VolumeState>(),
        VolumeState {
            level: 50,
            muted: false
        }
    );
}

/// Same for `muted`: `null` is "not given", so the call just returns the current state.
#[tokio::test]
async fn null_muted_is_treated_as_omitted() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_volume(Some(30), Some(true)).await.unwrap();

    for body in [r#"{"muted":null}"#, r#"{"level":null,"muted":null}"#] {
        let reply = post_volume(&h, &token, body).await;
        assert_eq!(reply.status, StatusCode::OK, "{body}");
        assert_eq!(
            reply.json::<VolumeState>(),
            VolumeState {
                level: 30,
                muted: true
            },
            "{body}: nothing changed"
        );
    }
}

/// Compares the raw JSON, so an accidental extra field in a response fails here.
#[tokio::test]
async fn responses_have_exactly_the_sample_shapes() {
    let h = harness();
    let token = paired_token(&h).await;

    let volume = get_volume(&h, &token).await;
    assert_eq!(
        volume.json::<serde_json::Value>(),
        json!({ "level": 50, "muted": false })
    );
    let changed = post_volume(&h, &token, r#"{"level":30,"muted":true}"#).await;
    assert_eq!(
        changed.json::<serde_json::Value>(),
        json!({ "level": 30, "muted": true })
    );

    let status = send(&h.app, Method::GET, "/api/v1/status", Some(&token), None).await;
    assert_eq!(
        status.json::<serde_json::Value>(),
        json!({ "agent_version": env!("CARGO_PKG_VERSION"), "apple_music_running": true })
    );
}
