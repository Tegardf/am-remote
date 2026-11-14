//! Catalog search through the public iTunes Search API (no Apple account involved).
//!
//! `https://itunes.apple.com/search?term=...&entity=song&limit=...&country=...` for `GET /search`,
//! and `https://itunes.apple.com/lookup?id=<trackId>&country=...` to learn a song's title and artist
//! from the link a client sends to `POST /play/track` (the link alone has no title).
//! The traits let tests run without the network.

use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::media::links::parse_apple_music_url;

const SEARCH_URL: &str = "https://itunes.apple.com/search";
const LOOKUP_URL: &str = "https://itunes.apple.com/lookup";
const TIMEOUT: Duration = Duration::from_secs(8);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(4);
/// Used when the Windows region cannot be read.
const FALLBACK_STOREFRONT: &str = "us";

/// The contract's `SearchResult`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// `https` image URL.
    pub artwork_url: String,
    /// The song's `music.apple.com` link (iTunes' `trackViewUrl`).
    pub url: String,
}

/// What a song is called, learned from its id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackInfo {
    pub title: String,
    pub artist: String,
    /// The album (or single) the song is on, as the Apple Music page names it.
    pub album: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchError {
    /// Could not reach the service (no internet, timeout).
    Network(String),
    /// The service answered with something we cannot read.
    BadResponse(String),
}

impl fmt::Display for SearchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Network(detail) => write!(f, "cannot reach the Apple search service: {detail}"),
            Self::BadResponse(detail) => {
                write!(
                    f,
                    "the Apple search service answered unexpectedly: {detail}"
                )
            }
        }
    }
}

impl std::error::Error for SearchError {}

#[async_trait]
pub trait SearchBackend: Send + Sync {
    /// Songs matching `query`, at most `limit` of them.
    async fn search(&self, query: &str, limit: u8) -> Result<Vec<SearchResult>, SearchError>;

    /// Title and artist of the song with this id, if the catalog knows it. `storefront` is the
    /// two-letter country of the link, if it had one.
    async fn lookup_track(
        &self,
        track_id: &str,
        storefront: Option<&str>,
    ) -> Result<Option<TrackInfo>, SearchError>;
}

// ---------- iTunes JSON ----------

#[derive(Deserialize)]
struct ItunesResponse {
    #[serde(default)]
    results: Vec<ItunesItem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ItunesItem {
    kind: Option<String>,
    track_id: Option<u64>,
    track_name: Option<String>,
    artist_name: Option<String>,
    collection_name: Option<String>,
    track_view_url: Option<String>,
    artwork_url100: Option<String>,
}

fn items(json: &str) -> Result<Vec<ItunesItem>, SearchError> {
    serde_json::from_str::<ItunesResponse>(json)
        .map(|r| r.results)
        .map_err(|e| SearchError::BadResponse(e.to_string()))
}

/// `http://` image links become `https://`; the app must not load plain-http images.
fn https(url: &str) -> String {
    match url.strip_prefix("http://") {
        Some(rest) => format!("https://{rest}"),
        None => url.to_owned(),
    }
}

/// Maps the iTunes search JSON to the contract's results. Songs without an id, title or link are
/// skipped (they could not be played).
pub fn parse_search_response(json: &str) -> Result<Vec<SearchResult>, SearchError> {
    Ok(items(json)?
        .into_iter()
        .filter(|item| item.kind.as_deref() == Some("song"))
        .filter_map(|item| {
            let result = SearchResult {
                id: item.track_id?.to_string(),
                title: item.track_name?,
                artist: item.artist_name.unwrap_or_default(),
                album: item.collection_name.unwrap_or_default(),
                artwork_url: item.artwork_url100.map(|u| https(&u)).unwrap_or_default(),
                url: https(&item.track_view_url?),
            };
            // Only songs the agent could play later: a music.apple.com link with the song id.
            parse_apple_music_url(&result.url)
                .ok()
                .filter(|link| link.track_id.is_some())
                .map(|_| result)
        })
        .collect())
}

/// Maps the iTunes lookup JSON to the song it describes.
pub fn parse_lookup_response(json: &str) -> Result<Option<TrackInfo>, SearchError> {
    Ok(items(json)?
        .into_iter()
        .find(|item| item.kind.as_deref() == Some("song"))
        .and_then(|item| {
            Some(TrackInfo {
                title: item.track_name?,
                artist: item.artist_name?,
                album: item.collection_name.unwrap_or_default(),
            })
        }))
}

// ---------- storefront ----------

/// The two-letter country for the catalog: the region of a Windows locale name such as `en-ID`
/// or `zh-Hans-CN` (its last part), lower-cased. Anything else gives `us`.
pub fn storefront_from_locale(locale: &str) -> String {
    locale
        .rsplit('-')
        .next()
        .filter(|region| {
            locale.contains('-')
                && region.len() == 2
                && region.chars().all(|c| c.is_ascii_alphabetic())
        })
        .map(str::to_ascii_lowercase)
        .unwrap_or_else(|| FALLBACK_STOREFRONT.to_owned())
}

/// The country of this Windows installation's region settings.
pub fn system_storefront() -> String {
    use windows::Win32::Globalization::GetUserDefaultLocaleName;
    let mut buffer = [0u16; 85]; // LOCALE_NAME_MAX_LENGTH
                                 // SAFETY: the buffer is valid for the length passed.
    let length = unsafe { GetUserDefaultLocaleName(&mut buffer) };
    if length <= 1 {
        return FALLBACK_STOREFRONT.to_owned();
    }
    storefront_from_locale(&String::from_utf16_lossy(&buffer[..(length - 1) as usize]))
}

// ---------- the real client ----------

pub struct ItunesSearch {
    client: reqwest::Client,
    storefront: String,
}

impl ItunesSearch {
    /// `storefront`: the two-letter country whose catalog is searched.
    pub fn new(storefront: String) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .user_agent("am-remote-agent")
            .build()
            .context("cannot create the HTTP client")?;
        Ok(Self { client, storefront })
    }

    async fn get(&self, url: &str, query: &[(&str, &str)]) -> Result<String, SearchError> {
        let response = self
            .client
            .get(url)
            .query(query)
            .send()
            .await
            .map_err(|e| SearchError::Network(e.without_url().to_string()))?;
        if !response.status().is_success() {
            return Err(SearchError::BadResponse(format!(
                "HTTP {}",
                response.status()
            )));
        }
        response
            .text()
            .await
            .map_err(|e| SearchError::Network(e.without_url().to_string()))
    }
}

#[async_trait]
impl SearchBackend for ItunesSearch {
    async fn search(&self, query: &str, limit: u8) -> Result<Vec<SearchResult>, SearchError> {
        let limit = limit.to_string();
        let body = self
            .get(
                SEARCH_URL,
                &[
                    ("term", query),
                    ("entity", "song"),
                    ("media", "music"),
                    ("limit", &limit),
                    ("country", &self.storefront),
                ],
            )
            .await?;
        parse_search_response(&body)
    }

    async fn lookup_track(
        &self,
        track_id: &str,
        storefront: Option<&str>,
    ) -> Result<Option<TrackInfo>, SearchError> {
        let country = storefront.unwrap_or(&self.storefront);
        let body = self
            .get(LOOKUP_URL, &[("id", track_id), ("country", country)])
            .await?;
        parse_lookup_response(&body)
    }
}

// ---------- test double ----------

/// Test double: returns what it was given, remembers what it was asked, never touches the network.
#[derive(Default)]
pub struct FakeSearch {
    results: Mutex<Vec<SearchResult>>,
    tracks: Mutex<HashMap<String, TrackInfo>>,
    queries: Mutex<Vec<(String, u8)>>,
    offline: AtomicBool,
}

impl FakeSearch {
    pub fn set_results(&self, results: Vec<SearchResult>) {
        *self.results.lock().unwrap_or_else(|e| e.into_inner()) = results;
    }

    pub fn add_track(&self, track_id: &str, title: &str, artist: &str, album: &str) {
        self.tracks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(
                track_id.to_owned(),
                TrackInfo {
                    title: title.to_owned(),
                    artist: artist.to_owned(),
                    album: album.to_owned(),
                },
            );
    }

    /// Simulates no internet.
    pub fn set_offline(&self, offline: bool) {
        self.offline.store(offline, Ordering::SeqCst);
    }

    /// Every `(query, limit)` searched so far.
    pub fn queries(&self) -> Vec<(String, u8)> {
        self.queries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn check_online(&self) -> Result<(), SearchError> {
        if self.offline.load(Ordering::SeqCst) {
            Err(SearchError::Network("simulated: no internet".into()))
        } else {
            Ok(())
        }
    }
}

#[async_trait]
impl SearchBackend for FakeSearch {
    async fn search(&self, query: &str, limit: u8) -> Result<Vec<SearchResult>, SearchError> {
        self.check_online()?;
        self.queries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((query.to_owned(), limit));
        let results = self.results.lock().unwrap_or_else(|e| e.into_inner());
        Ok(results.iter().take(usize::from(limit)).cloned().collect())
    }

    async fn lookup_track(
        &self,
        track_id: &str,
        _storefront: Option<&str>,
    ) -> Result<Option<TrackInfo>, SearchError> {
        self.check_online()?;
        Ok(self
            .tracks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(track_id)
            .cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Real responses of the public iTunes API (saved 2026-11-14, country=id).
    const SEARCH_JSON: &str = include_str!("../tests/fixtures/itunes_search.json");
    const LOOKUP_JSON: &str = include_str!("../tests/fixtures/itunes_lookup.json");

    #[test]
    fn the_saved_search_response_maps_to_results() {
        let results = parse_search_response(SEARCH_JSON).unwrap();
        assert_eq!(results.len(), 3);
        let first = &results[0];
        assert_eq!(first.id, "6784585105");
        assert_eq!(first.title, "Teh Hijau");
        assert_eq!(first.artist, "Tulus");
        assert_eq!(first.album, "Teh Hijau - Single");
        assert_eq!(
            first.url,
            "https://music.apple.com/id/album/teh-hijau/6784585100?i=6784585105&uo=4"
        );
        assert!(
            first.artwork_url.starts_with("https://"),
            "{}",
            first.artwork_url
        );
        assert!(first.artwork_url.contains("mzstatic.com"));
    }

    #[test]
    fn every_result_has_all_fields_and_an_https_link() {
        for result in parse_search_response(SEARCH_JSON).unwrap() {
            for field in [
                &result.id,
                &result.title,
                &result.artist,
                &result.album,
                &result.url,
            ] {
                assert!(!field.is_empty(), "{result:?}");
            }
            assert!(result.url.starts_with("https://music.apple.com/"));
            assert!(result.artwork_url.starts_with("https://"));
        }
    }

    #[test]
    fn the_saved_lookup_response_gives_title_and_artist() {
        let info = parse_lookup_response(LOOKUP_JSON).unwrap().unwrap();
        assert_eq!(info.title, "Jatuh Suka");
        assert_eq!(info.artist, "Tulus");
        assert_eq!(info.album, "Manusia");
    }

    #[test]
    fn songs_with_links_the_agent_could_not_play_are_dropped() {
        let json = r#"{ "results": [
            { "kind": "song", "trackId": 1, "trackName": "Other Host", "artistName": "X",
              "trackViewUrl": "https://music.apple.com.evil.com/us/album/a/1?i=1" },
            { "kind": "song", "trackId": 2, "trackName": "No Song Id In Link", "artistName": "X",
              "trackViewUrl": "https://music.apple.com/us/album/a/1" },
            { "kind": "song", "trackId": 3, "trackName": "Fine", "artistName": "X",
              "trackViewUrl": "https://music.apple.com/us/album/a/1?i=3" }
        ] }"#;
        let results = parse_search_response(json).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Fine");
    }

    #[test]
    fn non_songs_and_incomplete_items_are_skipped() {
        let json = r#"{ "resultCount": 4, "results": [
            { "kind": "feature-movie", "trackId": 1, "trackName": "A Movie", "artistName": "X",
              "trackViewUrl": "https://music.apple.com/x" },
            { "kind": "song", "trackName": "No Id", "artistName": "X",
              "trackViewUrl": "https://music.apple.com/y" },
            { "kind": "song", "trackId": 3, "artistName": "X",
              "trackViewUrl": "https://music.apple.com/z" },
            { "kind": "song", "trackId": 4, "trackName": "No Link", "artistName": "X" }
        ] }"#;
        assert_eq!(parse_search_response(json).unwrap(), vec![]);
    }

    #[test]
    fn missing_optional_fields_become_empty_strings_and_http_becomes_https() {
        let json = r#"{ "results": [
            { "kind": "song", "trackId": 7, "trackName": "T",
              "trackViewUrl": "http://music.apple.com/us/album/t/1?i=7",
              "artworkUrl100": "http://is1-ssl.mzstatic.com/a/100x100bb.jpg" }
        ] }"#;
        let results = parse_search_response(json).unwrap();
        assert_eq!(results[0].artist, "");
        assert_eq!(results[0].album, "");
        assert_eq!(results[0].url, "https://music.apple.com/us/album/t/1?i=7");
        assert_eq!(
            results[0].artwork_url,
            "https://is1-ssl.mzstatic.com/a/100x100bb.jpg"
        );
    }

    #[test]
    fn an_empty_or_odd_response_is_handled() {
        assert_eq!(
            parse_search_response(r#"{"resultCount":0,"results":[]}"#).unwrap(),
            vec![]
        );
        assert_eq!(parse_search_response("{}").unwrap(), vec![]);
        assert!(matches!(
            parse_search_response("<html>nope</html>"),
            Err(SearchError::BadResponse(_))
        ));
        assert_eq!(parse_lookup_response(r#"{"results":[]}"#).unwrap(), None);
    }

    #[test]
    fn the_storefront_is_the_region_of_the_locale() {
        assert_eq!(storefront_from_locale("en-ID"), "id");
        assert_eq!(storefront_from_locale("en-US"), "us");
        assert_eq!(storefront_from_locale("zh-Hans-CN"), "cn");
        assert_eq!(storefront_from_locale("pt-BR"), "br");
        assert_eq!(storefront_from_locale("en"), "us");
        assert_eq!(storefront_from_locale(""), "us");
        assert_eq!(storefront_from_locale("en-001"), "us");
    }

    #[test]
    fn the_system_storefront_is_two_letters() {
        let storefront = system_storefront();
        assert_eq!(storefront.len(), 2, "{storefront}");
        assert!(storefront.chars().all(|c| c.is_ascii_lowercase()));
    }

    #[test]
    fn search_result_uses_the_contract_field_names() {
        let json = serde_json::to_value(&parse_search_response(SEARCH_JSON).unwrap()[0]).unwrap();
        let mut keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["album", "artist", "artwork_url", "id", "title", "url"]
        );
    }
}
