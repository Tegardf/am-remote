//! Round-trips the shared samples in `../contract/samples.md` through the real wire types.
//!
//! Format of the file: each `### <key>` heading is followed by one fenced `json` block.
//! When a type is added to the agent, add its key here.

use std::collections::BTreeMap;

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

use am_remote_agent::api::library::{PlayPlaylistRequest, PlayTrackRequest};
use am_remote_agent::api::pair::{PairRequest, PairResponse};
use am_remote_agent::api::player::SeekRequest;
use am_remote_agent::api::volume::VolumeRequest;
use am_remote_agent::api::ws::{ClientMessage, ServerMessage};
use am_remote_agent::api::{ApiError, ErrorBody, StatusResponse};
use am_remote_agent::media::{PlayerState, Playlist, VolumeState, UIA_UNAVAILABLE};
use am_remote_agent::search::SearchResult;

const SAMPLES_MD: &str = include_str!("../../contract/samples.md");

fn samples() -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    for section in SAMPLES_MD.split("\n### ").skip(1) {
        let key = section.lines().next().unwrap_or_default().trim().to_owned();
        let json = section
            .split("```json")
            .nth(1)
            .and_then(|rest| rest.split("```").next())
            .unwrap_or_else(|| panic!("sample `{key}` has no json block"));
        let value: Value = serde_json::from_str(json)
            .unwrap_or_else(|e| panic!("sample `{key}` is not valid JSON: {e}"));
        assert!(
            out.insert(key.clone(), value).is_none(),
            "duplicate sample `{key}`"
        );
    }
    out
}

fn sample(key: &str) -> Value {
    samples()
        .remove(key)
        .unwrap_or_else(|| panic!("sample `{key}` is missing from contract/samples.md"))
}

/// Deserializes the sample into `T`, serializes it back and expects the identical JSON.
fn round_trip<T: Serialize + DeserializeOwned>(key: &str) {
    let original = sample(key);
    let typed: T = serde_json::from_value(original.clone())
        .unwrap_or_else(|e| panic!("sample `{key}` does not fit its type: {e}"));
    let again = serde_json::to_value(&typed).expect("serializes");
    assert_eq!(again, original, "sample `{key}` changed in the round trip");
}

#[test]
fn samples_file_is_parsed() {
    assert!(samples().len() >= 20, "expected the full sample set");
}

/// Samples of shapes the agent does not model yet (none: every shape of the contract is implemented).
const NOT_YET_MODELLED: [&str; 0] = [];

/// Every sample key the tests below round-trip. A new sample in `contract/samples.md` that is in
/// neither list fails `every_sample_is_covered`, so a new shape cannot be forgotten.
const COVERED: [&str; 28] = [
    "status_response",
    "pair_request",
    "pair_response",
    "volume_response",
    "volume_request_level",
    "volume_request_muted",
    "volume_request_both",
    "player_state_playing",
    "player_state_paused_in_playlist",
    "seek_request",
    "playlists_response",
    "play_playlist_request",
    "search_response",
    "play_track_request",
    "ws_state",
    "ws_volume",
    "ws_apple_music",
    "ws_ping",
    "ws_pong",
    "error_unauthorized",
    "error_bad_pairing_code",
    "error_apple_music_not_running",
    "error_not_supported",
    "error_internal",
    "error_bad_request",
    "error_not_found",
    "error_method_not_allowed",
    "error_no_audio_session",
];

#[test]
fn every_sample_is_covered() {
    for key in samples().keys() {
        assert!(
            COVERED.contains(&key.as_str()) || NOT_YET_MODELLED.contains(&key.as_str()),
            "sample `{key}` is not round-tripped by any test: add it to COVERED or NOT_YET_MODELLED"
        );
    }
    for key in COVERED {
        assert!(
            samples().contains_key(key),
            "COVERED lists `{key}`, which is not in contract/samples.md"
        );
    }
}

#[test]
fn status_and_pairing_samples() {
    round_trip::<StatusResponse>("status_response");
    round_trip::<PairRequest>("pair_request");
    round_trip::<PairResponse>("pair_response");
}

#[test]
fn volume_samples() {
    round_trip::<VolumeState>("volume_response");
    for key in [
        "volume_request_level",
        "volume_request_muted",
        "volume_request_both",
    ] {
        round_trip::<VolumeRequest>(key);
    }
}

#[test]
fn player_samples() {
    round_trip::<PlayerState>("player_state_playing");
    round_trip::<PlayerState>("player_state_paused_in_playlist");
    round_trip::<SeekRequest>("seek_request");
}

#[test]
fn library_samples() {
    round_trip::<Vec<Playlist>>("playlists_response");
    round_trip::<PlayPlaylistRequest>("play_playlist_request");
    round_trip::<Vec<SearchResult>>("search_response");
    round_trip::<PlayTrackRequest>("play_track_request");
}

#[test]
fn websocket_samples() {
    for key in ["ws_state", "ws_volume", "ws_apple_music", "ws_pong"] {
        round_trip::<ServerMessage>(key);
    }
    round_trip::<ClientMessage>("ws_ping");
}

#[test]
fn error_samples() {
    for key in [
        "error_unauthorized",
        "error_bad_pairing_code",
        "error_apple_music_not_running",
        "error_not_supported",
        "error_internal",
        "error_bad_request",
        "error_not_found",
        "error_method_not_allowed",
        "error_no_audio_session",
    ] {
        round_trip::<ErrorBody>(key);
    }
}

/// The messages the agent really sends are the ones in the samples.
#[test]
fn api_errors_produce_the_sample_bodies() {
    fn body(error: &ApiError) -> Value {
        serde_json::json!({ "error": { "code": error.code, "message": error.message } })
    }
    assert_eq!(
        body(&ApiError::unauthorized()),
        sample("error_unauthorized")
    );
    assert_eq!(
        body(&ApiError::bad_pairing_code()),
        sample("error_bad_pairing_code")
    );
    assert_eq!(
        body(&ApiError::apple_music_not_running()),
        sample("error_apple_music_not_running")
    );
    // The message is human text (clients switch on `code`), so the real agent wording
    // (`UIA_UNAVAILABLE`) may differ from the sample's; the shape and the code must match.
    let not_supported = body(&ApiError::not_supported(UIA_UNAVAILABLE));
    let sample_not_supported = sample("error_not_supported");
    assert_eq!(
        not_supported["error"]["code"],
        sample_not_supported["error"]["code"]
    );
    assert_eq!(not_supported["error"]["message"], UIA_UNAVAILABLE);
    assert!(sample_not_supported["error"]["message"].is_string());
    assert_eq!(body(&ApiError::internal()), sample("error_internal"));

    // Codes added by C-001. Their messages are human text and differ from the samples (the agent
    // says what is missing, e.g. "No such endpoint"); the code, status and shape must match.
    for (error, key, status) in [
        (
            ApiError::bad_request("Request body is not valid JSON"),
            "error_bad_request",
            400,
        ),
        (ApiError::not_found(), "error_not_found", 404),
        (
            ApiError::method_not_allowed(),
            "error_method_not_allowed",
            405,
        ),
        // C-003.
        (ApiError::no_audio_session(), "error_no_audio_session", 409),
    ] {
        let sample = sample(key);
        assert_eq!(
            body(&error)["error"]["code"],
            sample["error"]["code"],
            "{key}"
        );
        assert!(sample["error"]["message"].is_string(), "{key}");
        assert_eq!(error.status.as_u16(), status, "{key}");
    }
    // The contract fixes this message too (the agent's own `MediaError` text).
    assert_eq!(
        body(&ApiError::no_audio_session()),
        sample("error_no_audio_session")
    );
}
