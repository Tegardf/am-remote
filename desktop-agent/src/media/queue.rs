//! The Up Next queue (contract C-006): the wire type and the pure rules for turning the pane's
//! row texts into `QueueItem`s. The driver in `uia.rs` reads the rows; nothing here needs Windows.
//! See `docs/spikes/mvp4.md` section B for what the pane exposes.

use serde::{Deserialize, Serialize};

/// AutomationId of the toolbar toggle that shows or hides the queue pane (`TogglePattern`).
// verified: spike mvp4, 2026-11-14
pub const QUEUE_TOGGLE_ID: &str = "PlayQueueToggleButton";
/// AutomationId of the "Playing Next" list.
// verified: spike mvp4, 2026-11-14
pub const QUEUE_LIST_ID: &str = "PlayQueueListView";
/// AutomationId of the "AutoPlay" list that follows it.
// verified: spike mvp4, 2026-11-14
pub const AUTOPLAY_LIST_ID: &str = "PlayQueueListViewAutoplay";
/// The pane shows at most about this many rows in total (observed 11-20, never more).
// verified: spike mvp4, 2026-11-14
pub const MAX_ROWS: usize = 20;

/// Where a row comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QueueSource {
    /// "Playing Next": the user's queue.
    Queue,
    /// "AutoPlay": songs Apple Music adds after the queue.
    Autoplay,
}

/// `QueueItem` on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct QueueItem {
    pub index: u32,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// 0 if the app shows none.
    pub duration_ms: u64,
    pub source: QueueSource,
}

/// The separator between artist and album in the row's second line.
const SEPARATOR: &str = " — ";

/// `"Artist — Album"` -> (artist, album), split at the first separator. Without one the whole text
/// is the artist.
pub fn split_artist_album(text: &str) -> (String, String) {
    let text = text.trim();
    match text.split_once(SEPARATOR) {
        Some((artist, album)) => (artist.trim().to_owned(), album.trim().to_owned()),
        None => (text.to_owned(), String::new()),
    }
}

/// `"4:49"` or `"1:02:03"` -> milliseconds; 0 for anything else (including empty).
pub fn parse_duration_ms(text: &str) -> u64 {
    let parts: Vec<&str> = text.trim().split(':').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return 0;
    }
    let mut seconds = 0u64;
    for part in &parts {
        match part.trim().parse::<u64>() {
            Ok(n) => seconds = seconds.saturating_mul(60).saturating_add(n),
            Err(_) => return 0,
        }
    }
    seconds.saturating_mul(1000)
}

/// One row from its three text parts (title, `"Artist — Album"`, `"m:ss"`; the last two may be
/// missing). `index` is the position across both lists.
pub fn queue_item(index: usize, source: QueueSource, parts: &[String]) -> Option<QueueItem> {
    let title = parts.first()?.trim();
    if title.is_empty() {
        return None;
    }
    let (artist, album) = split_artist_album(parts.get(1).map(String::as_str).unwrap_or(""));
    Some(QueueItem {
        index: u32::try_from(index).unwrap_or(u32::MAX),
        title: title.to_owned(),
        artist,
        album,
        duration_ms: parse_duration_ms(parts.get(2).map(String::as_str).unwrap_or("")),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn artist_and_album_split_at_the_first_separator() {
        assert_eq!(
            split_artist_album("Slipknot — All Hope Is Gone (10th Anniversary)"),
            (
                "Slipknot".into(),
                "All Hope Is Gone (10th Anniversary)".into()
            )
        );
        assert_eq!(
            split_artist_album("A & B — Album — Deluxe"),
            ("A & B".into(), "Album — Deluxe".into())
        );
        assert_eq!(
            split_artist_album("Only Artist"),
            ("Only Artist".into(), "".into())
        );
        assert_eq!(split_artist_album(""), ("".into(), "".into()));
    }

    #[test]
    fn durations_are_minutes_and_seconds() {
        assert_eq!(parse_duration_ms("4:49"), 289_000);
        assert_eq!(parse_duration_ms("0:05"), 5_000);
        assert_eq!(parse_duration_ms("1:02:03"), 3_723_000);
        for bad in ["", "abc", "4", "4:xx", "1:2:3:4", "-1:00"] {
            assert_eq!(parse_duration_ms(bad), 0, "{bad:?}");
        }
    }

    #[test]
    fn a_row_becomes_a_queue_item() {
        let item = queue_item(
            2,
            QueueSource::Autoplay,
            &parts(&["Dead Memories", "Slipknot — .5: The Gray Chapter", "4:29"]),
        )
        .unwrap();
        assert_eq!(
            item,
            QueueItem {
                index: 2,
                title: "Dead Memories".into(),
                artist: "Slipknot".into(),
                album: ".5: The Gray Chapter".into(),
                duration_ms: 269_000,
                source: QueueSource::Autoplay,
            }
        );
    }

    #[test]
    fn a_missing_duration_or_second_line_is_zero_or_empty() {
        let no_duration =
            queue_item(0, QueueSource::Queue, &parts(&["Song", "Artist — Album"])).unwrap();
        assert_eq!(no_duration.duration_ms, 0);
        let title_only = queue_item(0, QueueSource::Queue, &parts(&["Song"])).unwrap();
        assert_eq!(
            (title_only.artist.as_str(), title_only.album.as_str()),
            ("", "")
        );
    }

    #[test]
    fn a_row_without_a_title_is_not_an_item() {
        assert_eq!(queue_item(0, QueueSource::Queue, &[]), None);
        assert_eq!(queue_item(0, QueueSource::Queue, &parts(&["  "])), None);
    }

    #[test]
    fn items_serialise_like_the_contract() {
        let item = queue_item(1, QueueSource::Queue, &parts(&["A", "B — C", "1:00"])).unwrap();
        assert_eq!(
            serde_json::to_value(item).unwrap(),
            serde_json::json!({
                "index": 1, "title": "A", "artist": "B", "album": "C",
                "duration_ms": 60000, "source": "queue"
            })
        );
    }
}
