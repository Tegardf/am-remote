//! Search through Apple Music's own search box (contract C-006): the wire types and the pure rules
//! that turn a result card's texts into an `AppSearchResult`. The driver in `uia.rs` drives the
//! search box and reads the page; nothing here needs Windows. See `docs/spikes/mvp4.md` section C.

use serde::{Deserialize, Serialize};

use super::queue;

/// AutomationId of the sidebar button that opens the search box (`InvokePattern`).
// verified: spike mvp4, 2026-11-14
pub const SEARCH_BUTTON_ID: &str = "Search_Button";
/// AutomationId of the search box's text field (a UIA Edit with `ValuePattern`).
// verified: spike mvp4, 2026-11-14
pub const SEARCH_BOX_ID: &str = "TextBox";
/// AutomationId of the suggestions list that pops up under the box.
// verified: spike mvp4, 2026-11-14
pub const SUGGESTIONS_LIST_ID: &str = "SuggestionsList";
/// AutomationId of the results page's status text: `Showing results for “<query>”`.
// verified: spike mvp4, 2026-11-14
pub const STATUS_TEXT_ID: &str = "SearchPageStatus";
/// AutomationIds of the two scope toggles on the results page (`TogglePattern`, exactly one is on).
// verified: spike mvp4, 2026-11-14
pub const SCOPE_CATALOG_ID: &str = "SearchAppleMusic";
pub const SCOPE_LIBRARY_ID: &str = "SearchLibrary";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    #[default]
    Catalog,
    Library,
}

impl Scope {
    /// `catalog` or `library` (any case); `None` for anything else.
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "catalog" => Some(Self::Catalog),
            "library" => Some(Self::Library),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Song,
    Album,
    Artist,
    Playlist,
    Video,
    Other,
}

/// `AppSearchResult` on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AppSearchResult {
    /// The section heading as the app shows it (`Songs`, `Albums`, `Top Results`, ...).
    pub section: String,
    /// The position inside that section.
    pub index: u32,
    pub kind: Kind,
    pub title: String,
    /// Artist or curator; empty if none.
    pub subtitle: String,
}

/// What a section heading says about its items (the `Top Results` section is mixed, so its items
/// are told apart by their label, see [`kind_from_label`]).
pub fn kind_from_section(section: &str) -> Kind {
    match section.trim().to_ascii_lowercase().as_str() {
        "songs" => Kind::Song,
        "albums" => Kind::Album,
        "artists" => Kind::Artist,
        "playlists" => Kind::Playlist,
        "music videos" => Kind::Video,
        _ => Kind::Other,
    }
}

/// The kind a card's label stands for (`Song · Artist` -> song, a bare `Artist` -> artist).
pub fn kind_from_label(label: &str) -> Kind {
    match label.trim().to_ascii_lowercase().as_str() {
        "song" => Kind::Song,
        "album" | "single" | "ep" => Kind::Album,
        "artist" => Kind::Artist,
        "playlist" => Kind::Playlist,
        "music video" | "video" => Kind::Video,
        _ => Kind::Other,
    }
}

/// The middle dot that separates label and detail in a card's second line.
const DOT: char = '·';

/// Splits `Song · Artist` into (label, detail); a text without the dot is a label alone.
fn split_label(text: &str) -> (String, String) {
    match text.split_once(DOT) {
        Some((label, detail)) => (label.trim().to_owned(), detail.trim().to_owned()),
        None => (text.trim().to_owned(), String::new()),
    }
}

/// A result from the card's text parts (its `TextBlock` children, in order) and its Name.
///
/// - In `Top Results` the first part is the title and the second `Song · <artist>` or `Artist`:
///   the kind comes from that label.
/// - In every other section the kind comes from the heading; the first part is the title and the
///   second, if any, the subtitle.
/// - With no parts at all the Name is the title.
pub fn parse_result(
    section: &str,
    index: usize,
    name: &str,
    parts: &[String],
) -> Option<AppSearchResult> {
    let name = name.trim();
    // A library song row (`Track 4 Title 3 minutes`) starts with its track number as a text part.
    let parts = match parts.first() {
        Some(first)
            if title_from_track_row(name).is_some()
                && !first.trim().is_empty()
                && first.trim().chars().all(|c| c.is_ascii_digit()) =>
        {
            &parts[1..]
        }
        _ => parts,
    };
    let title = parts
        .first()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .unwrap_or(name);
    if title.is_empty() {
        return None;
    }
    let second = parts.get(1).map(|p| p.trim()).unwrap_or("");
    // A row's last part is its duration (`3:32`): not a subtitle.
    let second = if second.contains(':') && queue::parse_duration_ms(second) > 0 {
        ""
    } else {
        second
    };
    let (kind, subtitle) = if section.trim().eq_ignore_ascii_case("top results") {
        let (label, detail) = split_label(second);
        (kind_from_label(&label), detail)
    } else {
        let (label, detail) = split_label(second);
        // A second line like `Album · 2020` in another section still has a dot: the detail is then
        // not an artist, keep the whole line as the subtitle.
        let known_label = kind_from_label(&label) != Kind::Other && !detail.is_empty();
        (
            kind_from_section(section),
            if known_label {
                detail
            } else {
                second.to_owned()
            },
        )
    };
    Some(AppSearchResult {
        section: section.trim().to_owned(),
        index: u32::try_from(index).unwrap_or(u32::MAX),
        kind,
        title: title.to_owned(),
        subtitle,
    })
}

/// Whether the first version can start a result of this kind in this scope: songs (library and
/// catalog) and catalog albums. Everything else answers `501` (contract C-006).
pub fn can_play(scope: Scope, kind: Kind) -> bool {
    matches!(
        (scope, kind),
        (_, Kind::Song) | (Scope::Catalog, Kind::Album)
    )
}

/// The song title in a page row's spoken name: `"Track 4 Bunga Tidur 3 minutes, 32 seconds"` ->
/// `"Bunga Tidur"`. `None` if the name is not that shape.
pub fn title_from_track_row(name: &str) -> Option<String> {
    let rest = name.trim().strip_prefix("Track ")?;
    let (number, rest) = rest.split_once(' ')?;
    if number.is_empty() || !number.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    // Drop the spoken duration at the end: "<n> minute(s)[, <n> second(s)]" or "<n> second(s)".
    let words: Vec<&str> = rest.split(' ').collect();
    let mut end = words.len();
    let is_unit =
        |w: &str, unit: &str| w == unit || w == format!("{unit}s") || w == format!("{unit}s,");
    let is_count = |w: &str| !w.is_empty() && w.chars().all(|c| c.is_ascii_digit());
    // seconds part
    if end >= 2 && is_unit(words[end - 1], "second") && is_count(words[end - 2]) {
        end -= 2;
    }
    // minutes part (it ends with a comma when seconds follow, and stands alone for exact minutes)
    if end >= 2
        && is_unit(words[end - 1].trim_end_matches(','), "minute")
        && is_count(words[end - 2])
    {
        end -= 2;
    }
    let title = words[..end].join(" ");
    (!title.trim().is_empty()).then(|| title.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn scopes_parse_in_any_case() {
        assert_eq!(Scope::parse("catalog"), Some(Scope::Catalog));
        assert_eq!(Scope::parse(" Library "), Some(Scope::Library));
        assert_eq!(Scope::parse("everything"), None);
        assert_eq!(Scope::default(), Scope::Catalog);
    }

    #[test]
    fn sections_name_their_kind() {
        assert_eq!(kind_from_section("Songs"), Kind::Song);
        assert_eq!(kind_from_section("Albums"), Kind::Album);
        assert_eq!(kind_from_section("Artists"), Kind::Artist);
        assert_eq!(kind_from_section("Playlists"), Kind::Playlist);
        assert_eq!(kind_from_section("Music Videos"), Kind::Video);
        for other in ["Profiles", "Composers", "Top Results", ""] {
            assert_eq!(kind_from_section(other), Kind::Other, "{other:?}");
        }
    }

    #[test]
    fn top_result_cards_are_told_apart_by_their_label() {
        // The label is separated by a narrow no-break space on the real machine.
        let song = parse_result(
            "Top Results",
            0,
            "Adu Rayu Song\u{202f}·\u{202f}Yovie Widianto, Tulus & Glenn Fredly",
            &parts(&[
                "Adu Rayu",
                "Song\u{202f}·\u{202f}Yovie Widianto, Tulus & Glenn Fredly",
            ]),
        )
        .unwrap();
        assert_eq!(
            (song.kind, song.title.as_str(), song.subtitle.as_str()),
            (
                Kind::Song,
                "Adu Rayu",
                "Yovie Widianto, Tulus & Glenn Fredly"
            )
        );
        let artist = parse_result(
            "Top Results",
            3,
            "Tulus Artist",
            &parts(&["Tulus", "Artist"]),
        )
        .unwrap();
        assert_eq!((artist.kind, artist.subtitle.as_str()), (Kind::Artist, ""));
        let playlist = parse_result(
            "Top Results",
            4,
            "x",
            &parts(&[
                "2010s Essentials",
                "Playlist\u{202f}·\u{202f}Apple Music Indonesian Music",
            ]),
        )
        .unwrap();
        assert_eq!(
            (playlist.kind, playlist.subtitle.as_str()),
            (Kind::Playlist, "Apple Music Indonesian Music")
        );
        let album = parse_result(
            "Top Results",
            1,
            "x",
            &parts(&["Single", "Album · Someone"]),
        )
        .unwrap();
        assert_eq!(album.kind, Kind::Album);
        assert_eq!(album.index, 1);
    }

    #[test]
    fn other_sections_take_their_kind_from_the_heading() {
        let album =
            parse_result("Albums", 2, "Manusia Tulus", &parts(&["Manusia", "Tulus"])).unwrap();
        assert_eq!(
            (
                album.kind,
                album.title.as_str(),
                album.subtitle.as_str(),
                album.index
            ),
            (Kind::Album, "Manusia", "Tulus", 2)
        );
        let artist = parse_result("Artists", 0, "Tulus", &parts(&["Tulus"])).unwrap();
        assert_eq!((artist.kind, artist.subtitle.as_str()), (Kind::Artist, ""));
        let other = parse_result(
            "Composers",
            0,
            "Muhammad Tulus",
            &parts(&["Muhammad Tulus"]),
        )
        .unwrap();
        assert_eq!(other.kind, Kind::Other);
    }

    #[test]
    fn a_library_song_row_skips_its_track_number_and_duration() {
        let row = parse_result(
            "Songs",
            0,
            "Track 4 Bunga Tidur 3 minutes, 32 seconds",
            &parts(&["4", "Bunga Tidur", "Tulus", "3:32"]),
        )
        .unwrap();
        assert_eq!(
            (row.kind, row.title.as_str(), row.subtitle.as_str()),
            (Kind::Song, "Bunga Tidur", "Tulus")
        );
        // Only the duration after the title: no subtitle.
        let bare = parse_result(
            "Songs",
            1,
            "Track 5 Other 1 minute",
            &parts(&["5", "Other", "1:00"]),
        )
        .unwrap();
        assert_eq!((bare.title.as_str(), bare.subtitle.as_str()), ("Other", ""));
    }

    #[test]
    fn without_parts_the_name_is_the_title() {
        let result = parse_result("Songs", 0, "Some Title", &[]).unwrap();
        assert_eq!(
            (result.title.as_str(), result.subtitle.as_str()),
            ("Some Title", "")
        );
        assert_eq!(parse_result("Songs", 0, "  ", &[]), None);
    }

    #[test]
    fn a_track_row_name_gives_its_title() {
        let cases = [
            ("Track 4 Bunga Tidur 3 minutes, 32 seconds", "Bunga Tidur"),
            (
                "Track 12 A Song With Many Words 1 minute, 1 second",
                "A Song With Many Words",
            ),
            ("Track 1 Short 45 seconds", "Short"),
            ("Track 3 Exactly 3 minutes", "Exactly"),
        ];
        for (name, title) in cases {
            assert_eq!(title_from_track_row(name).as_deref(), Some(title), "{name}");
        }
        for not_a_row in ["Bunga Tidur", "Track x Song 1 minute", "Track 5", ""] {
            assert_eq!(title_from_track_row(not_a_row), None, "{not_a_row:?}");
        }
    }

    #[test]
    fn results_serialise_like_the_contract() {
        let result = parse_result(
            "Songs",
            0,
            "Example Song",
            &parts(&["Example Song", "Example Artist"]),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            serde_json::json!({
                "section": "Songs", "index": 0, "kind": "song",
                "title": "Example Song", "subtitle": "Example Artist"
            })
        );
    }
}
