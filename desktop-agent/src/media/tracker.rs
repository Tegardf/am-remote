//! Turns observations of Apple Music into the events clients should see.
//!
//! The media thread reads Windows (a [`Snapshot`], the volume) and hands the facts to
//! [`Tracker::observe`], which remembers the previous ones and says what changed. It is pure, so it
//! is unit-tested without Windows.

use super::audio::VolumeState;
use super::state::{
    live_position_ms, Artwork, ArtworkCache, MediaEvent, PlaybackStatus, PlayerState, Snapshot,
};

/// Clients work out the position themselves (position + time since the state arrived), so a new
/// `state` is only pushed for the position when it differs from that prediction by more than this:
/// a seek, a skip, a stall. Ordinary playback, which SMTC reports every second, stays silent.
pub const POSITION_JUMP_MS: u64 = 2_000;

/// What the media thread saw in one look at Windows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    /// `None` means Apple Music has no media session (not running).
    pub snapshot: Option<Snapshot>,
    /// `None` if there is no audio session (yet).
    pub volume: Option<VolumeState>,
}

#[derive(Debug, Default)]
pub struct Tracker {
    running: Option<bool>,
    /// Latest facts from Windows, for `state()` queries.
    last: Option<Snapshot>,
    /// The last state pushed to clients, and when.
    published: Option<(PlayerState, i64)>,
    volume: Option<VolumeState>,
    artwork: ArtworkCache,
}

impl Tracker {
    /// Applies a fresh observation and returns the events to publish, in order: running,
    /// state, volume. `fetch_artwork` is called only when the cover has to be (re)read.
    pub fn observe(
        &mut self,
        observation: Observation,
        now_unix_ms: i64,
        fetch_artwork: impl FnOnce() -> Option<Vec<u8>>,
    ) -> Vec<MediaEvent> {
        let mut events = Vec::new();

        let running = observation.snapshot.is_some();
        // The first look only establishes the baseline: nobody was told anything before it.
        if self.running.is_some_and(|before| before != running) {
            events.push(MediaEvent::AppleMusic { running });
        }
        self.running = Some(running);

        match observation.snapshot {
            Some(snapshot) => {
                let key = snapshot.track_key();
                if self.artwork.needs_fetch(&key) {
                    self.artwork.update(&key, fetch_artwork());
                }
                let state = snapshot.to_state(self.artwork.rev(), now_unix_ms);
                if self.should_publish(&state, now_unix_ms) {
                    events.push(MediaEvent::State(state.clone()));
                    self.published = Some((state, now_unix_ms));
                }
                self.last = Some(snapshot);
            }
            None => {
                if self.last.take().is_some() {
                    self.artwork.clear();
                    self.published = None;
                    events.push(MediaEvent::State(PlayerState::empty(self.artwork.rev())));
                }
            }
        }

        if observation.volume != self.volume {
            self.volume = observation.volume;
            if let Some(volume) = observation.volume {
                events.push(MediaEvent::Volume(volume));
            }
        }
        events
    }

    /// Whether `state` differs from what clients already have, beyond normal playback progress.
    fn should_publish(&self, state: &PlayerState, now_unix_ms: i64) -> bool {
        let Some((published, at)) = &self.published else {
            return true;
        };
        let predicted = live_position_ms(
            published.position_ms,
            *at,
            now_unix_ms,
            published.status == PlaybackStatus::Playing,
            published.duration_ms,
        );
        published.status != state.status
            || published.title != state.title
            || published.artist != state.artist
            || published.album != state.album
            || published.duration_ms != state.duration_ms
            || published.artwork_rev != state.artwork_rev
            || predicted.abs_diff(state.position_ms) > POSITION_JUMP_MS
    }

    /// The current state with the position brought up to `now_unix_ms`.
    pub fn state(&self, now_unix_ms: i64) -> PlayerState {
        match &self.last {
            Some(snapshot) => snapshot.to_state(self.artwork.rev(), now_unix_ms),
            None => PlayerState::empty(self.artwork.rev()),
        }
    }

    pub fn artwork(&self) -> Option<Artwork> {
        self.artwork.artwork()
    }
}

#[cfg(test)]
mod tests {
    use super::super::state::PlaybackStatus;
    use super::*;

    fn snapshot(title: &str, status: PlaybackStatus, position_ms: u64) -> Snapshot {
        Snapshot {
            status,
            title: title.into(),
            artist: "Artist".into(),
            album: "Album".into(),
            duration_ms: 200_000,
            position_ms,
            position_reported_at_unix_ms: 0,
        }
    }

    fn look(snapshot: Option<Snapshot>, volume: Option<VolumeState>) -> Observation {
        Observation { snapshot, volume }
    }

    fn jpeg() -> Option<Vec<u8>> {
        Some(vec![0xFF, 0xD8, 0xFF, 1])
    }

    fn volume(level: u8) -> VolumeState {
        VolumeState {
            level,
            muted: false,
        }
    }

    fn states(events: &[MediaEvent]) -> Vec<&PlayerState> {
        events
            .iter()
            .filter_map(|e| match e {
                MediaEvent::State(s) => Some(s),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn first_look_publishes_the_state_but_no_running_event() {
        let mut tracker = Tracker::default();
        let events = tracker.observe(
            look(Some(snapshot("A", PlaybackStatus::Playing, 0)), None),
            0,
            jpeg,
        );
        assert_eq!(states(&events).len(), 1);
        assert!(!events
            .iter()
            .any(|e| matches!(e, MediaEvent::AppleMusic { .. })));
    }

    /// Playing for a while: SMTC reports a fresh position every second, the clock advances with it.
    #[test]
    fn normal_playback_progress_publishes_nothing() {
        let mut tracker = Tracker::default();
        let at = |secs: i64| Snapshot {
            position_ms: 10_000 + secs as u64 * 1_000,
            position_reported_at_unix_ms: secs * 1_000,
            ..snapshot("A", PlaybackStatus::Playing, 0)
        };
        assert_eq!(tracker.observe(look(Some(at(0)), None), 0, jpeg).len(), 1);
        for secs in 1..=30 {
            let events = tracker.observe(look(Some(at(secs)), None), secs * 1_000, || {
                panic!("same track: no cover read")
            });
            assert!(events.is_empty(), "second {secs}: {events:?}");
        }
    }

    #[test]
    fn a_seek_is_published() {
        let mut tracker = Tracker::default();
        let playing = |position_ms, reported_at| Snapshot {
            position_ms,
            position_reported_at_unix_ms: reported_at,
            ..snapshot("A", PlaybackStatus::Playing, 0)
        };
        tracker.observe(look(Some(playing(10_000, 0)), None), 0, jpeg);

        // Jump forward to 1:00 two seconds later.
        let events = tracker.observe(look(Some(playing(60_000, 2_000)), None), 2_000, || None);
        assert_eq!(states(&events)[0].position_ms, 60_000);

        // And back to the start.
        let events = tracker.observe(look(Some(playing(0, 3_000)), None), 3_000, || None);
        assert_eq!(states(&events)[0].position_ms, 0);
    }

    #[test]
    fn a_small_drift_is_not_published() {
        let mut tracker = Tracker::default();
        let snap = snapshot("A", PlaybackStatus::Playing, 10_000);
        tracker.observe(look(Some(snap.clone()), None), 0, jpeg);
        // The reported position is 1.5 s ahead of the prediction: within tolerance.
        let drifted = Snapshot {
            position_ms: 11_500,
            position_reported_at_unix_ms: 0,
            ..snap
        };
        assert!(tracker
            .observe(look(Some(drifted), None), 0, || None)
            .is_empty());
    }

    #[test]
    fn nothing_changed_means_no_events() {
        let mut tracker = Tracker::default();
        let snap = snapshot("A", PlaybackStatus::Playing, 5_000);
        tracker.observe(look(Some(snap.clone()), Some(volume(40))), 0, jpeg);
        let again = tracker.observe(look(Some(snap), Some(volume(40))), 1_000, || {
            panic!("the cover must not be read again for the same track")
        });
        assert!(again.is_empty(), "{again:?}");
    }

    #[test]
    fn a_new_track_publishes_state_with_a_higher_artwork_rev() {
        let mut tracker = Tracker::default();
        let first = tracker.observe(
            look(Some(snapshot("A", PlaybackStatus::Playing, 0)), None),
            0,
            jpeg,
        );
        let second = tracker.observe(
            look(Some(snapshot("B", PlaybackStatus::Playing, 0)), None),
            0,
            jpeg,
        );
        let (a, b) = (
            states(&first)[0].artwork_rev,
            states(&second)[0].artwork_rev,
        );
        assert!(b > a, "{a} -> {b}");
        assert_eq!(states(&second)[0].title, "B");
    }

    #[test]
    fn pausing_changes_the_state_but_not_the_artwork_rev() {
        let mut tracker = Tracker::default();
        let playing = tracker.observe(
            look(Some(snapshot("A", PlaybackStatus::Playing, 1_000)), None),
            0,
            jpeg,
        );
        let paused = tracker.observe(
            look(Some(snapshot("A", PlaybackStatus::Paused, 1_000)), None),
            0,
            || panic!("same track: no cover read"),
        );
        assert_eq!(states(&paused)[0].status, PlaybackStatus::Paused);
        assert_eq!(
            states(&paused)[0].artwork_rev,
            states(&playing)[0].artwork_rev
        );
    }

    #[test]
    fn a_cover_that_arrives_late_republishes_the_state_with_a_new_rev() {
        let mut tracker = Tracker::default();
        let snap = snapshot("A", PlaybackStatus::Playing, 0);
        let first = tracker.observe(look(Some(snap.clone()), None), 0, || None);
        let second = tracker.observe(look(Some(snap), None), 0, jpeg);
        assert!(states(&second)[0].artwork_rev > states(&first)[0].artwork_rev);
        assert!(tracker.artwork().is_some());
    }

    #[test]
    fn quitting_apple_music_publishes_running_false_and_an_empty_state() {
        let mut tracker = Tracker::default();
        tracker.observe(
            look(
                Some(snapshot("A", PlaybackStatus::Playing, 0)),
                Some(volume(40)),
            ),
            0,
            jpeg,
        );
        let events = tracker.observe(look(None, None), 0, || None);
        assert_eq!(events[0], MediaEvent::AppleMusic { running: false });
        let empty = states(&events);
        assert_eq!(empty.len(), 1);
        assert_eq!(empty[0].title, "");
        assert_eq!(empty[0].status, PlaybackStatus::Stopped);
        assert!(tracker.artwork().is_none());

        // Still gone: silence.
        assert!(tracker.observe(look(None, None), 0, || None).is_empty());
    }

    #[test]
    fn restarting_apple_music_publishes_running_true_then_the_state() {
        let mut tracker = Tracker::default();
        tracker.observe(look(None, None), 0, || None);
        let events = tracker.observe(
            look(Some(snapshot("A", PlaybackStatus::Playing, 0)), None),
            0,
            jpeg,
        );
        assert_eq!(events[0], MediaEvent::AppleMusic { running: true });
        assert!(matches!(events[1], MediaEvent::State(_)));
    }

    #[test]
    fn a_volume_change_publishes_one_volume_event() {
        let mut tracker = Tracker::default();
        let snap = snapshot("A", PlaybackStatus::Playing, 0);
        tracker.observe(look(Some(snap.clone()), Some(volume(40))), 0, jpeg);
        let events = tracker.observe(look(Some(snap), Some(volume(25))), 0, || None);
        assert_eq!(events, vec![MediaEvent::Volume(volume(25))]);
    }

    /// Contract C-003: no `volume` event when the audio session goes away while Apple Music keeps
    /// running (clients treat the last level as stale), and one again when it comes back, even at
    /// the same level.
    #[test]
    fn losing_the_audio_session_sends_no_volume_event_and_getting_it_back_sends_one() {
        let mut tracker = Tracker::default();
        let snap = snapshot("A", PlaybackStatus::Playing, 0);
        tracker.observe(look(Some(snap.clone()), Some(volume(40))), 0, jpeg);

        let lost = tracker.observe(look(Some(snap.clone()), None), 0, || None);
        assert_eq!(lost, vec![], "the session vanished: nothing is sent");
        let still_lost = tracker.observe(look(Some(snap.clone()), None), 0, || None);
        assert_eq!(
            still_lost,
            vec![],
            "and nothing is sent while it stays gone"
        );

        let back = tracker.observe(look(Some(snap), Some(volume(40))), 0, || None);
        assert_eq!(back, vec![MediaEvent::Volume(volume(40))]);
    }

    #[test]
    fn state_query_interpolates_the_position() {
        let mut tracker = Tracker::default();
        tracker.observe(
            look(Some(snapshot("A", PlaybackStatus::Playing, 10_000)), None),
            0,
            jpeg,
        );
        assert_eq!(tracker.state(2_500).position_ms, 12_500);
        assert_eq!(Tracker::default().state(0), PlayerState::empty(0));
    }
}
