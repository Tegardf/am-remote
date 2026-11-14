//! `GET /volume` and `POST /volume`: Apple Music's own volume, 0-100.

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};

use super::{ApiError, ApiJson, AppState};
use crate::media::VolumeState;

/// `POST /volume` body: either field may be left out; a missing field is left unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct VolumeRequest {
    /// Read as a wide integer so that 150 or -5 reach our range check (a clear 400 message)
    /// instead of a generic deserialization error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
}

impl VolumeRequest {
    /// The level as 0-100, or a `BAD_REQUEST` if it is outside that range.
    fn checked_level(&self) -> Result<Option<u8>, ApiError> {
        match self.level {
            None => Ok(None),
            Some(level) => u8::try_from(level)
                .ok()
                .filter(|level| *level <= 100)
                .map(Some)
                .ok_or_else(|| ApiError::bad_request("level must be between 0 and 100")),
        }
    }
}

pub async fn get_volume(State(state): State<AppState>) -> Result<Json<VolumeState>, ApiError> {
    Ok(Json(state.media.volume().await?))
}

pub async fn set_volume(
    State(state): State<AppState>,
    ApiJson(request): ApiJson<VolumeRequest>,
) -> Result<Json<VolumeState>, ApiError> {
    let level = request.checked_level()?;
    Ok(Json(state.media.set_volume(level, request.muted).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(level: Option<i64>) -> VolumeRequest {
        VolumeRequest { level, muted: None }
    }

    #[test]
    fn levels_in_range_are_accepted() {
        assert_eq!(request(None).checked_level().unwrap(), None);
        assert_eq!(request(Some(0)).checked_level().unwrap(), Some(0));
        assert_eq!(request(Some(100)).checked_level().unwrap(), Some(100));
    }

    #[test]
    fn levels_out_of_range_are_rejected() {
        for level in [-1, 101, 255, 256, 1_000_000, i64::MIN, i64::MAX] {
            let error = request(Some(level)).checked_level().unwrap_err();
            assert_eq!(error.code, super::super::CODE_BAD_REQUEST, "level {level}");
        }
    }
}
