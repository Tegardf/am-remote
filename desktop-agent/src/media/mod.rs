//! Abstraction over "the thing that controls Apple Music".
//!
//! `WindowsBackend` (SMTC + Core Audio on one dedicated thread) is the real implementation.
//! `FakeBackend` is the test double: tests never need Apple Music, a window or the tray.

pub mod audio;
pub mod library;
pub mod links;
pub mod smtc;
pub mod state;
pub mod tracker;
#[cfg(feature = "uia")]
pub mod uia;
pub mod worker;

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use async_trait::async_trait;
use tokio::sync::broadcast;

pub use audio::VolumeState;
pub use library::Playlist;
pub use state::{Artwork, MediaEvent, PlaybackStatus, PlayerState};
pub use worker::WindowsBackend;

/// Answer when playlists and playing songs cannot work: the `uia` feature is off, or Apple Music's
/// window could not be driven.
pub const UIA_UNAVAILABLE: &str =
    "Playlists and playing a song need UI Automation of the Apple Music window, which is not available on this agent";

/// How many events a slow WebSocket client may lag behind before it is resynchronised.
pub const EVENT_BUFFER: usize = 64;

/// A transport command for the Apple Music session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
}

/// What `POST /play/track` asks the backend to play.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayTrack {
    /// The `music://` link of the song's album page, which the app is asked to open.
    pub open_url: String,
    pub title: String,
    pub artist: String,
    /// The album (or single): the page that opens is expected to be named like this.
    pub album: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaError {
    /// Apple Music has no media session, so there is nothing to control.
    NotRunning,
    /// Apple Music is there but Windows has no audio session for it yet (it appears after the
    /// first playback). Only the volume calls answer it: `409 NO_AUDIO_SESSION` (contract C-003).
    NoAudioSession,
    /// Apple Music is running but cannot do this (for example seek). The text is shown to the client.
    NotSupported(String),
    /// What was asked for does not exist (a playlist id that is no longer in the library).
    NotFound(String),
    /// Anything unexpected from Windows. The text is for the log, never for the client.
    Internal(String),
}

impl fmt::Display for MediaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotRunning => write!(f, "Apple Music is not running"),
            Self::NoAudioSession => write!(f, "Apple Music has no audio session yet"),
            Self::NotSupported(message) | Self::NotFound(message) | Self::Internal(message) => {
                write!(f, "{message}")
            }
        }
    }
}

impl std::error::Error for MediaError {}

#[async_trait]
pub trait MediaBackend: Send + Sync {
    /// Whether the Apple Music app has a media session we can control.
    async fn apple_music_running(&self) -> bool;

    /// Sends a transport command to Apple Music's session (and to no other app's).
    async fn transport(&self, command: Transport) -> Result<(), MediaError>;

    /// Apple Music's own volume (not the master volume).
    async fn volume(&self) -> Result<VolumeState, MediaError>;

    /// Sets whichever of `level` (0-100) and `muted` is given; returns the resulting state.
    async fn set_volume(
        &self,
        level: Option<u8>,
        muted: Option<bool>,
    ) -> Result<VolumeState, MediaError>;

    /// What is playing now, with the position brought up to date. When Apple Music is not running
    /// this is [`PlayerState::empty`]; it never fails.
    async fn state(&self) -> PlayerState;

    /// The current track's cover, if there is one.
    async fn artwork(&self) -> Option<Artwork>;

    /// Jumps to `position_ms` in the current track.
    async fn seek(&self, position_ms: u64) -> Result<(), MediaError>;

    /// Live changes: state, volume and whether Apple Music is running.
    fn subscribe(&self) -> broadcast::Receiver<MediaEvent>;

    /// Whether playlists and playing songs can work at all (the `uia` feature is built in and its
    /// thread started). Callers use it to answer `NotSupported` before doing any other work.
    fn ui_supported(&self) -> bool;

    /// The playlists of the user's library. Needs UI Automation: without it `NotSupported`.
    async fn playlists(&self) -> Result<Vec<Playlist>, MediaError>;

    /// Starts the library playlist with this id (an id from [`MediaBackend::playlists`]).
    /// `NotFound` if there is no such playlist, `NotSupported` if Apple Music did not start it.
    async fn play_playlist(&self, id: &str) -> Result<(), MediaError>;

    /// Opens the song's page and plays exactly that song.
    async fn play_track(&self, track: PlayTrack) -> Result<(), MediaError>;
}

/// Used by the binary if the Windows backend cannot start. `/status` says not running, and media
/// commands answer `INTERNAL` (the agent is broken; Apple Music may be fine). The cause is logged.
#[derive(Debug)]
pub struct NullBackend {
    events: broadcast::Sender<MediaEvent>,
}

impl Default for NullBackend {
    fn default() -> Self {
        Self {
            events: broadcast::channel(1).0,
        }
    }
}

fn backend_failed() -> MediaError {
    MediaError::Internal("the media backend failed to start".into())
}

#[async_trait]
impl MediaBackend for NullBackend {
    async fn apple_music_running(&self) -> bool {
        false
    }

    async fn transport(&self, _command: Transport) -> Result<(), MediaError> {
        Err(backend_failed())
    }

    async fn volume(&self) -> Result<VolumeState, MediaError> {
        Err(backend_failed())
    }

    async fn set_volume(
        &self,
        _level: Option<u8>,
        _muted: Option<bool>,
    ) -> Result<VolumeState, MediaError> {
        Err(backend_failed())
    }

    async fn state(&self) -> PlayerState {
        PlayerState::empty(0)
    }

    async fn artwork(&self) -> Option<Artwork> {
        None
    }

    async fn seek(&self, _position_ms: u64) -> Result<(), MediaError> {
        Err(backend_failed())
    }

    fn subscribe(&self) -> broadcast::Receiver<MediaEvent> {
        self.events.subscribe()
    }

    fn ui_supported(&self) -> bool {
        false
    }

    async fn playlists(&self) -> Result<Vec<Playlist>, MediaError> {
        Err(backend_failed())
    }

    async fn play_playlist(&self, _id: &str) -> Result<(), MediaError> {
        Err(backend_failed())
    }

    async fn play_track(&self, _track: PlayTrack) -> Result<(), MediaError> {
        Err(backend_failed())
    }
}

/// Test double. Starts with Apple Music running, an audio session, volume 50 and not muted, and an
/// empty player state. Its `set_*` methods change what it reports and publish the matching event,
/// like the real backend does.
#[derive(Debug)]
pub struct FakeBackend {
    running: AtomicBool,
    audio_session: AtomicBool,
    failing: AtomicBool,
    seek_supported: AtomicBool,
    volume: Mutex<VolumeState>,
    commands: Mutex<Vec<Transport>>,
    state: Mutex<PlayerState>,
    artwork: Mutex<Option<Artwork>>,
    seeks: Mutex<Vec<u64>>,
    playlists: Mutex<Vec<Playlist>>,
    uia_supported: AtomicBool,
    played_playlists: Mutex<Vec<String>>,
    played_tracks: Mutex<Vec<PlayTrack>>,
    events: broadcast::Sender<MediaEvent>,
}

impl Default for FakeBackend {
    fn default() -> Self {
        Self::new(true)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

impl FakeBackend {
    pub fn new(running: bool) -> Self {
        Self::with_event_buffer(running, EVENT_BUFFER)
    }

    /// Like [`FakeBackend::new`] with a small event buffer, so a test can make a client lag.
    pub fn with_event_buffer(running: bool, buffer: usize) -> Self {
        Self {
            running: AtomicBool::new(running),
            audio_session: AtomicBool::new(true),
            failing: AtomicBool::new(false),
            seek_supported: AtomicBool::new(true),
            volume: Mutex::new(VolumeState {
                level: 50,
                muted: false,
            }),
            commands: Mutex::new(Vec::new()),
            state: Mutex::new(PlayerState::empty(0)),
            artwork: Mutex::new(None),
            seeks: Mutex::new(Vec::new()),
            playlists: Mutex::new(Vec::new()),
            uia_supported: AtomicBool::new(true),
            played_playlists: Mutex::new(Vec::new()),
            played_tracks: Mutex::new(Vec::new()),
            events: broadcast::channel(buffer).0,
        }
    }

    fn publish(&self, event: MediaEvent) {
        // No subscribers is fine.
        let _ = self.events.send(event);
    }

    /// Simulates Apple Music starting or quitting; publishes `AppleMusic` if that changed it.
    pub fn set_running(&self, running: bool) {
        if self.running.swap(running, Ordering::SeqCst) != running {
            self.publish(MediaEvent::AppleMusic { running });
        }
    }

    /// Simulates Apple Music running but not having played anything yet.
    pub fn set_audio_session(&self, present: bool) {
        self.audio_session.store(present, Ordering::SeqCst);
    }

    /// Makes every transport and volume call fail with `MediaError::Internal`.
    pub fn set_failing(&self, failing: bool) {
        self.failing.store(failing, Ordering::SeqCst);
    }

    /// Whether Apple Music accepts seeking (on the real machine SMTC may refuse it).
    pub fn set_seek_supported(&self, supported: bool) {
        self.seek_supported.store(supported, Ordering::SeqCst);
    }

    /// Replaces the track state and publishes it, like a track change in Apple Music.
    pub fn set_state(&self, state: PlayerState) {
        *lock(&self.state) = state.clone();
        self.publish(MediaEvent::State(state));
    }

    pub fn set_artwork(&self, artwork: Option<Artwork>) {
        *lock(&self.artwork) = artwork;
    }

    /// The library playlists the fake reports.
    pub fn set_playlists(&self, playlists: Vec<Playlist>) {
        *lock(&self.playlists) = playlists;
    }

    /// Whether UI Automation works (on the real machine it is a cargo feature that may be off, or
    /// Apple Music may have changed its UI).
    pub fn set_uia_supported(&self, supported: bool) {
        self.uia_supported.store(supported, Ordering::SeqCst);
    }

    /// Ids of every playlist started, oldest first.
    pub fn played_playlists(&self) -> Vec<String> {
        lock(&self.played_playlists).clone()
    }

    /// Every song started, oldest first.
    pub fn played_tracks(&self) -> Vec<PlayTrack> {
        lock(&self.played_tracks).clone()
    }

    /// Like the real backend: "not supported" is answered before "not running".
    fn require_uia(&self) -> Result<(), MediaError> {
        if !self.uia_supported.load(Ordering::SeqCst) {
            return Err(MediaError::NotSupported(UIA_UNAVAILABLE.into()));
        }
        self.require_running()
    }

    /// Every transport command received while running, oldest first.
    pub fn commands(&self) -> Vec<Transport> {
        lock(&self.commands).clone()
    }

    /// Every accepted seek position (ms), oldest first.
    pub fn seeks(&self) -> Vec<u64> {
        lock(&self.seeks).clone()
    }

    fn require_running(&self) -> Result<(), MediaError> {
        if self.failing.load(Ordering::SeqCst) {
            return Err(MediaError::Internal("simulated backend failure".into()));
        }
        if self.running.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(MediaError::NotRunning)
        }
    }

    fn require_audio_session(&self) -> Result<(), MediaError> {
        self.require_running()?;
        if self.audio_session.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(MediaError::NoAudioSession)
        }
    }
}

#[async_trait]
impl MediaBackend for FakeBackend {
    async fn apple_music_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    async fn transport(&self, command: Transport) -> Result<(), MediaError> {
        self.require_running()?;
        lock(&self.commands).push(command);
        Ok(())
    }

    async fn volume(&self) -> Result<VolumeState, MediaError> {
        self.require_audio_session()?;
        Ok(*lock(&self.volume))
    }

    async fn set_volume(
        &self,
        level: Option<u8>,
        muted: Option<bool>,
    ) -> Result<VolumeState, MediaError> {
        self.require_audio_session()?;
        let now = {
            let mut volume = lock(&self.volume);
            if let Some(level) = level {
                volume.level = level.min(100);
            }
            if let Some(muted) = muted {
                volume.muted = muted;
            }
            *volume
        };
        self.publish(MediaEvent::Volume(now));
        Ok(now)
    }

    async fn state(&self) -> PlayerState {
        let stored = lock(&self.state).clone();
        if self.running.load(Ordering::SeqCst) {
            stored
        } else {
            PlayerState::empty(stored.artwork_rev)
        }
    }

    async fn artwork(&self) -> Option<Artwork> {
        if self.running.load(Ordering::SeqCst) {
            lock(&self.artwork).clone()
        } else {
            None
        }
    }

    async fn seek(&self, position_ms: u64) -> Result<(), MediaError> {
        self.require_running()?;
        if !self.seek_supported.load(Ordering::SeqCst) {
            return Err(MediaError::NotSupported(
                "Apple Music does not allow seeking from outside".into(),
            ));
        }
        lock(&self.seeks).push(position_ms);
        Ok(())
    }

    fn subscribe(&self) -> broadcast::Receiver<MediaEvent> {
        self.events.subscribe()
    }

    fn ui_supported(&self) -> bool {
        self.uia_supported.load(Ordering::SeqCst)
    }

    async fn playlists(&self) -> Result<Vec<Playlist>, MediaError> {
        self.require_uia()?;
        Ok(lock(&self.playlists).clone())
    }

    async fn play_playlist(&self, id: &str) -> Result<(), MediaError> {
        self.require_uia()?;
        if !lock(&self.playlists).iter().any(|p| p.id == id) {
            return Err(MediaError::NotFound(
                "There is no playlist with this id".into(),
            ));
        }
        lock(&self.played_playlists).push(id.to_owned());
        Ok(())
    }

    async fn play_track(&self, track: PlayTrack) -> Result<(), MediaError> {
        self.require_uia()?;
        lock(&self.played_tracks).push(track);
        Ok(())
    }
}
