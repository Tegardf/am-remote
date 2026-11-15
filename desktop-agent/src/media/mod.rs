//! Abstraction over "the thing that controls Apple Music".
//!
//! `WindowsBackend` (SMTC + Core Audio on one dedicated thread) is the real implementation.
//! `FakeBackend` is the test double: tests never need Apple Music, a window or the tray.

pub mod appsearch;
pub mod audio;
pub mod library;
pub mod links;
pub mod modes;
pub mod queue;
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

pub use appsearch::{AppSearchResult, Scope};
pub use audio::VolumeState;
pub use library::Playlist;
pub use modes::{Modes, Repeat};
pub use queue::QueueItem;
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

    /// Shuffle and repeat as the transport bar shows them (contract C-006). May bring a minimized
    /// window back. `NotSupported` when the buttons cannot be found or read.
    async fn modes(&self) -> Result<Modes, MediaError>;

    /// Like [`MediaBackend::modes`] but never touches the window: `None` unless the main window is
    /// visible and both buttons can be read. Used for polling and the WebSocket burst.
    async fn modes_if_visible(&self) -> Option<Modes>;

    /// Sets whichever of shuffle and repeat is given and returns the state read back afterwards.
    /// `Internal` if the app did not end up in the asked state.
    async fn set_modes(
        &self,
        shuffle: Option<bool>,
        repeat: Option<Repeat>,
    ) -> Result<Modes, MediaError>;

    /// What the queue pane shows: at most about 20 rows, `queue` rows first, then `autoplay`.
    /// Opens the pane if it is closed and leaves it open.
    async fn queue(&self) -> Result<Vec<QueueItem>, MediaError>;

    /// Skips forward to row `index` after checking that its title is still `title`.
    /// `NotFound` if the row moved or changed, `Internal` if another song ended up playing.
    async fn queue_play(&self, index: usize, title: &str) -> Result<(), MediaError>;

    /// Searches through Apple Music's own search box. An empty answer is `Ok(vec![])`.
    async fn app_search(
        &self,
        query: &str,
        scope: Scope,
    ) -> Result<Vec<AppSearchResult>, MediaError>;

    /// Repeats the search, finds `title` at `section` and `index` (`NotFound` if it is not there)
    /// and plays it. `NotSupported` for a kind that cannot be started yet.
    async fn app_search_play(
        &self,
        query: &str,
        scope: Scope,
        section: &str,
        index: usize,
        title: &str,
    ) -> Result<(), MediaError>;
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

    async fn modes(&self) -> Result<Modes, MediaError> {
        Err(backend_failed())
    }

    async fn modes_if_visible(&self) -> Option<Modes> {
        None
    }

    async fn set_modes(
        &self,
        _shuffle: Option<bool>,
        _repeat: Option<Repeat>,
    ) -> Result<Modes, MediaError> {
        Err(backend_failed())
    }

    async fn queue(&self) -> Result<Vec<QueueItem>, MediaError> {
        Err(backend_failed())
    }

    async fn queue_play(&self, _index: usize, _title: &str) -> Result<(), MediaError> {
        Err(backend_failed())
    }

    async fn app_search(
        &self,
        _query: &str,
        _scope: Scope,
    ) -> Result<Vec<AppSearchResult>, MediaError> {
        Err(backend_failed())
    }

    async fn app_search_play(
        &self,
        _query: &str,
        _scope: Scope,
        _section: &str,
        _index: usize,
        _title: &str,
    ) -> Result<(), MediaError> {
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
    modes: Mutex<Modes>,
    queue: Mutex<Vec<QueueItem>>,
    queue_plays: Mutex<Vec<usize>>,
    queue_lands_wrong: AtomicBool,
    catalog_results: Mutex<Vec<AppSearchResult>>,
    library_results: Mutex<Vec<AppSearchResult>>,
    app_searches: Mutex<Vec<(String, Scope)>>,
    app_plays: Mutex<Vec<AppPlay>>,
    events: broadcast::Sender<MediaEvent>,
}

/// One call of `app_search_play` the fake accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPlay {
    pub query: String,
    pub scope: Scope,
    pub section: String,
    pub index: usize,
    pub title: String,
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
            modes: Mutex::new(Modes {
                shuffle: false,
                repeat: Repeat::Off,
            }),
            queue: Mutex::new(Vec::new()),
            queue_plays: Mutex::new(Vec::new()),
            queue_lands_wrong: AtomicBool::new(false),
            catalog_results: Mutex::new(Vec::new()),
            library_results: Mutex::new(Vec::new()),
            app_searches: Mutex::new(Vec::new()),
            app_plays: Mutex::new(Vec::new()),
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

    /// Shuffle and repeat as the fake's buttons show them. Nothing is published: the real app gives
    /// no event either, the WebSocket polls.
    pub fn set_modes_state(&self, modes: Modes) {
        *lock(&self.modes) = modes;
    }

    /// The rows the fake's queue pane shows.
    pub fn set_queue(&self, items: Vec<QueueItem>) {
        *lock(&self.queue) = items;
    }

    /// Makes the next skips end on a different song than asked for.
    pub fn set_queue_lands_wrong(&self, wrong: bool) {
        self.queue_lands_wrong.store(wrong, Ordering::SeqCst);
    }

    /// Indexes of every queue row played, oldest first.
    pub fn queue_plays(&self) -> Vec<usize> {
        lock(&self.queue_plays).clone()
    }

    /// What the fake's search answers for `scope`.
    pub fn set_app_results(&self, scope: Scope, results: Vec<AppSearchResult>) {
        match scope {
            Scope::Catalog => *lock(&self.catalog_results) = results,
            Scope::Library => *lock(&self.library_results) = results,
        }
    }

    /// Every (query, scope) searched, oldest first.
    pub fn app_searches(&self) -> Vec<(String, Scope)> {
        lock(&self.app_searches).clone()
    }

    /// Every app-search result played, oldest first.
    pub fn app_plays(&self) -> Vec<AppPlay> {
        lock(&self.app_plays).clone()
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

    async fn modes(&self) -> Result<Modes, MediaError> {
        self.require_uia()?;
        Ok(*lock(&self.modes))
    }

    async fn modes_if_visible(&self) -> Option<Modes> {
        self.require_uia().ok()?;
        Some(*lock(&self.modes))
    }

    async fn set_modes(
        &self,
        shuffle: Option<bool>,
        repeat: Option<Repeat>,
    ) -> Result<Modes, MediaError> {
        self.require_uia()?;
        let mut modes = lock(&self.modes);
        if let Some(shuffle) = shuffle {
            modes.shuffle = shuffle;
        }
        if let Some(repeat) = repeat {
            modes.repeat = repeat;
        }
        Ok(*modes)
    }

    async fn queue(&self) -> Result<Vec<QueueItem>, MediaError> {
        self.require_uia()?;
        let mut items = lock(&self.queue).clone();
        items.truncate(queue::MAX_ROWS);
        Ok(items)
    }

    async fn queue_play(&self, index: usize, title: &str) -> Result<(), MediaError> {
        self.require_uia()?;
        let items = lock(&self.queue).clone();
        let row = items
            .get(index)
            .ok_or_else(|| MediaError::NotFound("There is no queue row at this index".into()))?;
        if !library::same_text(&row.title, title) {
            return Err(MediaError::NotFound(
                "The queue has changed: that row is not this song any more".into(),
            ));
        }
        if self.queue_lands_wrong.load(Ordering::SeqCst) {
            return Err(MediaError::Internal(
                "a different song is playing after skipping".into(),
            ));
        }
        lock(&self.queue_plays).push(index);
        Ok(())
    }

    async fn app_search(
        &self,
        query: &str,
        scope: Scope,
    ) -> Result<Vec<AppSearchResult>, MediaError> {
        self.require_uia()?;
        lock(&self.app_searches).push((query.to_owned(), scope));
        Ok(match scope {
            Scope::Catalog => lock(&self.catalog_results).clone(),
            Scope::Library => lock(&self.library_results).clone(),
        })
    }

    async fn app_search_play(
        &self,
        query: &str,
        scope: Scope,
        section: &str,
        index: usize,
        title: &str,
    ) -> Result<(), MediaError> {
        self.require_uia()?;
        let results = self.app_search(query, scope).await?;
        let found = results.iter().find(|r| {
            r.section == section
                && usize::try_from(r.index).is_ok_and(|i| i == index)
                && library::same_text(&r.title, title)
        });
        let result = found
            .ok_or_else(|| MediaError::NotFound("The search no longer finds that result".into()))?;
        if !appsearch::can_play(scope, result.kind) {
            return Err(MediaError::NotSupported(
                "This kind of result cannot be started yet".into(),
            ));
        }
        lock(&self.app_plays).push(AppPlay {
            query: query.to_owned(),
            scope,
            section: section.to_owned(),
            index,
            title: title.to_owned(),
        });
        Ok(())
    }
}
