//! MVP 4 (contract C-006): shuffle and repeat, the Up Next queue, and search through Apple Music's
//! own search box. Order of checks everywhere: auth (middleware) -> `400` input -> `501` no UI
//! Automation -> `409`/`404` from the backend.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use super::library::{MAX_QUERY_CHARS, MIN_QUERY_CHARS};
use super::{ApiError, ApiJson, ApiQuery, AppState};
use crate::media::{AppSearchResult, Modes, QueueItem, Repeat, Scope, UIA_UNAVAILABLE};

/// Everything here drives Apple Music's window: without UI Automation it is `501`, before any
/// other work.
fn ensure_ui(state: &AppState) -> Result<(), ApiError> {
    if state.media.ui_supported() {
        Ok(())
    } else {
        Err(ApiError::not_supported(UIA_UNAVAILABLE))
    }
}

/// `GET /player/modes`
pub async fn get_modes(State(state): State<AppState>) -> Result<Json<Modes>, ApiError> {
    ensure_ui(&state)?;
    Ok(Json(state.media.modes().await?))
}

/// `POST /player/modes` body. Both fields are optional; an explicit `null` counts as not given.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", try_from = "serde_json::Value")]
pub struct ModesRequest {
    pub shuffle: Option<bool>,
    pub repeat: Option<Repeat>,
}

/// The fields as they are read; split out so that [`ModesRequest`] can first insist on an object
/// (serde would otherwise also read a struct from a JSON array, and `[]` is not a valid body).
#[derive(Deserialize)]
struct ModesFields {
    #[serde(default)]
    shuffle: Option<bool>,
    #[serde(default)]
    repeat: Option<Repeat>,
}

impl TryFrom<serde_json::Value> for ModesRequest {
    type Error = String;

    fn try_from(value: serde_json::Value) -> Result<Self, String> {
        if !value.is_object() {
            return Err("the body must be a JSON object".into());
        }
        let fields: ModesFields = serde_json::from_value(value).map_err(|e| e.to_string())?;
        Ok(Self {
            shuffle: fields.shuffle,
            repeat: fields.repeat,
        })
    }
}

/// `POST /player/modes { shuffle?, repeat? }` -> the state read back from Apple Music.
pub async fn set_modes(
    State(state): State<AppState>,
    ApiJson(request): ApiJson<ModesRequest>,
) -> Result<Json<Modes>, ApiError> {
    ensure_ui(&state)?;
    if request.shuffle.is_none() && request.repeat.is_none() {
        return Ok(Json(state.media.modes().await?));
    }
    Ok(Json(
        state
            .media
            .set_modes(request.shuffle, request.repeat)
            .await?,
    ))
}

/// `GET /queue` body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueResponse {
    pub items: Vec<QueueItem>,
}

/// `GET /queue`
pub async fn get_queue(State(state): State<AppState>) -> Result<Json<QueueResponse>, ApiError> {
    ensure_ui(&state)?;
    Ok(Json(QueueResponse {
        items: state.media.queue().await?,
    }))
}

/// `POST /queue/play` body. `index` is read as a signed integer so a negative one gets a clear `400`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct QueuePlayRequest {
    pub index: i64,
    pub title: String,
}

/// A non-negative index, or a `400`.
fn checked_index(index: i64) -> Result<usize, ApiError> {
    usize::try_from(index).map_err(|_| ApiError::bad_request("index must be 0 or more"))
}

/// A non-blank text, or a `400` naming the field.
fn checked_text<'a>(text: &'a str, field: &str) -> Result<&'a str, ApiError> {
    let text = text.trim();
    if text.is_empty() {
        Err(ApiError::bad_request(format!("{field} must not be empty")))
    } else {
        Ok(text)
    }
}

/// `q` trimmed, 2-200 characters, or a `400`.
fn checked_query(query: &str) -> Result<&str, ApiError> {
    let query = query.trim();
    let length = query.chars().count();
    if length < MIN_QUERY_CHARS {
        return Err(ApiError::bad_request(format!(
            "q must be at least {MIN_QUERY_CHARS} characters"
        )));
    }
    if length > MAX_QUERY_CHARS {
        return Err(ApiError::bad_request(format!(
            "q must be at most {MAX_QUERY_CHARS} characters"
        )));
    }
    Ok(query)
}

/// `POST /queue/play { index, title }` -> `204` once that song is current.
pub async fn play_queue_row(
    State(state): State<AppState>,
    ApiJson(request): ApiJson<QueuePlayRequest>,
) -> Result<StatusCode, ApiError> {
    let index = checked_index(request.index)?;
    let title = checked_text(&request.title, "title")?;
    ensure_ui(&state)?;
    state.media.queue_play(index, title).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct AppSearchParams {
    q: Option<String>,
    scope: Option<String>,
}

/// `GET /app-search` body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppSearchResponse {
    pub results: Vec<AppSearchResult>,
}

/// `GET /app-search?q=&scope=catalog`
pub async fn app_search(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<AppSearchParams>,
) -> Result<Json<AppSearchResponse>, ApiError> {
    let query = checked_query(params.q.as_deref().unwrap_or_default())?;
    let scope = match params.scope.as_deref() {
        None => Scope::Catalog,
        Some(text) => Scope::parse(text)
            .ok_or_else(|| ApiError::bad_request("scope must be catalog or library"))?,
    };
    ensure_ui(&state)?;
    Ok(Json(AppSearchResponse {
        results: state.media.app_search(query, scope).await?,
    }))
}

/// `POST /app-search/play` body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AppSearchPlayRequest {
    pub q: String,
    pub scope: Scope,
    pub section: String,
    pub index: i64,
    pub title: String,
}

/// `POST /app-search/play` -> `204` after playback started.
pub async fn app_search_play(
    State(state): State<AppState>,
    ApiJson(request): ApiJson<AppSearchPlayRequest>,
) -> Result<StatusCode, ApiError> {
    let query = checked_query(&request.q)?;
    let section = checked_text(&request.section, "section")?;
    let index = checked_index(request.index)?;
    let title = checked_text(&request.title, "title")?;
    ensure_ui(&state)?;
    state
        .media
        .app_search_play(query, request.scope, section, index, title)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
