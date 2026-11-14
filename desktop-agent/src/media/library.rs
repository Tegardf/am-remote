//! Playlists and matching rules for the Apple Music UI. Pure logic, no Windows calls.
//!
//! What these rules rely on was observed with `examples/spike_playlists.rs`,
//! `spike_playlist_page.rs` and `spike_link_click.rs` on Apple Music 1.1792.39942.0, English UI
//! (see `docs/spikes/link-playback.md`).

use serde::{Deserialize, Serialize};

/// A playlist of the user's library (`GET /playlists`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Playlist {
    /// Opaque, stable: the sidebar item's AutomationId (`DBID:...-IKIND:ePlaylist`).
    pub id: String,
    pub name: String,
}

/// Library playlists in the sidebar have an AutomationId that ends like this.
// verified: spike_playlists, 2026-11-14
pub const PLAYLIST_ID_SUFFIX: &str = "IKIND:ePlaylist";

/// AutomationId of the sidebar entry that opens (and expands) the playlists list.
// verified: spike_playlists, 2026-11-14
pub const PLAYLISTS_HEADER_ID: &str = "Sidebar_Header_Playlists";

/// AutomationId of the page's Play button (album and playlist pages).
// verified: spike_link_click, spike_playlist_page, 2026-11-14
pub const PLAY_BUTTON_ID: &str = "PlayButtonElement";

/// AutomationId of a content page's header, named exactly like the album or playlist shown.
// verified: spike_playlist_page, 2026-11-14 (playlist pages only: album pages have no such header)
pub const PAGE_HEADER_ID: &str = "ContainerDetailHeaderLockupRegularContent";

/// The playlist behind a sidebar item, or `None` if the item is something else.
pub fn playlist_from_sidebar_item(automation_id: &str, name: &str) -> Option<Playlist> {
    if automation_id.ends_with(PLAYLIST_ID_SUFFIX) && !name.trim().is_empty() {
        Some(Playlist {
            id: automation_id.to_owned(),
            name: name.to_owned(),
        })
    } else {
        None
    }
}

/// Lower-case, quotes and dashes unified, whitespace collapsed: so that `Don’t` equals `Don't`.
pub fn normalize(text: &str) -> String {
    let unified: String = text
        .chars()
        .map(|c| match c {
            '\u{2018}' | '\u{2019}' | '\u{02BC}' => '\'',
            '\u{201C}' | '\u{201D}' => '"',
            '\u{2013}' | '\u{2014}' => '-',
            other => other,
        })
        .collect();
    unified
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Whether `shown` (what the UI says) and `expected` (what we looked up) are the same text.
pub fn same_text(shown: &str, expected: &str) -> bool {
    let (shown, expected) = (normalize(shown), normalize(expected));
    !expected.is_empty() && shown == expected
}

/// Rows of an album page are named `"Track <n> <title> <duration>"`, for example
/// `"Track 6 Jatuh Suka 3 minutes, 55 seconds"`. (The play-queue pane on the right lists rows named
/// `"<title> <artist> — <album> <duration>"` instead; those must never be mistaken for the page.)
// verified: spike_link_click with DUMP_ALL, 2026-11-14 (English UI)
pub fn is_page_row_name(name: &str) -> bool {
    normalize(name)
        .strip_prefix("track ")
        .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
}

/// True if `text` is a spoken duration: one to three pairs of a number and a unit word, for example
/// `"3 minutes, 55 seconds"`, `"4 minutes"` or `"1 hour, 3 minutes, 44 seconds"`.
fn is_duration(text: &str) -> bool {
    let tokens: Vec<&str> = text
        .split_whitespace()
        .map(|t| t.trim_end_matches(','))
        .collect();
    matches!(tokens.len(), 2 | 4 | 6)
        && tokens.chunks(2).all(|pair| {
            !pair[0].is_empty()
                && pair[0].chars().all(|c| c.is_ascii_digit())
                && !pair[1].is_empty()
                && pair[1].chars().all(|c| c.is_alphabetic())
        })
}

/// True if the album page row `name` is the song with the (already normalized) `wanted` title:
/// after `"Track <n> "` comes exactly the title and then a duration.
fn row_is_track(name: &str, wanted: &str) -> bool {
    let normalized = normalize(name);
    let Some(rest) = normalized.strip_prefix("track ") else {
        return false;
    };
    let Some((number, rest)) = rest.split_once(' ') else {
        return false;
    };
    number.chars().all(|c| c.is_ascii_digit())
        && rest
            .strip_prefix(wanted)
            .is_some_and(|after| after.starts_with(' ') && is_duration(after))
}

/// Picks the row of the song `title` among the album page rows (`rows` are only page rows, see
/// [`is_page_row_name`]). Exactly one row must match: two songs with the same title on one page
/// are never guessed between.
pub fn find_track_row(rows: &[String], title: &str) -> Option<usize> {
    let wanted = normalize(title);
    if wanted.is_empty() {
        return None;
    }
    let mut matches = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row_is_track(row, &wanted))
        .map(|(index, _)| index);
    match (matches.next(), matches.next()) {
        (Some(only), None) => Some(only),
        _ => None,
    }
}

/// The context-menu entry that plays one song is named `Play “<title>”` (curly quotes). It is
/// matched by the quoted title, so it does not depend on the UI language and never matches
/// "Play Next".
// verified: spike_link_click rowmenu, 2026-11-14 (English UI)
pub fn menu_item_plays(menu_item_name: &str, title: &str) -> bool {
    let title = normalize(title);
    if title.is_empty() {
        return false;
    }
    // Exactly one word, then the quoted title, nothing after it: not "Add “x” to Library" and not
    // "Play Next". UNVERIFIED: another one-word entry that quotes the title (a hypothetical
    // `Share “x”`) would match too, which is why the driver takes the first match, and `Play` is
    // the first entry that quotes the title in the menu seen on 2026-11-14.
    let name = normalize(menu_item_name);
    name.split_once(' ')
        .is_some_and(|(verb, rest)| !verb.contains('"') && rest == format!("\"{title}\""))
}

/// Did playback start since `before` was read? It must be playing, not showing the transient
/// "Connecting…" title, and either have been stopped before, or show another track, or have gone
/// back to the start of the same one. With `expected_title`, the track must also be that song.
pub fn playback_started(
    before: &crate::media::PlayerState,
    now: &crate::media::PlayerState,
    expected_title: Option<&str>,
) -> bool {
    use crate::media::PlaybackStatus::Playing;
    if now.status != Playing || now.title.starts_with("Connecting") {
        return false;
    }
    if let Some(expected) = expected_title {
        return same_text(&now.title, expected);
    }
    before.status != Playing
        || now.title != before.title
        || now.position_ms + 2_000 < before.position_ms
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::{PlaybackStatus, PlayerState};

    const ID: &str = "DBID:0x1-PID:0x2-PPID:0x0-PIPID:0x0-IKIND:ePlaylist";

    #[test]
    fn sidebar_items_become_playlists_only_when_they_are_playlists() {
        assert_eq!(
            playlist_from_sidebar_item(ID, "Road Trip"),
            Some(Playlist {
                id: ID.into(),
                name: "Road Trip".into()
            })
        );
        for (id, name) in [
            ("Sidebar_AllPlaylists", "All Playlists"),
            ("Sidebar_Home", "Home"),
            ("Sidebar_Header_Playlists", "Playlists"),
            ("", "Library"),
            (ID, ""),
            (ID, "   "),
        ] {
            assert_eq!(
                playlist_from_sidebar_item(id, name),
                None,
                "{id:?} {name:?}"
            );
        }
    }

    #[test]
    fn playlist_uses_the_contract_field_names() {
        let json = serde_json::to_value(Playlist {
            id: "pl-1".into(),
            name: "Favourites".into(),
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "id": "pl-1", "name": "Favourites" })
        );
    }

    #[test]
    fn text_comparison_ignores_case_quotes_dashes_and_spacing() {
        assert!(same_text("Don\u{2019}t Stop", "don't  stop"));
        assert!(same_text("Chill & relax", "chill & RELAX"));
        assert!(same_text("A \u{2013} B", "a - b"));
        assert!(!same_text("Chill", "Chilling"));
        assert!(
            !same_text("anything", ""),
            "an empty expectation never matches"
        );
    }

    fn rows() -> Vec<String> {
        [
            "Track 1 Tujuh Belas 4 minutes, 15 seconds",
            "Track 2 Kelana 3 minutes, 12 seconds",
            "Track 3 Remedi Part 2 1 minute, 1 second",
            "Track 4 Remedi 4 minutes, 8 seconds",
            "Track 6 Jatuh Suka 3 minutes, 55 seconds",
            "Track 7 Hati-Hati di Jalan 4 minutes, 2 seconds",
            "Track 8 Diri 4 minutes",
            "Track 9 Satu Kali 1 hour, 3 minutes, 44 seconds",
        ]
        .map(String::from)
        .to_vec()
    }

    #[test]
    fn album_page_rows_are_told_from_queue_rows() {
        assert!(is_page_row_name("Track 6 Jatuh Suka 3 minutes, 55 seconds"));
        assert!(is_page_row_name("track 10 Satu Kali 3 minutes"));
        assert!(!is_page_row_name(
            "Jatuh Suka Tulus \u{2014} Manusia 3 minutes, 55 seconds"
        ));
        assert!(!is_page_row_name(
            "Track Star Someone \u{2014} Album 3 minutes"
        ));
        assert!(!is_page_row_name("Home"));
        assert!(!is_page_row_name(""));
    }

    #[test]
    fn the_right_row_is_found_by_title() {
        assert_eq!(find_track_row(&rows(), "Jatuh Suka"), Some(4));
        assert_eq!(find_track_row(&rows(), "Kelana"), Some(1));
        assert_eq!(find_track_row(&rows(), "Hati-Hati di Jalan"), Some(5));
        assert_eq!(find_track_row(&rows(), "diri"), Some(6));
    }

    #[test]
    fn a_title_that_is_the_start_of_another_title_does_not_confuse_it() {
        assert_eq!(find_track_row(&rows(), "Remedi"), Some(3));
        assert_eq!(find_track_row(&rows(), "Remedi Part 2"), Some(2));
    }

    #[test]
    fn titles_containing_numbers_still_work() {
        let rows = vec![
            "Track 1 2001x 3 minutes, 24 seconds".to_owned(),
            "Track 2 99 Problems 3 minutes, 54 seconds".to_owned(),
            "Track 3 Song 2 4 minutes".to_owned(),
        ];
        assert_eq!(find_track_row(&rows, "2001x"), Some(0));
        assert_eq!(find_track_row(&rows, "99 Problems"), Some(1));
        assert_eq!(find_track_row(&rows, "Song 2"), Some(2));
        assert_eq!(find_track_row(&rows, "Song"), None);
    }

    #[test]
    fn a_long_duration_is_recognised() {
        assert_eq!(find_track_row(&rows(), "Satu Kali"), Some(7));
    }

    #[test]
    fn ambiguity_is_never_guessed() {
        let twice = vec![
            "Track 1 Intro 1 minute".to_owned(),
            "Track 5 Intro 2 minutes".to_owned(),
        ];
        assert_eq!(find_track_row(&twice, "Intro"), None);
    }

    #[test]
    fn a_missing_song_or_empty_title_finds_nothing() {
        assert_eq!(find_track_row(&rows(), "Not On This Album"), None);
        assert_eq!(find_track_row(&rows(), ""), None);
        assert_eq!(find_track_row(&[], "Jatuh Suka"), None);
    }

    #[test]
    fn curly_quotes_in_titles_match() {
        let rows = vec!["Track 1 Don't Stop Me Now 3 minutes".to_owned()];
        assert_eq!(find_track_row(&rows, "Don\u{2019}t Stop Me Now"), Some(0));
    }

    #[test]
    fn only_the_play_this_song_menu_item_is_chosen() {
        let title = "Jatuh Suka";
        assert!(menu_item_plays("Play \u{201C}Jatuh Suka\u{201D}", title));
        assert!(menu_item_plays("Putar \u{201C}Jatuh Suka\u{201D}", title));
        for other in [
            "Play Next",
            "Add to Queue",
            "Add to Library",
            "Create Station",
            "Play",
            "Share",
            "",
        ] {
            assert!(!menu_item_plays(other, title), "{other:?}");
        }
        assert!(!menu_item_plays("Play \u{201C}Another Song\u{201D}", title));
        assert!(!menu_item_plays("Play \u{201C}Jatuh Suka\u{201D}", ""));
        // Entries that mention the title inside a longer phrase are never the play entry.
        assert!(!menu_item_plays(
            "Remove \u{201C}Jatuh Suka\u{201D} from Library",
            title
        ));
        assert!(!menu_item_plays(
            "Add \u{201C}Jatuh Suka\u{201D} to Playlist",
            title
        ));
        assert!(!menu_item_plays(
            "Play \u{201C}Jatuh Suka\u{201D} next",
            title
        ));
        assert!(!menu_item_plays("\u{201C}Jatuh Suka\u{201D}", title));
    }

    fn state(status: PlaybackStatus, title: &str, position_ms: u64) -> PlayerState {
        PlayerState {
            status,
            title: title.into(),
            position_ms,
            ..PlayerState::empty(0)
        }
    }

    #[test]
    fn playback_that_started_is_recognised() {
        use PlaybackStatus::{Paused, Playing};
        let paused = state(Paused, "Old", 30_000);
        // Was paused, now playing (maybe the same track).
        assert!(playback_started(&paused, &state(Playing, "Old", 0), None));
        // Was playing something else, now another track.
        let playing = state(Playing, "Old", 30_000);
        assert!(playback_started(&playing, &state(Playing, "New", 0), None));
        // Same track restarted from the beginning.
        assert!(playback_started(
            &playing,
            &state(Playing, "Old", 1_000),
            None
        ));
    }

    #[test]
    fn playback_that_did_not_start_is_not_recognised() {
        use PlaybackStatus::{Paused, Playing};
        let playing = state(Playing, "Old", 30_000);
        assert!(!playback_started(
            &playing,
            &state(Playing, "Old", 31_000),
            None
        ));
        assert!(!playback_started(&playing, &state(Paused, "New", 0), None));
        assert!(!playback_started(
            &state(Paused, "Old", 0),
            &state(Playing, "Connecting\u{2026}", 0),
            None
        ));
    }

    #[test]
    fn an_expected_title_must_be_the_one_playing() {
        use PlaybackStatus::{Paused, Playing};
        let before = state(Paused, "Old", 0);
        assert!(playback_started(
            &before,
            &state(Playing, "Jatuh Suka", 0),
            Some("jatuh  suka")
        ));
        assert!(!playback_started(
            &before,
            &state(Playing, "Another Song", 0),
            Some("Jatuh Suka")
        ));
    }
}
