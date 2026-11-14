//! MVP 3: `GET /search`, `GET /playlists`, `POST /play/playlist` and `POST /play/track`.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use super::{ApiError, ApiJson, ApiQuery, AppState};
use crate::media::links::parse_apple_music_url;
use crate::media::{PlayTrack, Playlist};
use crate::search::SearchResult;

/// `limit` when the client sends none.
pub const DEFAULT_LIMIT: u8 = 25;
/// The most results one search returns.
pub const MAX_LIMIT: u8 = 50;
/// A query shorter than this is a validation error.
pub const MIN_QUERY_CHARS: usize = 2;
/// A longer query is certainly a mistake.
pub const MAX_QUERY_CHARS: usize = 200;

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    q: Option<String>,
    /// Read as a wide integer so that `0` or `500` get a clear message instead of a parse error.
    limit: Option<i64>,
}

/// `GET /search?q=&limit=25`
pub async fn search(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<SearchParams>,
) -> Result<Json<Vec<SearchResult>>, ApiError> {
    let query = params.q.as_deref().map(str::trim).unwrap_or_default();
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
    let limit = match params.limit {
        None => DEFAULT_LIMIT,
        Some(limit) => u8::try_from(limit)
            .ok()
            .filter(|limit| (1..=MAX_LIMIT).contains(limit))
            .ok_or_else(|| {
                ApiError::bad_request(format!("limit must be between 1 and {MAX_LIMIT}"))
            })?,
    };
    Ok(Json(state.search.search(query, limit).await?))
}

/// `GET /playlists`: the playlists of the user's library.
pub async fn playlists(State(state): State<AppState>) -> Result<Json<Vec<Playlist>>, ApiError> {
    Ok(Json(state.media.playlists().await?))
}

/// `POST /play/playlist` body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PlayPlaylistRequest {
    pub id: String,
}

/// `POST /play/playlist { id }` -> `204`.
pub async fn play_playlist(
    State(state): State<AppState>,
    ApiJson(request): ApiJson<PlayPlaylistRequest>,
) -> Result<StatusCode, ApiError> {
    if request.id.trim().is_empty() {
        return Err(ApiError::bad_request("id must not be empty"));
    }
    state.media.play_playlist(&request.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /play/track` body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PlayTrackRequest {
    pub url: String,
}

/// `POST /play/track { url }` -> `204`. The url is a song link from `/search`.
pub async fn play_track(
    State(state): State<AppState>,
    ApiJson(request): ApiJson<PlayTrackRequest>,
) -> Result<StatusCode, ApiError> {
    let link =
        parse_apple_music_url(&request.url).map_err(|e| ApiError::bad_request(e.to_string()))?;
    let track_id = link.track_id.as_deref().ok_or_else(|| {
        ApiError::bad_request(
            "url must be a link to a song (it needs ?i=<song id>), as /search returns",
        )
    })?;

    // Without UI Automation nothing can be played: say so before any network call. (Whether Apple
    // Music's window exists is the UI driver's business: Apple Music can be open without a media
    // session, before anything has played, and then it must still work.)
    if !state.media.ui_supported() {
        return Err(ApiError::not_supported(crate::media::UIA_UNAVAILABLE));
    }
    // The link has no title, so ask the catalog what this song is called.
    let info = state
        .search
        .lookup_track(track_id, link.storefront.as_deref())
        .await?
        .ok_or_else(|| ApiError::not_found_with("The Apple catalog does not know this song"))?;

    state
        .media
        .play_track(PlayTrack {
            open_url: link.open_url,
            title: info.title,
            artist: info.artist,
            album: info.album,
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
