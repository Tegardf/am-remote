//! Shared helpers for the HTTP tests: the real router in memory, with the fake media backend.
//! No sockets, no Apple Music, no tray.

// Each test crate uses a different subset of these helpers.
#![allow(dead_code)]

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use serde_json::json;
use tower::ServiceExt;

use am_remote_agent::api::pair::PairResponse;
use am_remote_agent::api::{self, AppState, ErrorBody};
use am_remote_agent::config::ConfigStore;
use am_remote_agent::media::FakeBackend;
use am_remote_agent::search::FakeSearch;

pub struct Harness {
    pub app: Router,
    pub state: AppState,
    pub media: Arc<FakeBackend>,
    pub search: Arc<FakeSearch>,
    pub config_path: std::path::PathBuf,
    _dir: tempfile::TempDir,
}

pub fn harness() -> Harness {
    harness_with(FakeBackend::new(true), None)
}

/// A harness with a custom fake backend and, optionally, a shorter WebSocket idle timeout.
pub fn harness_with(media: FakeBackend, ws_idle_timeout: Option<std::time::Duration>) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let store = Arc::new(ConfigStore::open(config_path.clone()).unwrap());
    let media = Arc::new(media);
    let search = Arc::new(FakeSearch::default());
    let mut state = AppState::new(store, media.clone(), search.clone()).unwrap();
    if let Some(timeout) = ws_idle_timeout {
        state.ws_idle_timeout = timeout;
    }
    Harness {
        app: api::router(state.clone()),
        state,
        media,
        search,
        config_path,
        _dir: dir,
    }
}

pub struct Reply {
    pub status: StatusCode,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

impl Reply {
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> T {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|e| panic!("body is not the expected JSON ({e}): {:?}", self.body))
    }

    /// Asserts the contract error shape and returns `(code)`.
    pub fn error_code(&self) -> String {
        assert_eq!(
            self.content_type.as_deref(),
            Some("application/json"),
            "errors are JSON, never plain text"
        );
        let body: ErrorBody = self.json();
        assert!(!body.error.message.is_empty());
        body.error.code
    }
}

pub async fn send(
    app: &Router,
    method: Method,
    uri: &str,
    token: Option<&str>,
    body: Option<&str>,
) -> Reply {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let request = match body {
        Some(body) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_owned())),
        None => builder.body(Body::empty()),
    }
    .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .map(|v| v.to_str().unwrap().to_owned());
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec();
    Reply {
        status,
        content_type,
        body,
    }
}

pub async fn pair_with(h: &Harness, code: &str) -> Reply {
    let body = json!({ "code": code, "device_name": "pc-test" }).to_string();
    send(&h.app, Method::POST, "/api/v1/pair", None, Some(&body)).await
}

/// Pairs with the current code and returns the token.
pub async fn paired_token(h: &Harness) -> String {
    let reply = pair_with(h, &h.state.pairing_code()).await;
    assert_eq!(reply.status, StatusCode::OK);
    reply.json::<PairResponse>().token
}

/// Serves the harness's router on a free localhost port (for WebSocket tests) and returns the address.
pub async fn serve(h: &Harness) -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = h.app.clone();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}
