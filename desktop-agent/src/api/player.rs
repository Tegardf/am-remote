//! Player endpoints: `POST /player/play|pause|toggle|next|previous` (204),
//! `GET /player/state`, `GET /player/artwork` and `POST /player/seek`.

use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

use super::{ApiError, ApiJson, AppState};
use crate::media::{PlayerState, Transport};

async fn send(state: AppState, command: Transport) -> Result<StatusCode, ApiError> {
    state.media.transport(command).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn play(State(state): State<AppState>) -> Result<StatusCode, ApiError> {
    send(state, Transport::Play).await
}

pub async fn pause(State(state): State<AppState>) -> Result<StatusCode, ApiError> {
    send(state, Transport::Pause).await
}

pub async fn toggle(State(state): State<AppState>) -> Result<StatusCode, ApiError> {
    send(state, Transport::Toggle).await
}

pub async fn next(State(state): State<AppState>) -> Result<StatusCode, ApiError> {
    send(state, Transport::Next).await
}

pub async fn previous(State(state): State<AppState>) -> Result<StatusCode, ApiError> {
    send(state, Transport::Previous).await
}

/// `GET /player/state`. Always `200`: when Apple Music is not running or nothing is loaded it is the
/// empty state (open question #2, logged in `docs/contract-requests.md`).
pub async fn state(State(state): State<AppState>) -> Json<PlayerState> {
    Json(state.media.state().await)
}

/// `GET /player/artwork`: the cover with its own content type (`image/jpeg`, `image/png`, ...),
/// or `404` with the error body if there is none.
pub async fn artwork(State(state): State<AppState>) -> Result<Response, ApiError> {
    let artwork = state
        .media
        .artwork()
        .await
        .ok_or_else(ApiError::no_artwork)?;
    Ok((
        [(header::CONTENT_TYPE, artwork.content_type)],
        artwork.bytes.as_ref().clone(),
    )
        .into_response())
}

/// `POST /player/seek` body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SeekRequest {
    /// Read as a signed integer so a negative value gets a clear `400` message.
    pub position_ms: i64,
}

/// `POST /player/seek { position_ms }` -> `204`.
pub async fn seek(
    State(state): State<AppState>,
    ApiJson(request): ApiJson<SeekRequest>,
) -> Result<StatusCode, ApiError> {
    let position = u64::try_from(request.position_ms)
        .map_err(|_| ApiError::bad_request("position_ms must not be negative"))?;
    if !state.media.apple_music_running().await {
        return Err(ApiError::apple_music_not_running());
    }
    let duration = state.media.state().await.duration_ms;
    if duration > 0 && position > duration {
        return Err(ApiError::bad_request(
            "position_ms is past the end of the track",
        ));
    }
    state.media.seek(position).await?;
    Ok(StatusCode::NO_CONTENT)
}
