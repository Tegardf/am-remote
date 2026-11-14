//! Apple Music through the Windows media session API (SMTC).
//!
//! SMTC gives transport control and the current track only: no queue, no playlists, no volume.
//! The manager and every session are WinRT objects: use this only from the media thread. The event
//! callbacks run on WinRT threads, so they only call the `Notify` closures (which post a message to
//! the media thread) and touch nothing else.

use std::sync::Arc;

use windows::core::Result;
use windows::Foundation::TypedEventHandler;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as Manager, MediaPropertiesChangedEventArgs,
    PlaybackInfoChangedEventArgs, SessionsChangedEventArgs, TimelinePropertiesChangedEventArgs,
};
use windows::Storage::Streams::DataReader;

use super::state::{
    ms_to_ticks, reported_at_unix_ms, ticks_to_ms, unix_now_ms, PlaybackStatus, Snapshot,
};
use super::Transport;

/// Start of Apple Music's AppUserModelId (publisher + package name), matched case-insensitively.
/// The full id is `AppleInc.AppleMusicWin_nzyj5cx40ttqa!App`; the hash part is not needed.
// verified: `cargo run --example list_sessions` with Apple Music playing, 2026-11-14
pub const APPLE_MUSIC_AUMID_PREFIX: &str = "appleinc.applemusicwin";

/// Covers must not be absurdly large; the one measured was 308 KB.
const MAX_ARTWORK_BYTES: u64 = 10 * 1024 * 1024;

/// Called from WinRT threads when something happened. Must be cheap and thread-safe.
pub type Notify = Arc<dyn Fn() + Send + Sync + 'static>;

/// What a seek request led to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeekOutcome {
    NoSession,
    Accepted,
    Refused,
}

struct Subscription {
    session: Session,
    media: i64,
    playback: i64,
    timeline: i64,
}

pub struct Smtc {
    manager: Manager,
    sessions_changed_token: i64,
    on_changed: Notify,
    subscription: Option<Subscription>,
}

impl Smtc {
    /// `on_sessions_changed` fires when a media session appears or disappears (Apple Music started
    /// or quit); `on_changed` fires when the Apple Music session's track, playback or timeline changed.
    pub fn new(on_sessions_changed: Notify, on_changed: Notify) -> Result<Self> {
        let manager = Manager::RequestAsync()?.join()?;
        let handler = TypedEventHandler::<Manager, SessionsChangedEventArgs>::new(move |_, _| {
            on_sessions_changed();
            Ok(())
        });
        let sessions_changed_token = manager.SessionsChanged(&handler)?;
        Ok(Self {
            manager,
            sessions_changed_token,
            on_changed,
            subscription: None,
        })
    }

    /// Apple Music's session, if there is one. Sessions of other apps are never returned.
    fn session(&self) -> Result<Option<Session>> {
        for session in self.manager.GetSessions()? {
            // A broken session of some other app must not stop us from finding Apple Music.
            let Ok(aumid) = session.SourceAppUserModelId() else {
                continue;
            };
            if aumid
                .to_string()
                .to_lowercase()
                .starts_with(APPLE_MUSIC_AUMID_PREFIX)
            {
                return Ok(Some(session));
            }
        }
        Ok(None)
    }

    pub fn is_running(&self) -> Result<bool> {
        Ok(self.session()?.is_some())
    }

    pub fn is_subscribed(&self) -> bool {
        self.subscription.is_some()
    }

    /// Drops the current subscription and subscribes to the (possibly new) Apple Music session.
    /// Call it whenever the session list changed: after Apple Music restarts, the old session object
    /// is dead and its events never fire again.
    pub fn resubscribe(&mut self) -> Result<()> {
        self.unsubscribe();
        let Some(session) = self.session()? else {
            return Ok(());
        };
        let (on_media, on_playback, on_timeline) = (
            self.on_changed.clone(),
            self.on_changed.clone(),
            self.on_changed.clone(),
        );
        let media = session.MediaPropertiesChanged(&TypedEventHandler::<
            Session,
            MediaPropertiesChangedEventArgs,
        >::new(move |_, _| {
            on_media();
            Ok(())
        }))?;
        let playback = session.PlaybackInfoChanged(&TypedEventHandler::<
            Session,
            PlaybackInfoChangedEventArgs,
        >::new(move |_, _| {
            on_playback();
            Ok(())
        }))?;
        let timeline = session.TimelinePropertiesChanged(&TypedEventHandler::<
            Session,
            TimelinePropertiesChangedEventArgs,
        >::new(move |_, _| {
            on_timeline();
            Ok(())
        }))?;
        self.subscription = Some(Subscription {
            session,
            media,
            playback,
            timeline,
        });
        Ok(())
    }

    pub fn unsubscribe(&mut self) {
        if let Some(sub) = self.subscription.take() {
            // The session may already be gone; failing to remove a handler from it is harmless.
            let _ = sub.session.RemoveMediaPropertiesChanged(sub.media);
            let _ = sub.session.RemovePlaybackInfoChanged(sub.playback);
            let _ = sub.session.RemoveTimelinePropertiesChanged(sub.timeline);
        }
    }

    /// Current track, status and timeline, or `None` if there is no Apple Music session.
    pub fn snapshot(&self) -> Result<Option<Snapshot>> {
        let Some(session) = self.session()? else {
            return Ok(None);
        };
        let props = session.TryGetMediaPropertiesAsync()?.join()?;
        let status = PlaybackStatus::from_smtc(session.GetPlaybackInfo()?.PlaybackStatus()?.0);
        let timeline = session.GetTimelineProperties()?;
        let start = timeline.StartTime()?.Duration;
        let end = timeline.EndTime()?.Duration;
        let position = timeline.Position()?.Duration;
        Ok(Some(Snapshot {
            status,
            title: props.Title()?.to_string(),
            artist: props.Artist()?.to_string(),
            album: props.AlbumTitle()?.to_string(),
            duration_ms: ticks_to_ms(end.saturating_sub(start)),
            position_ms: ticks_to_ms(position.saturating_sub(start)),
            position_reported_at_unix_ms: reported_at_unix_ms(
                timeline.LastUpdatedTime()?.UniversalTime,
                unix_now_ms(),
            ),
        }))
    }

    /// The current track's cover as raw image bytes, if Apple Music provides one.
    pub fn thumbnail(&self) -> Result<Option<Vec<u8>>> {
        let Some(session) = self.session()? else {
            return Ok(None);
        };
        let props = session.TryGetMediaPropertiesAsync()?.join()?;
        let Ok(reference) = props.Thumbnail() else {
            return Ok(None);
        };
        let stream = reference.OpenReadAsync()?.join()?;
        let size = stream.Size()?;
        if size == 0 || size > MAX_ARTWORK_BYTES {
            return Ok(None);
        }
        let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0)?)?;
        reader.LoadAsync(size as u32)?.join()?;
        let mut bytes = vec![0u8; size as usize];
        reader.ReadBytes(&mut bytes)?;
        Ok(Some(bytes))
    }

    /// Asks Apple Music to jump to `position_ms`.
    pub fn seek(&self, position_ms: u64) -> Result<SeekOutcome> {
        let Some(session) = self.session()? else {
            return Ok(SeekOutcome::NoSession);
        };
        let start = session.GetTimelineProperties()?.StartTime()?.Duration;
        let target = start.saturating_add(ms_to_ticks(position_ms));
        let accepted = session.TryChangePlaybackPositionAsync(target)?.join()?;
        Ok(if accepted {
            SeekOutcome::Accepted
        } else {
            SeekOutcome::Refused
        })
    }

    /// Sends the command. `Ok(false)` means there is no Apple Music session.
    /// Apple Music may still refuse a command (for example `Next` at the end of a queue);
    /// that is not an error for the caller.
    pub fn send(&self, command: Transport) -> Result<bool> {
        let Some(session) = self.session()? else {
            return Ok(false);
        };
        let accepted = match command {
            Transport::Play => session.TryPlayAsync()?.join()?,
            Transport::Pause => session.TryPauseAsync()?.join()?,
            Transport::Toggle => session.TryTogglePlayPauseAsync()?.join()?,
            Transport::Next => session.TrySkipNextAsync()?.join()?,
            Transport::Previous => session.TrySkipPreviousAsync()?.join()?,
        };
        if !accepted {
            tracing::debug!(?command, "Apple Music did not accept the command");
        }
        Ok(true)
    }
}

impl Drop for Smtc {
    fn drop(&mut self) {
        self.unsubscribe();
        let _ = self
            .manager
            .RemoveSessionsChanged(self.sessions_changed_token);
    }
}
