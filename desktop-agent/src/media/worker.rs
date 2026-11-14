//! The media thread: one dedicated OS thread owns every WinRT/COM object.
//!
//! Handlers send a command over a channel and await the reply on a oneshot channel, so they never
//! block the tokio runtime and no COM object crosses threads. The same thread watches Apple Music:
//! WinRT event callbacks only post a message to it, and it also looks again once a second (volume
//! has no event here, and a missed event must not leave clients stale). Changes go out through a
//! `tokio::sync::broadcast` channel to the WebSocket clients.

use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use tokio::sync::{broadcast, oneshot};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

use super::audio::AppleMusicVolume;
#[cfg(feature = "uia")]
use super::library::playback_started;
use super::smtc::{Notify, SeekOutcome, Smtc};
use super::state::{unix_now_ms, Artwork, MediaEvent, PlayerState};
use super::tracker::{Observation, Tracker};
use super::{
    MediaBackend, MediaError, PlayTrack, Playlist, Transport, VolumeState, EVENT_BUFFER,
    UIA_UNAVAILABLE,
};

type Reply<T> = oneshot::Sender<T>;

/// How long a handler waits for the media thread before answering `INTERNAL`.
const REPLY_TIMEOUT: Duration = Duration::from_secs(5);

/// How long to wait, after the UI was driven, for Apple Music to actually start playing.
#[cfg(feature = "uia")]
const START_WAIT: Duration = Duration::from_secs(6);

/// How long dropping the backend waits for the media thread to wind down.
const SHUTDOWN_WAIT: Duration = Duration::from_secs(2);

/// After an event, wait this long without another one before looking (events come in bursts).
const DEBOUNCE: Duration = Duration::from_millis(150);

/// A steady stream of events must not postpone the look forever.
const MAX_DEBOUNCE: Duration = Duration::from_millis(600);

/// How often the media thread looks at Apple Music even if no event arrived.
const POLL: Duration = Duration::from_secs(1);

enum Command {
    /// Something changed in the Apple Music session (from a WinRT callback).
    Refresh,
    /// A media session appeared or disappeared (from a WinRT callback).
    SessionsChanged,
    Stop,
    Running(Reply<bool>),
    Transport(Transport, Reply<Result<(), MediaError>>),
    GetVolume(Reply<Result<VolumeState, MediaError>>),
    SetVolume {
        level: Option<u8>,
        muted: Option<bool>,
        reply: Reply<Result<VolumeState, MediaError>>,
    },
    State(Reply<PlayerState>),
    Artwork(Reply<Option<Artwork>>),
    Seek(u64, Reply<Result<(), MediaError>>),
}

/// Real backend: SMTC for playback and metadata, Core Audio for volume.
pub struct WindowsBackend {
    commands: mpsc::Sender<Command>,
    events: broadcast::Sender<MediaEvent>,
    thread: Option<thread::JoinHandle<()>>,
    /// Drives Apple Music's window for playlists and songs; `None` if it could not start.
    #[cfg(feature = "uia")]
    uia: Option<super::uia::Uia>,
}

impl WindowsBackend {
    /// Starts the media thread and waits until COM and SMTC are initialised on it.
    pub fn start() -> Result<Self> {
        let (commands, inbox) = mpsc::channel();
        let (events, _) = broadcast::channel(EVENT_BUFFER);
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
        let thread_commands = commands.clone();
        let thread_events = events.clone();
        let thread = thread::Builder::new()
            .name("media".into())
            .spawn(move || run(inbox, thread_commands, thread_events, ready_tx))
            .context("cannot start the media thread")?;
        ready_rx
            .recv()
            .context("the media thread stopped during startup")?
            .map_err(|e| anyhow!(e))?;
        // Playlists and songs are optional: if UI Automation cannot start, everything else works.
        #[cfg(feature = "uia")]
        let uia = match super::uia::Uia::start() {
            Ok(uia) => Some(uia),
            Err(e) => {
                tracing::warn!("playlists and playing songs are off: {e:#}");
                None
            }
        };
        Ok(Self {
            commands,
            events,
            thread: Some(thread),
            #[cfg(feature = "uia")]
            uia,
        })
    }

    /// Polls the playing state until playback has started (see [`playback_started`]), up to 6 s.
    #[cfg(feature = "uia")]
    async fn wait_until_started(
        &self,
        before: &PlayerState,
        expected_title: Option<&str>,
        what: &str,
    ) -> Result<(), MediaError> {
        let deadline = tokio::time::Instant::now() + START_WAIT;
        loop {
            let now = self.state().await;
            if playback_started(before, &now, expected_title) {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(MediaError::NotSupported(format!(
                    "Apple Music did not start {what}"
                )));
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    }

    async fn ask<T>(&self, build: impl FnOnce(Reply<T>) -> Command) -> Result<T, MediaError> {
        let stopped = || MediaError::Internal("the media thread is not running".into());
        let (reply, answer) = oneshot::channel();
        self.commands.send(build(reply)).map_err(|_| stopped())?;
        // One stuck Windows call must not make every client hang forever.
        match tokio::time::timeout(REPLY_TIMEOUT, answer).await {
            Ok(answer) => answer.map_err(|_| stopped()),
            Err(_) => Err(MediaError::Internal(
                "the media thread did not answer in time".into(),
            )),
        }
    }
}

impl Drop for WindowsBackend {
    fn drop(&mut self) {
        // The thread keeps its own sender alive (inside the WinRT callbacks), so it needs telling.
        let _ = self.commands.send(Command::Stop);
        // Let the thread remove its WinRT handlers and uninitialise COM before the process exits,
        // but never hang the exit on a stuck Windows call.
        if let Some(thread) = self.thread.take() {
            let deadline = Instant::now() + SHUTDOWN_WAIT;
            while !thread.is_finished() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(20));
            }
            if thread.is_finished() {
                let _ = thread.join();
            }
        }
    }
}

#[async_trait]
impl MediaBackend for WindowsBackend {
    async fn apple_music_running(&self) -> bool {
        self.ask(Command::Running).await.unwrap_or(false)
    }

    async fn transport(&self, command: Transport) -> Result<(), MediaError> {
        self.ask(|reply| Command::Transport(command, reply)).await?
    }

    async fn volume(&self) -> Result<VolumeState, MediaError> {
        self.ask(Command::GetVolume).await?
    }

    async fn set_volume(
        &self,
        level: Option<u8>,
        muted: Option<bool>,
    ) -> Result<VolumeState, MediaError> {
        self.ask(|reply| Command::SetVolume {
            level,
            muted,
            reply,
        })
        .await?
    }

    async fn state(&self) -> PlayerState {
        self.ask(Command::State)
            .await
            .unwrap_or_else(|_| PlayerState::empty(0))
    }

    async fn artwork(&self) -> Option<Artwork> {
        self.ask(Command::Artwork).await.unwrap_or(None)
    }

    async fn seek(&self, position_ms: u64) -> Result<(), MediaError> {
        self.ask(|reply| Command::Seek(position_ms, reply)).await?
    }

    fn subscribe(&self) -> broadcast::Receiver<MediaEvent> {
        self.events.subscribe()
    }

    fn ui_supported(&self) -> bool {
        #[cfg(feature = "uia")]
        {
            self.uia.is_some()
        }
        #[cfg(not(feature = "uia"))]
        {
            false
        }
    }

    async fn playlists(&self) -> Result<Vec<Playlist>, MediaError> {
        #[cfg(feature = "uia")]
        if let Some(uia) = &self.uia {
            return uia.playlists().await;
        }
        Err(MediaError::NotSupported(UIA_UNAVAILABLE.into()))
    }

    async fn play_playlist(&self, id: &str) -> Result<(), MediaError> {
        #[cfg(feature = "uia")]
        if let Some(uia) = &self.uia {
            let before = self.state().await;
            uia.play_playlist(id).await?;
            return self.wait_until_started(&before, None, "the playlist").await;
        }
        let _ = id;
        Err(MediaError::NotSupported(UIA_UNAVAILABLE.into()))
    }

    async fn play_track(&self, track: PlayTrack) -> Result<(), MediaError> {
        #[cfg(feature = "uia")]
        if let Some(uia) = &self.uia {
            let before = self.state().await;
            uia.play_track(&track.open_url, &track.title, &track.album)
                .await?;
            return self
                .wait_until_started(&before, Some(&track.title), "the song")
                .await;
        }
        let _ = track;
        Err(MediaError::NotSupported(UIA_UNAVAILABLE.into()))
    }
}

fn internal(e: windows::core::Error) -> MediaError {
    MediaError::Internal(e.to_string())
}

/// Volume is only served while Apple Music has a media session, like `/status` and the transport
/// commands, so all three always agree on "running".
fn require_running(smtc: &Smtc) -> Result<(), MediaError> {
    match smtc.is_running() {
        Ok(true) => Ok(()),
        Ok(false) => Err(MediaError::NotRunning),
        Err(e) => Err(internal(e)),
    }
}

/// Everything the media thread owns.
struct Worker {
    smtc: Smtc,
    volume: AppleMusicVolume,
    tracker: Tracker,
    events: broadcast::Sender<MediaEvent>,
}

impl Worker {
    /// Looks at Apple Music once and publishes whatever changed.
    fn refresh(&mut self) {
        let snapshot = match self.smtc.snapshot() {
            Ok(snapshot) => snapshot,
            Err(e) => {
                // Usually Apple Music quitting mid-call; the next look sorts it out.
                tracing::debug!("cannot read the media session: {e}");
                return;
            }
        };
        let running = snapshot.is_some();
        if running && !self.smtc.is_subscribed() {
            self.resubscribe();
        } else if !running && self.smtc.is_subscribed() {
            self.smtc.unsubscribe();
        }
        let volume = if running {
            self.volume.get().ok().flatten()
        } else {
            None
        };

        let smtc = &self.smtc;
        let events = self
            .tracker
            .observe(Observation { snapshot, volume }, unix_now_ms(), || {
                smtc.thumbnail().unwrap_or_else(|e| {
                    tracing::debug!("cannot read the artwork: {e}");
                    None
                })
            });
        for event in events {
            // No WebSocket clients connected is fine.
            let _ = self.events.send(event);
        }
    }

    fn resubscribe(&mut self) {
        if let Err(e) = self.smtc.resubscribe() {
            tracing::warn!("cannot subscribe to Apple Music events (polling still works): {e}");
        }
    }

    fn handle(&mut self, command: Command) {
        // A dropped receiver just means the HTTP request went away; ignore send errors.
        match command {
            Command::Refresh | Command::SessionsChanged | Command::Stop => {}
            Command::Running(reply) => {
                let running = self.smtc.is_running().unwrap_or_else(|e| {
                    tracing::warn!("cannot read media sessions: {e}");
                    false
                });
                let _ = reply.send(running);
            }
            Command::Transport(transport, reply) => {
                let result = match self.smtc.send(transport) {
                    Ok(true) => Ok(()),
                    Ok(false) => Err(MediaError::NotRunning),
                    Err(e) => Err(internal(e)),
                };
                let _ = reply.send(result);
            }
            Command::GetVolume(reply) => {
                let result = require_running(&self.smtc).and_then(|()| {
                    self.volume
                        .get()
                        .map_err(internal)
                        .and_then(|volume| volume.ok_or(MediaError::NoAudioSession))
                });
                let _ = reply.send(result);
            }
            Command::SetVolume {
                level,
                muted,
                reply,
            } => {
                let result = require_running(&self.smtc).and_then(|()| {
                    self.volume
                        .set(level, muted)
                        .map_err(internal)
                        .and_then(|volume| volume.ok_or(MediaError::NoAudioSession))
                });
                let _ = reply.send(result);
                // Tell the WebSocket clients now instead of at the next look.
                self.refresh();
            }
            Command::State(reply) => {
                let _ = reply.send(self.tracker.state(unix_now_ms()));
            }
            Command::Artwork(reply) => {
                let _ = reply.send(self.tracker.artwork());
            }
            Command::Seek(position_ms, reply) => {
                let result = match self.smtc.seek(position_ms) {
                    Ok(SeekOutcome::Accepted) => Ok(()),
                    Ok(SeekOutcome::NoSession) => Err(MediaError::NotRunning),
                    Ok(SeekOutcome::Refused) => Err(MediaError::NotSupported(
                        "Apple Music did not accept the seek".into(),
                    )),
                    Err(e) => Err(internal(e)),
                };
                let _ = reply.send(result);
            }
        }
    }
}

/// Body of the media thread.
fn run(
    inbox: mpsc::Receiver<Command>,
    commands: mpsc::Sender<Command>,
    events: broadcast::Sender<MediaEvent>,
    ready: mpsc::Sender<Result<(), String>>,
) {
    // SAFETY: first COM call on this thread; paired with CoUninitialize below.
    let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    if com.is_err() {
        let _ = ready.send(Err(format!("CoInitializeEx failed: {com:?}")));
        return;
    }

    // WinRT calls these on its own threads: they only post a message to this thread.
    let on_sessions_changed: Notify = {
        let commands = commands.clone();
        Arc::new(move || {
            let _ = commands.send(Command::SessionsChanged);
        })
    };
    let on_changed: Notify = Arc::new(move || {
        let _ = commands.send(Command::Refresh);
    });

    let smtc = match Smtc::new(on_sessions_changed, on_changed) {
        Ok(smtc) => smtc,
        Err(e) => {
            let _ = ready.send(Err(format!("cannot open the media session manager: {e}")));
            // SAFETY: balances the successful CoInitializeEx above.
            unsafe { CoUninitialize() };
            return;
        }
    };
    let mut worker = Worker {
        smtc,
        volume: AppleMusicVolume::default(),
        tracker: Tracker::default(),
        events,
    };
    worker.resubscribe();
    worker.refresh();
    let _ = ready.send(Ok(()));

    let mut pending: Option<Command> = None;
    'main: loop {
        let command = match pending.take() {
            Some(command) => Some(command),
            None => match inbox.recv_timeout(POLL) {
                Ok(Command::Stop) => break,
                Ok(command) => Some(command),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            },
        };

        let mut resubscribe = matches!(command, Some(Command::SessionsChanged));
        if matches!(
            command,
            Some(Command::Refresh) | Some(Command::SessionsChanged)
        ) {
            // Apple Music reports a track change as a burst of events, and the first ones carry
            // half-updated facts (new title, old timeline). Wait for a quiet moment, then look once.
            let started = Instant::now();
            while started.elapsed() < MAX_DEBOUNCE {
                match inbox.recv_timeout(DEBOUNCE) {
                    Ok(Command::Refresh) => {}
                    Ok(Command::SessionsChanged) => resubscribe = true,
                    Ok(Command::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break 'main,
                    Ok(other) => {
                        pending = Some(other);
                        break;
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => break,
                }
            }
        }
        if resubscribe {
            // The old session object may be dead (Apple Music restarted): subscribe to the new one.
            worker.resubscribe();
        }
        // Always look first, so every answer and every event is based on fresh facts.
        worker.refresh();
        if let Some(command) =
            command.filter(|c| !matches!(c, Command::Refresh | Command::SessionsChanged))
        {
            worker.handle(command);
        }
    }

    // Drop the WinRT objects before COM goes away.
    drop(worker);
    // SAFETY: balances the successful CoInitializeEx above.
    unsafe { CoUninitialize() };
}
