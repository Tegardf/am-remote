//! HTTP API: router, auth middleware and the single error type.
//!
//! Every non-2xx response, including axum rejections, the 404/405 fallbacks and panics,
//! goes through [`ApiError`] and has the body `{ "error": { "code", "message" } }`.

pub mod library;
pub mod mvp4;
pub mod pair;
pub mod player;
pub mod volume;
pub mod ws;

use std::any::Any;
use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::Result;
use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::http::{header, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tower_http::catch_panic::CatchPanicLayer;

use crate::config::ConfigStore;
use crate::media::{MediaBackend, MediaError};
use crate::search::{SearchBackend, SearchError};
use pair::PairingState;

pub const API_PREFIX: &str = "/api/v1";

// Error codes from the contract.
pub const CODE_UNAUTHORIZED: &str = "UNAUTHORIZED";
pub const CODE_BAD_PAIRING_CODE: &str = "BAD_PAIRING_CODE";
pub const CODE_APPLE_MUSIC_NOT_RUNNING: &str = "APPLE_MUSIC_NOT_RUNNING";
pub const CODE_NOT_SUPPORTED: &str = "NOT_SUPPORTED";
pub const CODE_INTERNAL: &str = "INTERNAL";
/// Malformed JSON, missing fields, out-of-range values (contract C-001).
pub const CODE_BAD_REQUEST: &str = "BAD_REQUEST";
/// Unknown route, no artwork, unknown playlist or song (contract C-001).
pub const CODE_NOT_FOUND: &str = "NOT_FOUND";
/// Known route, wrong HTTP method (contract C-001).
pub const CODE_METHOD_NOT_ALLOWED: &str = "METHOD_NOT_ALLOWED";
/// Apple Music has a media session but Windows has no audio session for it yet; only
/// `GET`/`POST /volume` answer it (contract C-003).
pub const CODE_NO_AUDIO_SESSION: &str = "NO_AUDIO_SESSION";

/// Wire shape of every error body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

impl ApiError {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }

    pub fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            CODE_UNAUTHORIZED,
            "Missing or invalid token",
        )
    }

    pub fn bad_pairing_code() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            CODE_BAD_PAIRING_CODE,
            "Pairing code is wrong or expired",
        )
    }

    pub fn apple_music_not_running() -> Self {
        Self::new(
            StatusCode::CONFLICT,
            CODE_APPLE_MUSIC_NOT_RUNNING,
            "Apple Music is not running",
        )
    }

    /// `409` for volume calls while Apple Music runs but has no audio session yet.
    pub fn no_audio_session() -> Self {
        Self::new(
            StatusCode::CONFLICT,
            CODE_NO_AUDIO_SESSION,
            MediaError::NoAudioSession.to_string(),
        )
    }

    pub fn not_supported(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_IMPLEMENTED, CODE_NOT_SUPPORTED, message)
    }

    pub fn internal() -> Self {
        Self::internal_with("Unexpected error")
    }

    /// `INTERNAL` with a message the user can act on (for example "no internet").
    pub fn internal_with(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, CODE_INTERNAL, message)
    }

    /// `404` for something that does not exist, with a message saying what.
    pub fn not_found_with(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, CODE_NOT_FOUND, message)
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, CODE_BAD_REQUEST, message)
    }

    pub fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, CODE_NOT_FOUND, "No such endpoint")
    }

    /// `GET /player/artwork` when the current track has no cover.
    pub fn no_artwork() -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            CODE_NOT_FOUND,
            "No artwork for the current track",
        )
    }

    pub fn method_not_allowed() -> Self {
        Self::new(
            StatusCode::METHOD_NOT_ALLOWED,
            CODE_METHOD_NOT_ALLOWED,
            "Method not allowed for this endpoint",
        )
    }
}

impl From<MediaError> for ApiError {
    fn from(error: MediaError) -> Self {
        match error {
            MediaError::NotRunning => Self::apple_music_not_running(),
            // Contract C-003: a media session without an audio session is its own code, so a
            // client can keep the playback buttons and disable only the volume slider.
            MediaError::NoAudioSession => Self::no_audio_session(),
            MediaError::NotSupported(message) => Self::not_supported(message),
            MediaError::NotFound(message) => Self::not_found_with(message),
            MediaError::Internal(message) => {
                tracing::error!("media backend failed: {message}");
                Self::internal()
            }
        }
    }
}

impl From<SearchError> for ApiError {
    fn from(error: SearchError) -> Self {
        // The details (a host name, a timeout) are for the log; the client gets something it can act on.
        tracing::warn!("search failed: {error}");
        match error {
            SearchError::Network(_) => Self::internal_with(
                "Could not reach the Apple search service. Check the laptop's internet connection.",
            ),
            SearchError::BadResponse(_) => {
                Self::internal_with("The Apple search service answered something unexpected.")
            }
        }
    }
}

/// `Query<T>` whose rejection is an [`ApiError`] instead of axum's plain-text body.
pub struct ApiQuery<T>(pub T);

impl<S, T> FromRequestParts<S> for ApiQuery<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match axum::extract::Query::<T>::from_request_parts(parts, state).await {
            Ok(axum::extract::Query(value)) => Ok(Self(value)),
            Err(rejection) => Err(ApiError::bad_request(rejection.body_text())),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ErrorBody {
            error: ErrorDetail {
                code: self.code.to_owned(),
                message: self.message,
            },
        };
        (self.status, Json(body)).into_response()
    }
}

/// `Json<T>` whose rejection is an [`ApiError`] instead of axum's plain-text body.
pub struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        // Every body of this API is a JSON object. serde would also read a struct from a JSON
        // array (by position), so the shape is checked first.
        match Json::<serde_json::Value>::from_request(req, state).await {
            Ok(Json(value)) if !value.is_object() => {
                Err(ApiError::bad_request("the body must be a JSON object"))
            }
            Ok(Json(value)) => serde_json::from_value::<T>(value)
                .map(Self)
                .map_err(|e| ApiError::bad_request(format!("Invalid request body: {e}"))),
            Err(rejection @ JsonRejection::MissingJsonContentType(_)) => {
                Err(ApiError::bad_request(format!(
                    "{} (send Content-Type: application/json)",
                    rejection.body_text()
                )))
            }
            Err(rejection) => Err(ApiError::bad_request(rejection.body_text())),
        }
    }
}

/// Shared handler state. Cheap to clone.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<ConfigStore>,
    pub media: Arc<dyn MediaBackend>,
    pub search: Arc<dyn SearchBackend>,
    /// How long a WebSocket may stay silent before it is closed (tests shorten it).
    pub ws_idle_timeout: std::time::Duration,
    /// How often a connected WebSocket looks at shuffle and repeat (Apple Music gives no event for
    /// them; tests shorten it).
    pub ws_modes_poll: std::time::Duration,
    pairing: Arc<Mutex<PairingState>>,
}

impl AppState {
    /// Creates the state with a fresh random pairing code.
    pub fn new(
        config: Arc<ConfigStore>,
        media: Arc<dyn MediaBackend>,
        search: Arc<dyn SearchBackend>,
    ) -> Result<Self> {
        let code = pair::generate_code()?;
        Ok(Self {
            config,
            media,
            search,
            ws_idle_timeout: ws::IDLE_TIMEOUT,
            ws_modes_poll: ws::MODES_POLL,
            pairing: Arc::new(Mutex::new(PairingState::new(code))),
        })
    }

    fn pairing(&self) -> MutexGuard<'_, PairingState> {
        // Poisoning only means another thread panicked; the state is still consistent.
        self.pairing.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The pairing code currently shown in the tray.
    pub fn pairing_code(&self) -> String {
        self.pairing().code().to_owned()
    }
}

/// Builds the full router: `/api/v1/...` plus JSON fallbacks, auth and panic catching.
pub fn router(state: AppState) -> Router {
    build_router(state, Router::new())
}

/// `extra` is merged under `/api/v1`; production passes none, tests add routes that fail on purpose.
fn build_router(state: AppState, extra: Router<AppState>) -> Router {
    let api = Router::new()
        .route("/status", get(status))
        .route("/pair", post(pair::pair))
        .route("/player/play", post(player::play))
        .route("/player/pause", post(player::pause))
        .route("/player/toggle", post(player::toggle))
        .route("/player/next", post(player::next))
        .route("/player/previous", post(player::previous))
        .route("/volume", get(volume::get_volume).post(volume::set_volume))
        .route("/player/state", get(player::state))
        .route("/player/artwork", get(player::artwork))
        .route("/player/seek", post(player::seek))
        .route("/ws", get(ws::ws))
        .route("/search", get(library::search))
        .route("/playlists", get(library::playlists))
        .route("/play/playlist", post(library::play_playlist))
        .route("/play/track", post(library::play_track))
        .route("/player/modes", get(mvp4::get_modes).post(mvp4::set_modes))
        .route("/queue", get(mvp4::get_queue))
        .route("/queue/play", post(mvp4::play_queue_row))
        .route("/app-search", get(mvp4::app_search))
        .route("/app-search/play", post(mvp4::app_search_play))
        .merge(extra)
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed);

    Router::new()
        .nest(API_PREFIX, api)
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed)
        // Layers added last run first: panics are caught around auth and the handlers.
        .layer(middleware::from_fn_with_state(state.clone(), auth))
        .layer(CatchPanicLayer::custom(panic_response))
        .with_state(state)
}

async fn not_found() -> ApiError {
    ApiError::not_found()
}

async fn method_not_allowed() -> ApiError {
    ApiError::method_not_allowed()
}

/// Turns a caught panic into the `INTERNAL` error body.
fn panic_response(_panic: Box<dyn Any + Send + 'static>) -> Response {
    // Never log the payload: it could contain request data.
    tracing::error!("a request handler panicked");
    ApiError::internal().into_response()
}

fn is_public(method: &Method, path: &str) -> bool {
    method == Method::POST && path == format!("{API_PREFIX}/pair")
}

fn is_ws_path(path: &str) -> bool {
    path == format!("{API_PREFIX}/ws")
}

/// `Authorization: Bearer <token>`; on `/ws` also `?token=<token>`.
fn extract_token(req: &Request) -> Option<String> {
    if let Some(value) = req.headers().get(header::AUTHORIZATION) {
        let value = value.to_str().ok()?;
        let (scheme, token) = value.split_once(' ')?;
        return scheme
            .eq_ignore_ascii_case("bearer")
            .then(|| token.trim().to_owned());
    }
    if is_ws_path(req.uri().path()) {
        // base64url tokens need no percent-decoding.
        return req
            .uri()
            .query()?
            .split('&')
            .find_map(|pair| pair.strip_prefix("token="))
            .map(str::to_owned);
    }
    None
}

/// The token a request authenticated with, for handlers that must notice its revocation (WebSocket).
/// No `Debug` on purpose: it must never end up in a log line.
#[derive(Clone)]
pub struct AuthToken(pub String);

async fn auth(State(state): State<AppState>, mut req: Request, next: Next) -> Response {
    if is_public(req.method(), req.uri().path()) {
        return next.run(req).await;
    }
    match extract_token(&req) {
        Some(token) if state.config.is_authorized(&token) => {
            req.extensions_mut().insert(AuthToken(token));
            next.run(req).await
        }
        _ => ApiError::unauthorized().into_response(),
    }
}

/// `GET /status` response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StatusResponse {
    pub agent_version: String,
    pub apple_music_running: bool,
}

async fn status(State(state): State<AppState>) -> Json<StatusResponse> {
    Json(StatusResponse {
        agent_version: env!("CARGO_PKG_VERSION").to_owned(),
        apple_music_running: state.media.apple_music_running().await,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PairedDevice;
    use crate::media::FakeBackend;
    use axum::body::{to_bytes, Body};
    use tower::ServiceExt;

    /// Goes through the real `build_router`, so removing the panic layer from it fails this test.
    #[tokio::test]
    async fn a_panicking_handler_becomes_a_json_500() {
        async fn boom() -> String {
            panic!("test panic");
        }
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(ConfigStore::open(dir.path().join("config.json")).unwrap());
        store
            .add_device(PairedDevice {
                name: "test".into(),
                token: "FAKE-TOKEN".into(),
                paired_at: 0,
            })
            .unwrap();
        let state = AppState::new(
            store,
            Arc::new(FakeBackend::new(false)),
            Arc::new(crate::search::FakeSearch::default()),
        )
        .unwrap();
        let app = build_router(state, Router::new().route("/boom", get(boom)));

        let request = Request::builder()
            .uri("/api/v1/boom")
            .header(header::AUTHORIZATION, "Bearer FAKE-TOKEN")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body: ErrorBody = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body.error.code, CODE_INTERNAL);
    }
}
