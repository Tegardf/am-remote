//! Pure player-state logic: wire types, position interpolation, artwork cache and events.
//!
//! Nothing here touches Windows, so it is all unit-tested with fixed values. Windows time units
//! (100 ns ticks, 1601-based dates) are converted to milliseconds at the edge (`smtc.rs`) using the
//! helpers below, and never leave the media module.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::audio::VolumeState;

/// `status` of the contract's `PlayerState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlaybackStatus {
    Playing,
    Paused,
    Stopped,
    Unknown,
}

impl PlaybackStatus {
    /// From SMTC's `GlobalSystemMediaTransportControlsSessionPlaybackStatus` value:
    /// Closed 0, Opened 1, Changing 2, Stopped 3, Playing 4, Paused 5.
    pub fn from_smtc(raw: i32) -> Self {
        match raw {
            4 => Self::Playing,
            5 => Self::Paused,
            0 | 3 => Self::Stopped,
            _ => Self::Unknown,
        }
    }
}

/// The contract's `PlayerState`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PlayerState {
    pub status: PlaybackStatus,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u64,
    pub position_ms: u64,
    /// Increments when the artwork changes: the app re-fetches `/player/artwork`.
    pub artwork_rev: u64,
    /// Always present on the wire; `null` until the UI Automation spike says GO.
    pub playlist_name: Option<String>,
}

impl PlayerState {
    /// What is reported when Apple Music is not running or nothing is loaded (contract C-002,
    /// "PlayerState"): empty strings, zeros, status `stopped`, `playlist_name` null.
    pub fn empty(artwork_rev: u64) -> Self {
        Self {
            status: PlaybackStatus::Stopped,
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            duration_ms: 0,
            position_ms: 0,
            artwork_rev,
            playlist_name: None,
        }
    }
}

/// Something that changed, pushed to every WebSocket client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaEvent {
    State(PlayerState),
    Volume(VolumeState),
    AppleMusic { running: bool },
}

/// 100 ns ticks (WinRT `TimeSpan`) to milliseconds; negative values become 0.
pub fn ticks_to_ms(ticks: i64) -> u64 {
    u64::try_from(ticks.max(0) / 10_000).unwrap_or(0)
}

/// Milliseconds to 100 ns ticks (for `TryChangePlaybackPositionAsync`).
pub fn ms_to_ticks(ms: u64) -> i64 {
    i64::try_from(ms)
        .unwrap_or(i64::MAX / 10_000)
        .saturating_mul(10_000)
}

/// WinRT `DateTime` (100 ns ticks since 1601-01-01 UTC) to Unix milliseconds.
pub fn windows_datetime_to_unix_ms(universal_ticks: i64) -> i64 {
    const MS_BETWEEN_1601_AND_1970: i64 = 11_644_473_600_000;
    universal_ticks / 10_000 - MS_BETWEEN_1601_AND_1970
}

/// Current time as Unix milliseconds.
pub fn unix_now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// When SMTC reported the position, as Unix ms. A timeline that was never updated has a zero
/// timestamp (year 1601), which would make the position race ahead by centuries: treat it as "now".
pub fn reported_at_unix_ms(universal_ticks: i64, now_unix_ms: i64) -> i64 {
    if universal_ticks <= 0 {
        now_unix_ms
    } else {
        windows_datetime_to_unix_ms(universal_ticks)
    }
}

/// SMTC timeline updates are sparse, so while playing the position is the reported one plus the time
/// since it was reported. Clamped to the duration (when known).
pub fn live_position_ms(
    reported_ms: u64,
    reported_at_unix_ms: i64,
    now_unix_ms: i64,
    playing: bool,
    duration_ms: u64,
) -> u64 {
    let elapsed = if playing {
        u64::try_from(now_unix_ms.saturating_sub(reported_at_unix_ms)).unwrap_or(0)
    } else {
        0
    };
    let position = reported_ms.saturating_add(elapsed);
    if duration_ms > 0 {
        position.min(duration_ms)
    } else {
        position
    }
}

/// Apple Music for Windows puts "Artist — Album" in SMTC's artist field and leaves the album empty.
/// This splits them for the contract's separate fields; anything else is passed through.
// verified: `cargo run --example read_player`: artist "Perunggu — Memorandum", album "", 2026-11-14
pub fn split_artist_album(artist: &str, album: &str) -> (String, String) {
    const SEPARATOR: &str = " \u{2014} ";
    if album.trim().is_empty() {
        if let Some((artist, album)) = artist.split_once(SEPARATOR) {
            return (artist.trim().to_owned(), album.trim().to_owned());
        }
    }
    (artist.to_owned(), album.to_owned())
}

/// Raw facts read from the Apple Music SMTC session, before conversion to the wire shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub status: PlaybackStatus,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u64,
    /// Position as last reported by SMTC.
    pub position_ms: u64,
    /// Unix time at which SMTC reported `position_ms`.
    pub position_reported_at_unix_ms: i64,
}

impl Snapshot {
    /// Identifies the track for artwork caching.
    pub fn track_key(&self) -> String {
        format!("{}\u{0}{}\u{0}{}", self.title, self.artist, self.album)
    }

    /// The wire state, with the position brought up to `now_unix_ms`.
    pub fn to_state(&self, artwork_rev: u64, now_unix_ms: i64) -> PlayerState {
        let (artist, album) = split_artist_album(&self.artist, &self.album);
        PlayerState {
            status: self.status,
            title: self.title.clone(),
            artist,
            album,
            duration_ms: self.duration_ms,
            position_ms: live_position_ms(
                self.position_ms,
                self.position_reported_at_unix_ms,
                now_unix_ms,
                self.status == PlaybackStatus::Playing,
                self.duration_ms,
            ),
            artwork_rev,
            playlist_name: None,
        }
    }
}

/// A cover image, ready to serve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artwork {
    pub bytes: Arc<Vec<u8>>,
    pub content_type: &'static str,
}

impl Artwork {
    /// Wraps image bytes, telling JPEG from PNG by the first bytes (SMTC's own content type string
    /// is unreliable: it reported "image/jpeg,image/jpe,image/jpg").
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        let content_type = if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            "image/jpeg"
        } else if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
            "image/png"
        } else {
            "application/octet-stream"
        };
        Self {
            bytes: Arc::new(bytes),
            content_type,
        }
    }
}

/// How often a track without artwork is retried (the thumbnail can arrive a moment after the title).
const MAX_FETCH_ATTEMPTS: u8 = 5;

/// Remembers the current track's artwork and its revision number.
#[derive(Debug, Default)]
pub struct ArtworkCache {
    rev: u64,
    key: Option<String>,
    artwork: Option<Artwork>,
    /// The previous track cover. SMTC can keep serving it for a moment after the title changed,
    /// so a "new" cover identical to it is treated as possibly stale and read again.
    previous: Option<Artwork>,
    attempts: u8,
}

impl ArtworkCache {
    pub fn rev(&self) -> u64 {
        self.rev
    }

    pub fn artwork(&self) -> Option<Artwork> {
        self.artwork.clone()
    }

    /// Whether the thumbnail should be read now: the track changed, or its cover is still missing
    /// or identical to the previous track cover (maybe stale), and we have not tried too often.
    /// Two tracks of one album legitimately share a cover; then this costs a few extra reads once.
    pub fn needs_fetch(&self, key: &str) -> bool {
        if self.key.as_deref() != Some(key) {
            return true;
        }
        let suspicious = self.artwork.is_none() || self.artwork == self.previous;
        suspicious && self.attempts < MAX_FETCH_ATTEMPTS
    }

    /// Stores the result of a fetch. Bumps the revision when the track changed, or when the artwork
    /// of the same track appeared or changed. Returns whether it bumped.
    pub fn update(&mut self, key: &str, image: Option<Vec<u8>>) -> bool {
        let track_changed = self.key.as_deref() != Some(key);
        if track_changed {
            self.attempts = 0;
            self.previous = self.artwork.clone();
        }
        self.attempts = self.attempts.saturating_add(1);
        let new = image.map(Artwork::from_bytes);
        let artwork_changed = new != self.artwork;
        if track_changed || artwork_changed {
            self.rev += 1;
        }
        self.key = Some(key.to_owned());
        self.artwork = new;
        track_changed || artwork_changed
    }

    /// Apple Music went away: forget the artwork (and bump the revision if there was one).
    pub fn clear(&mut self) {
        if self.artwork.is_some() {
            self.rev += 1;
        }
        self.key = None;
        self.artwork = None;
        self.previous = None;
        self.attempts = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_maps_from_smtc_values() {
        assert_eq!(PlaybackStatus::from_smtc(4), PlaybackStatus::Playing);
        assert_eq!(PlaybackStatus::from_smtc(5), PlaybackStatus::Paused);
        assert_eq!(PlaybackStatus::from_smtc(3), PlaybackStatus::Stopped);
        assert_eq!(PlaybackStatus::from_smtc(0), PlaybackStatus::Stopped);
        assert_eq!(PlaybackStatus::from_smtc(1), PlaybackStatus::Unknown);
        assert_eq!(PlaybackStatus::from_smtc(2), PlaybackStatus::Unknown);
        assert_eq!(PlaybackStatus::from_smtc(99), PlaybackStatus::Unknown);
    }

    #[test]
    fn status_serializes_lowercase() {
        for (status, text) in [
            (PlaybackStatus::Playing, "playing"),
            (PlaybackStatus::Paused, "paused"),
            (PlaybackStatus::Stopped, "stopped"),
            (PlaybackStatus::Unknown, "unknown"),
        ] {
            assert_eq!(serde_json::to_value(status).unwrap(), text);
        }
    }

    #[test]
    fn playlist_name_is_always_on_the_wire() {
        let json = serde_json::to_value(PlayerState::empty(3)).unwrap();
        assert!(json.get("playlist_name").is_some_and(|v| v.is_null()));
    }

    #[test]
    fn time_conversions() {
        assert_eq!(ticks_to_ms(10_000), 1);
        assert_eq!(ticks_to_ms(4_340_000_000), 434_000);
        assert_eq!(ticks_to_ms(-5), 0);
        assert_eq!(ms_to_ticks(1), 10_000);
        assert_eq!(ms_to_ticks(60_000), 600_000_000);
        // 1970-01-01 in WinRT ticks.
        assert_eq!(windows_datetime_to_unix_ms(116_444_736_000_000_000), 0);
        assert_eq!(windows_datetime_to_unix_ms(116_444_736_010_000_000), 1_000);
    }

    #[test]
    fn position_advances_while_playing() {
        // Reported 42 s at t=1_000 ms; now t=3_500 ms: 2.5 s later.
        assert_eq!(
            live_position_ms(42_000, 1_000, 3_500, true, 215_000),
            44_500
        );
    }

    #[test]
    fn position_is_frozen_when_not_playing() {
        assert_eq!(
            live_position_ms(42_000, 1_000, 9_000, false, 215_000),
            42_000
        );
    }

    #[test]
    fn position_is_clamped_to_the_duration() {
        assert_eq!(live_position_ms(214_000, 0, 10_000, true, 215_000), 215_000);
        // Unknown duration: no clamp.
        assert_eq!(live_position_ms(214_000, 0, 10_000, true, 0), 224_000);
    }

    #[test]
    fn a_report_from_the_future_does_not_go_backwards() {
        assert_eq!(
            live_position_ms(42_000, 5_000, 1_000, true, 215_000),
            42_000
        );
    }

    #[test]
    fn artist_and_album_are_split_when_apple_music_merges_them() {
        assert_eq!(
            split_artist_album("Perunggu \u{2014} Memorandum", ""),
            ("Perunggu".to_owned(), "Memorandum".to_owned())
        );
    }

    #[test]
    fn artist_and_album_are_left_alone_otherwise() {
        assert_eq!(
            split_artist_album("Perunggu", ""),
            ("Perunggu".to_owned(), String::new())
        );
        assert_eq!(
            split_artist_album("A \u{2014} B", "Real Album"),
            ("A \u{2014} B".to_owned(), "Real Album".to_owned()),
            "an album that is already there wins"
        );
        assert_eq!(split_artist_album("", ""), (String::new(), String::new()));
    }

    fn snapshot(status: PlaybackStatus) -> Snapshot {
        Snapshot {
            status,
            title: "33x".into(),
            artist: "Perunggu \u{2014} Memorandum".into(),
            album: String::new(),
            duration_ms: 434_000,
            position_ms: 175_000,
            position_reported_at_unix_ms: 1_000,
        }
    }

    #[test]
    fn snapshot_becomes_the_wire_state() {
        let state = snapshot(PlaybackStatus::Playing).to_state(7, 3_000);
        assert_eq!(state.status, PlaybackStatus::Playing);
        assert_eq!(state.artist, "Perunggu");
        assert_eq!(state.album, "Memorandum");
        assert_eq!(state.position_ms, 177_000);
        assert_eq!(state.artwork_rev, 7);
        assert_eq!(state.playlist_name, None);
        let paused = snapshot(PlaybackStatus::Paused).to_state(7, 3_000);
        assert_eq!(paused.position_ms, 175_000);
    }

    fn jpeg() -> Vec<u8> {
        vec![0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3]
    }

    #[test]
    fn content_type_is_sniffed_from_the_bytes() {
        assert_eq!(Artwork::from_bytes(jpeg()).content_type, "image/jpeg");
        assert_eq!(
            Artwork::from_bytes(vec![0x89, b'P', b'N', b'G', 0]).content_type,
            "image/png"
        );
        assert_eq!(
            Artwork::from_bytes(vec![1, 2, 3]).content_type,
            "application/octet-stream"
        );
    }

    #[test]
    fn artwork_rev_bumps_only_when_the_track_changes() {
        let mut cache = ArtworkCache::default();
        assert_eq!(cache.rev(), 0);
        assert!(cache.needs_fetch("track-a"));

        assert!(cache.update("track-a", Some(jpeg())), "first track");
        assert_eq!(cache.rev(), 1);
        assert!(!cache.needs_fetch("track-a"), "already have it");

        // Same track, same image (for example a pause): no bump.
        assert!(!cache.update("track-a", Some(jpeg())));
        assert_eq!(cache.rev(), 1);

        // New track bumps, even with identical bytes.
        assert!(cache.needs_fetch("track-b"));
        assert!(cache.update("track-b", Some(jpeg())));
        assert_eq!(cache.rev(), 2);
    }

    #[test]
    fn artwork_that_arrives_late_bumps_once() {
        let mut cache = ArtworkCache::default();
        assert!(cache.update("track-a", None), "new track without image");
        assert_eq!(cache.rev(), 1);
        assert!(cache.artwork().is_none());
        assert!(cache.needs_fetch("track-a"), "missing artwork is retried");

        assert!(cache.update("track-a", Some(jpeg())), "the image appeared");
        assert_eq!(cache.rev(), 2);
        assert!(!cache.needs_fetch("track-a"));
    }

    #[test]
    fn a_track_without_artwork_is_not_retried_forever() {
        let mut cache = ArtworkCache::default();
        cache.update("track-a", None);
        for _ in 1..MAX_FETCH_ATTEMPTS {
            assert!(cache.needs_fetch("track-a"));
            assert!(!cache.update("track-a", None));
        }
        assert!(!cache.needs_fetch("track-a"));
        assert_eq!(cache.rev(), 1, "no artwork ever appeared: one bump only");
    }

    fn other_jpeg() -> Vec<u8> {
        vec![0xFF, 0xD8, 0xFF, 0xE0, 9, 9, 9]
    }

    #[test]
    fn a_new_track_with_the_old_cover_is_read_again_until_the_cover_changes() {
        let mut cache = ArtworkCache::default();
        cache.update("track-a", Some(jpeg()));
        // SMTC still serves the cover of track A right after the title changed.
        assert!(cache.update("track-b", Some(jpeg())));
        assert_eq!(cache.rev(), 2);
        assert!(
            cache.needs_fetch("track-b"),
            "identical to the previous cover: look again"
        );
        // The real cover shows up.
        assert!(cache.update("track-b", Some(other_jpeg())));
        assert_eq!(cache.rev(), 3, "re-fetch the cover for the new revision");
        assert!(!cache.needs_fetch("track-b"));
        assert_eq!(cache.artwork().unwrap().bytes.as_ref(), &other_jpeg());
    }

    #[test]
    fn tracks_that_really_share_a_cover_stop_being_read_after_a_few_tries() {
        let mut cache = ArtworkCache::default();
        cache.update("track-a", Some(jpeg()));
        cache.update("track-b", Some(jpeg()));
        let mut reads = 0;
        while cache.needs_fetch("track-b") {
            assert!(!cache.update("track-b", Some(jpeg())));
            reads += 1;
            assert!(reads < 50, "must stop");
        }
        assert_eq!(cache.rev(), 2, "no bump for an unchanged cover");
    }

    #[test]
    fn an_unset_timeline_timestamp_means_now() {
        assert_eq!(reported_at_unix_ms(0, 5_000), 5_000);
        assert_eq!(reported_at_unix_ms(-1, 5_000), 5_000);
        assert_eq!(reported_at_unix_ms(116_444_736_010_000_000, 5_000), 1_000);
    }

    #[test]
    fn clearing_forgets_the_artwork() {
        let mut cache = ArtworkCache::default();
        cache.update("track-a", Some(jpeg()));
        cache.clear();
        assert_eq!(cache.rev(), 2);
        assert!(cache.artwork().is_none());
        assert!(cache.needs_fetch("track-a"));
        cache.clear();
        assert_eq!(cache.rev(), 2, "nothing to forget");
    }
}
