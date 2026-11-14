//! MVP 3: `/search`, `/playlists`, `/play/playlist` and `/play/track` through the real router with
//! the fake media backend and the fake search backend (no Apple Music, no network).

mod common;

use axum::http::{Method, StatusCode};
use serde_json::{json, Value};

use am_remote_agent::media::{PlayTrack, Playlist};
use am_remote_agent::search::{parse_search_response, SearchResult};
use common::*;

const SONG_URL: &str = "https://music.apple.com/id/album/jatuh-suka/1612163750?i=1612164263&uo=4";

fn result(n: u32) -> SearchResult {
    SearchResult {
        id: format!("{n}"),
        title: format!("Song {n}"),
        artist: "Example Artist".into(),
        album: "Example Album".into(),
        artwork_url: "https://is1-ssl.mzstatic.com/image/thumb/Example/100x100bb.jpg".into(),
        url: format!("https://music.apple.com/us/album/example-album/1?i={n}"),
    }
}

fn playlist(id: &str, name: &str) -> Playlist {
    Playlist {
        id: id.into(),
        name: name.into(),
    }
}

async fn get(h: &Harness, token: &str, path: &str) -> Reply {
    send(&h.app, Method::GET, path, Some(token), None).await
}

async fn post(h: &Harness, token: &str, path: &str, body: &str) -> Reply {
    send(&h.app, Method::POST, path, Some(token), Some(body)).await
}

// ---------- /search ----------

#[tokio::test]
async fn search_returns_the_results_in_the_sample_shape() {
    let h = harness();
    let token = paired_token(&h).await;
    h.search.set_results(vec![result(1), result(2)]);

    let reply = get(&h, &token, "/api/v1/search?q=example").await;
    assert_eq!(reply.status, StatusCode::OK);
    let body: Value = reply.json();
    assert_eq!(body.as_array().unwrap().len(), 2);
    assert_eq!(
        body[0],
        json!({
            "id": "1",
            "title": "Song 1",
            "artist": "Example Artist",
            "album": "Example Album",
            "artwork_url": "https://is1-ssl.mzstatic.com/image/thumb/Example/100x100bb.jpg",
            "url": "https://music.apple.com/us/album/example-album/1?i=1"
        })
    );
}

#[tokio::test]
async fn search_passes_the_query_trimmed_and_the_default_limit_of_25() {
    let h = harness();
    let token = paired_token(&h).await;

    get(&h, &token, "/api/v1/search?q=%20%20tulus%20").await;
    get(&h, &token, "/api/v1/search?q=tulus&limit=50").await;
    get(&h, &token, "/api/v1/search?q=tulus&limit=1").await;
    assert_eq!(
        h.search.queries(),
        vec![
            ("tulus".to_owned(), 25),
            ("tulus".to_owned(), 50),
            ("tulus".to_owned(), 1)
        ]
    );
}

/// Contract: a space in `q` may be sent as `+` or `%20`.
#[tokio::test]
async fn a_space_in_the_query_may_be_a_plus_or_percent_20() {
    let h = harness();
    let token = paired_token(&h).await;

    assert_eq!(
        get(&h, &token, "/api/v1/search?q=a+b").await.status,
        StatusCode::OK
    );
    assert_eq!(
        get(&h, &token, "/api/v1/search?q=a%20b").await.status,
        StatusCode::OK
    );
    assert_eq!(
        h.search.queries(),
        vec![("a b".to_owned(), 25), ("a b".to_owned(), 25)]
    );
}

#[tokio::test]
async fn search_with_a_bad_query_or_limit_is_400_and_never_reaches_the_service() {
    let h = harness();
    let token = paired_token(&h).await;

    for path in [
        "/api/v1/search",
        "/api/v1/search?q=",
        "/api/v1/search?q=a",
        "/api/v1/search?q=%20a%20",
        "/api/v1/search?limit=10",
        "/api/v1/search?q=tulus&limit=0",
        "/api/v1/search?q=tulus&limit=51",
        "/api/v1/search?q=tulus&limit=-1",
        "/api/v1/search?q=tulus&limit=1000",
        "/api/v1/search?q=tulus&limit=many",
    ] {
        let reply = get(&h, &token, path).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{path}");
        assert_eq!(reply.error_code(), "BAD_REQUEST", "{path}");
    }
    let too_long = format!("/api/v1/search?q={}", "a".repeat(201));
    assert_eq!(
        get(&h, &token, &too_long).await.status,
        StatusCode::BAD_REQUEST
    );
    assert!(h.search.queries().is_empty());
}

#[tokio::test]
async fn two_letters_are_enough_to_search() {
    let h = harness();
    let token = paired_token(&h).await;
    assert_eq!(
        get(&h, &token, "/api/v1/search?q=ab").await.status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn search_with_no_internet_is_a_clear_500_and_the_agent_goes_on() {
    let h = harness();
    let token = paired_token(&h).await;
    h.search.set_offline(true);

    let reply = get(&h, &token, "/api/v1/search?q=tulus").await;
    assert_eq!(reply.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(reply.error_code(), "INTERNAL");
    let message = reply.json::<Value>()["error"]["message"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(message.contains("internet"), "{message}");
    assert!(
        !message.contains("simulated"),
        "no internals in the message: {message}"
    );

    // Back online: the same agent works again.
    h.search.set_offline(false);
    assert_eq!(
        get(&h, &token, "/api/v1/search?q=tulus").await.status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn search_works_while_apple_music_is_closed() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_running(false);
    assert_eq!(
        get(&h, &token, "/api/v1/search?q=tulus").await.status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn an_empty_search_is_an_empty_list() {
    let h = harness();
    let token = paired_token(&h).await;
    let reply = get(&h, &token, "/api/v1/search?q=zzzzzz").await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.json::<Value>(), json!([]));
}

/// The saved real iTunes answer goes all the way to the wire.
#[tokio::test]
async fn real_itunes_data_survives_the_endpoint() {
    let h = harness();
    let token = paired_token(&h).await;
    let fixture = include_str!("fixtures/itunes_search.json");
    h.search
        .set_results(parse_search_response(fixture).unwrap());

    let reply = get(&h, &token, "/api/v1/search?q=tulus&limit=2").await;
    let results: Vec<SearchResult> = reply.json();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].artist, "Tulus");
    assert!(results
        .iter()
        .all(|r| r.url.starts_with("https://music.apple.com/")));
}

// ---------- /playlists ----------

#[tokio::test]
async fn playlists_are_listed_as_id_and_name() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_playlists(vec![
        playlist("pl-1", "Favourites"),
        playlist("pl-2", "Road Trip"),
    ]);

    let reply = get(&h, &token, "/api/v1/playlists").await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(
        reply.json::<Value>(),
        json!([{ "id": "pl-1", "name": "Favourites" }, { "id": "pl-2", "name": "Road Trip" }])
    );
}

#[tokio::test]
async fn no_playlists_is_an_empty_list() {
    let h = harness();
    let token = paired_token(&h).await;
    let reply = get(&h, &token, "/api/v1/playlists").await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.json::<Value>(), json!([]));
}

/// The `uia` feature off, or Apple Music's UI changed: a clear 501, never a crash or a hang.
#[tokio::test]
async fn playlists_without_ui_automation_are_501_not_supported() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_uia_supported(false);

    let reply = get(&h, &token, "/api/v1/playlists").await;
    assert_eq!(reply.status, StatusCode::NOT_IMPLEMENTED);
    assert_eq!(reply.error_code(), "NOT_SUPPORTED");
    let message = reply.json::<Value>()["error"]["message"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        message.contains("UI Automation"),
        "helpful message: {message}"
    );
}

#[tokio::test]
async fn playlists_are_409_when_apple_music_is_not_running() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_running(false);
    let reply = get(&h, &token, "/api/v1/playlists").await;
    assert_eq!(reply.status, StatusCode::CONFLICT);
    assert_eq!(reply.error_code(), "APPLE_MUSIC_NOT_RUNNING");
}

// ---------- /play/playlist ----------

#[tokio::test]
async fn a_playlist_is_started_by_its_id() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_playlists(vec![playlist("pl-1", "Favourites")]);

    let reply = post(&h, &token, "/api/v1/play/playlist", r#"{"id":"pl-1"}"#).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
    assert!(reply.body.is_empty());
    assert_eq!(h.media.played_playlists(), vec!["pl-1".to_owned()]);
}

#[tokio::test]
async fn ids_with_colons_pass_through_unchanged() {
    let h = harness();
    let token = paired_token(&h).await;
    let id = "DBID:0x1-PID:0x2-PPID:0x0-PIPID:0x0-IKIND:ePlaylist";
    h.media.set_playlists(vec![playlist(id, "Real Shaped")]);

    let body = json!({ "id": id }).to_string();
    let reply = post(&h, &token, "/api/v1/play/playlist", &body).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
    assert_eq!(h.media.played_playlists(), vec![id.to_owned()]);
}

#[tokio::test]
async fn an_unknown_playlist_id_is_404_not_found() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_playlists(vec![playlist("pl-1", "Favourites")]);

    let reply = post(&h, &token, "/api/v1/play/playlist", r#"{"id":"gone"}"#).await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
    assert_eq!(reply.error_code(), "NOT_FOUND");
    assert!(h.media.played_playlists().is_empty());
}

#[tokio::test]
async fn bad_playlist_requests_are_400() {
    let h = harness();
    let token = paired_token(&h).await;
    for body in [
        "{}",
        r#"{"id":""}"#,
        r#"{"id":"   "}"#,
        r#"{"id":5}"#,
        "not json",
    ] {
        let reply = post(&h, &token, "/api/v1/play/playlist", body).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(reply.error_code(), "BAD_REQUEST", "{body}");
    }
}

#[tokio::test]
async fn playing_a_playlist_without_ui_automation_or_apple_music_is_a_clear_error() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_playlists(vec![playlist("pl-1", "Favourites")]);

    h.media.set_uia_supported(false);
    let off = post(&h, &token, "/api/v1/play/playlist", r#"{"id":"pl-1"}"#).await;
    assert_eq!(off.status, StatusCode::NOT_IMPLEMENTED);
    assert_eq!(off.error_code(), "NOT_SUPPORTED");

    h.media.set_uia_supported(true);
    h.media.set_running(false);
    let closed = post(&h, &token, "/api/v1/play/playlist", r#"{"id":"pl-1"}"#).await;
    assert_eq!(closed.status, StatusCode::CONFLICT);
    assert!(h.media.played_playlists().is_empty());
}

// ---------- /play/track ----------

fn song_body(url: &str) -> String {
    json!({ "url": url }).to_string()
}

#[tokio::test]
async fn a_search_result_can_be_played() {
    let h = harness();
    let token = paired_token(&h).await;
    h.search
        .add_track("1612164263", "Jatuh Suka", "Tulus", "Manusia");

    let reply = post(&h, &token, "/api/v1/play/track", &song_body(SONG_URL)).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
    assert!(reply.body.is_empty());
    assert_eq!(
        h.media.played_tracks(),
        vec![PlayTrack {
            open_url: "music://music.apple.com/id/album/jatuh-suka/1612163750?i=1612164263".into(),
            title: "Jatuh Suka".into(),
            artist: "Tulus".into(),
            album: "Manusia".into(),
        }]
    );
}

#[tokio::test]
async fn only_music_apple_com_links_are_accepted() {
    let h = harness();
    let token = paired_token(&h).await;
    h.search.add_track("2", "Song", "Artist", "Album");

    for url in [
        "https://evil.com/id/album/x/1?i=2",
        "https://music.apple.com.evil.com/id/album/x/1?i=2",
        "https://evil.com/music.apple.com/id/album/x/1?i=2",
        "http://music.apple.com/id/album/x/1?i=2",
        "music://music.apple.com/id/album/x/1?i=2",
        "file:///C:/Windows/System32/calc.exe",
        "javascript:alert(1)",
        "https://user:pw@music.apple.com/id/album/x/1?i=2",
        "not a url",
        "",
    ] {
        let reply = post(&h, &token, "/api/v1/play/track", &song_body(url)).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{url:?}");
        assert_eq!(reply.error_code(), "BAD_REQUEST", "{url:?}");
    }
    assert!(
        h.media.played_tracks().is_empty(),
        "nothing reached Apple Music"
    );
}

#[tokio::test]
async fn a_link_that_is_not_a_song_is_400() {
    let h = harness();
    let token = paired_token(&h).await;
    for url in [
        "https://music.apple.com/id/album/manusia/1612163750",
        "https://music.apple.com/us/playlist/todays-hits/pl.f4d106fed2bd41149aaacabb233eb5eb",
        "https://music.apple.com/id/album/x/1?i=abc",
    ] {
        let reply = post(&h, &token, "/api/v1/play/track", &song_body(url)).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{url}");
    }
    assert!(h.media.played_tracks().is_empty());
}

#[tokio::test]
async fn a_bad_track_body_is_400() {
    let h = harness();
    let token = paired_token(&h).await;
    for body in ["{}", r#"{"url":5}"#, "not json", ""] {
        let reply = post(&h, &token, "/api/v1/play/track", body).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{body:?}");
        assert_eq!(reply.error_code(), "BAD_REQUEST");
    }
}

#[tokio::test]
async fn playing_a_song_while_apple_music_is_closed_is_409() {
    let h = harness();
    let token = paired_token(&h).await;
    h.search
        .add_track("1612164263", "Jatuh Suka", "Tulus", "Manusia");
    h.media.set_running(false);

    let reply = post(&h, &token, "/api/v1/play/track", &song_body(SONG_URL)).await;
    assert_eq!(reply.status, StatusCode::CONFLICT);
    assert_eq!(reply.error_code(), "APPLE_MUSIC_NOT_RUNNING");
}

#[tokio::test]
async fn a_song_the_catalog_does_not_know_is_404() {
    let h = harness();
    let token = paired_token(&h).await;
    let reply = post(&h, &token, "/api/v1/play/track", &song_body(SONG_URL)).await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
    assert_eq!(reply.error_code(), "NOT_FOUND");
    assert!(h.media.played_tracks().is_empty());
}

#[tokio::test]
async fn playing_a_song_with_no_internet_is_a_clear_500() {
    let h = harness();
    let token = paired_token(&h).await;
    h.search.set_offline(true);
    let reply = post(&h, &token, "/api/v1/play/track", &song_body(SONG_URL)).await;
    assert_eq!(reply.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(reply.error_code(), "INTERNAL");
}

/// Contract C-004, order of checks: `401` -> `400` -> `409`/`501` -> `404`. An invalid request
/// answers `400` even when Apple Music is closed, and a blank playlist id is `400`, not `404`.
#[tokio::test]
async fn a_bad_request_is_400_even_when_apple_music_is_closed() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_running(false);

    for (path, body) in [
        ("/api/v1/play/playlist", r#"{"id":""}"#),
        ("/api/v1/play/playlist", r#"{"id":"   "}"#),
        ("/api/v1/play/playlist", r#"{}"#),
        (
            "/api/v1/play/track",
            r#"{"url":"https://evil.example/x?i=1"}"#,
        ),
        (
            "/api/v1/play/track",
            r#"{"url":"https://music.apple.com/us/album/x/1"}"#,
        ),
        ("/api/v1/play/track", r#"{}"#),
    ] {
        let reply = post(&h, &token, path, body).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{path} {body}");
        assert_eq!(reply.error_code(), "BAD_REQUEST", "{path} {body}");
    }
    // A request with no body at all is also a 400.
    let no_body = send(
        &h.app,
        Method::POST,
        "/api/v1/play/playlist",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(no_body.status, StatusCode::BAD_REQUEST);
    assert_eq!(no_body.error_code(), "BAD_REQUEST");
    // A valid one is the 409 it always was.
    let valid = post(&h, &token, "/api/v1/play/playlist", r#"{"id":"pl-1"}"#).await;
    assert_eq!(valid.status, StatusCode::CONFLICT);
}

/// "Not supported" wins over "not running" and over "no internet", and costs no network call.
#[tokio::test]
async fn without_ui_automation_the_answer_is_501_even_when_closed_or_offline() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_uia_supported(false);
    h.media.set_running(false);
    h.search.set_offline(true);

    let song = post(&h, &token, "/api/v1/play/track", &song_body(SONG_URL)).await;
    assert_eq!(song.status, StatusCode::NOT_IMPLEMENTED);
    assert_eq!(song.error_code(), "NOT_SUPPORTED");
    let playlist = post(&h, &token, "/api/v1/play/playlist", r#"{"id":"pl-1"}"#).await;
    assert_eq!(playlist.status, StatusCode::NOT_IMPLEMENTED);
    let list = get(&h, &token, "/api/v1/playlists").await;
    assert_eq!(list.status, StatusCode::NOT_IMPLEMENTED);
}

#[tokio::test]
async fn playing_a_song_without_ui_automation_is_501() {
    let h = harness();
    let token = paired_token(&h).await;
    h.search
        .add_track("1612164263", "Jatuh Suka", "Tulus", "Manusia");
    h.media.set_uia_supported(false);

    let reply = post(&h, &token, "/api/v1/play/track", &song_body(SONG_URL)).await;
    assert_eq!(reply.status, StatusCode::NOT_IMPLEMENTED);
    assert_eq!(reply.error_code(), "NOT_SUPPORTED");
}

#[tokio::test]
async fn all_four_endpoints_need_a_token_and_the_right_method() {
    let h = harness();
    let token = paired_token(&h).await;
    for (method, path) in [
        (Method::GET, "/api/v1/search?q=tulus"),
        (Method::GET, "/api/v1/playlists"),
        (Method::POST, "/api/v1/play/playlist"),
        (Method::POST, "/api/v1/play/track"),
    ] {
        let reply = send(&h.app, method.clone(), path, None, Some("{}")).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{method} {path}");
    }
    for (wrong, path) in [
        (Method::POST, "/api/v1/search?q=tulus"),
        (Method::POST, "/api/v1/playlists"),
        (Method::GET, "/api/v1/play/playlist"),
        (Method::GET, "/api/v1/play/track"),
    ] {
        let reply = send(&h.app, wrong.clone(), path, Some(&token), Some("{}")).await;
        assert_eq!(
            reply.status,
            StatusCode::METHOD_NOT_ALLOWED,
            "{wrong} {path}"
        );
    }
}
