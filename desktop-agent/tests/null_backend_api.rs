//! When the agent's own media backend failed to start, `main` runs on `NullBackend`: `/status` says
//! Apple Music is not running and media calls fail. Which of `500`, `409` or `501` an endpoint
//! answers is not specified (contract C-005), so these tests pin only what is: every failure is a
//! well-formed JSON error, nothing is reported as success, and the read-only endpoints still work.

mod common;

use std::sync::Arc;

use axum::http::{Method, StatusCode};
use axum::Router;
use serde_json::{json, Value};

use am_remote_agent::api::pair::PairResponse;
use am_remote_agent::api::{self, AppState};
use am_remote_agent::config::ConfigStore;
use am_remote_agent::media::NullBackend;
use am_remote_agent::search::FakeSearch;
use common::{send, Reply};

struct Broken {
    app: Router,
    token: String,
    _dir: tempfile::TempDir,
}

async fn broken_agent() -> Broken {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(ConfigStore::open(dir.path().join("config.json")).unwrap());
    let state = AppState::new(
        store,
        Arc::new(NullBackend::default()),
        Arc::new(FakeSearch::default()),
    )
    .unwrap();
    let app = api::router(state.clone());
    let body = json!({ "code": state.pairing_code(), "device_name": "pc-test" }).to_string();
    let reply = send(&app, Method::POST, "/api/v1/pair", None, Some(&body)).await;
    assert_eq!(reply.status, StatusCode::OK);
    Broken {
        app,
        token: reply.json::<PairResponse>().token,
        _dir: dir,
    }
}

impl Broken {
    async fn call(&self, method: Method, path: &str, body: Option<&str>) -> Reply {
        send(&self.app, method, path, Some(&self.token), body).await
    }
}

#[tokio::test]
async fn status_and_the_empty_state_still_answer() {
    let agent = broken_agent().await;

    let status = agent.call(Method::GET, "/api/v1/status", None).await;
    assert_eq!(status.status, StatusCode::OK);
    let status: Value = status.json();
    assert_eq!(status["apple_music_running"], false);

    let state = agent.call(Method::GET, "/api/v1/player/state", None).await;
    assert_eq!(state.status, StatusCode::OK);
    let state: Value = state.json();
    assert_eq!(state["status"], "stopped");
    assert_eq!(state["title"], "");

    let artwork = agent
        .call(Method::GET, "/api/v1/player/artwork", None)
        .await;
    assert_eq!(artwork.status, StatusCode::NOT_FOUND);
    assert_eq!(artwork.error_code(), "NOT_FOUND");
}

#[tokio::test]
async fn media_calls_fail_with_a_json_error_and_never_succeed() {
    let agent = broken_agent().await;
    let song =
        json!({ "url": "https://music.apple.com/id/album/x/1612163750?i=1612164263" }).to_string();

    let calls: Vec<(Method, &str, Option<&str>)> = vec![
        (Method::POST, "/api/v1/player/play", None),
        (Method::POST, "/api/v1/player/toggle", None),
        (Method::GET, "/api/v1/volume", None),
        (Method::POST, "/api/v1/volume", Some(r#"{"level":10}"#)),
        (
            Method::POST,
            "/api/v1/player/seek",
            Some(r#"{"position_ms":5}"#),
        ),
        (Method::GET, "/api/v1/playlists", None),
        (
            Method::POST,
            "/api/v1/play/playlist",
            Some(r#"{"id":"pl-1"}"#),
        ),
        (Method::POST, "/api/v1/play/track", Some(&song)),
    ];
    for (method, path, body) in calls {
        let reply = agent.call(method.clone(), path, body).await;
        assert!(
            [
                StatusCode::INTERNAL_SERVER_ERROR,
                StatusCode::CONFLICT,
                StatusCode::NOT_IMPLEMENTED
            ]
            .contains(&reply.status),
            "{method} {path} answered {}",
            reply.status
        );
        let code = reply.error_code();
        assert!(
            ["INTERNAL", "APPLE_MUSIC_NOT_RUNNING", "NOT_SUPPORTED"].contains(&code.as_str()),
            "{method} {path}: {code}"
        );
    }
}

/// The failure text is for the log, never for the client.
#[tokio::test]
async fn the_failure_cause_is_not_sent_to_the_client() {
    let agent = broken_agent().await;
    let reply = agent.call(Method::POST, "/api/v1/player/play", None).await;
    assert_eq!(reply.status, StatusCode::INTERNAL_SERVER_ERROR);
    let body = String::from_utf8(reply.body).unwrap();
    assert!(!body.contains("failed to start"), "{body}");
}

#[tokio::test]
async fn the_null_backend_reports_not_running_and_has_no_volume_to_send() {
    use am_remote_agent::media::MediaBackend;
    let backend = NullBackend::default();
    assert!(!backend.apple_music_running().await);
    assert!(
        backend.volume().await.is_err(),
        "no volume, so no `volume` message"
    );
}
