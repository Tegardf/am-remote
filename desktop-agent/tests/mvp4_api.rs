//! MVP 4 (contract C-006): shuffle and repeat, the queue and app search through the real router
//! with the fake `MediaBackend`. Order of checks: 401 -> 400 -> 501 -> 409 -> 404.

mod common;

use axum::http::{Method, StatusCode};
use serde_json::{json, Value};

use am_remote_agent::media::appsearch::Kind;
use am_remote_agent::media::queue::QueueSource;
use am_remote_agent::media::{AppSearchResult, Modes, QueueItem, Repeat, Scope};
use common::*;

async fn get(h: &Harness, token: &str, path: &str) -> Reply {
    send(&h.app, Method::GET, path, Some(token), None).await
}

async fn post(h: &Harness, token: &str, path: &str, body: &str) -> Reply {
    send(&h.app, Method::POST, path, Some(token), Some(body)).await
}

fn row(index: u32, title: &str, source: QueueSource) -> QueueItem {
    QueueItem {
        index,
        title: title.into(),
        artist: "Artist".into(),
        album: "Album".into(),
        duration_ms: 200_000,
        source,
    }
}

fn result(section: &str, index: u32, kind: Kind, title: &str) -> AppSearchResult {
    AppSearchResult {
        section: section.into(),
        index,
        kind,
        title: title.into(),
        subtitle: "Someone".into(),
    }
}

/// Every new route with a valid body, for the checks that apply to all of them.
const ROUTES: [(Method, &str, Option<&str>); 6] = [
    (Method::GET, "/api/v1/player/modes", None),
    (
        Method::POST,
        "/api/v1/player/modes",
        Some(r#"{"shuffle":true}"#),
    ),
    (Method::GET, "/api/v1/queue", None),
    (
        Method::POST,
        "/api/v1/queue/play",
        Some(r#"{"index":0,"title":"A"}"#),
    ),
    (Method::GET, "/api/v1/app-search?q=ab", None),
    (
        Method::POST,
        "/api/v1/app-search/play",
        Some(r#"{"q":"ab","scope":"library","section":"Songs","index":0,"title":"A"}"#),
    ),
];

#[tokio::test]
async fn every_route_needs_a_token() {
    let h = harness();
    for (method, path, body) in ROUTES {
        let reply = send(&h.app, method.clone(), path, None, body).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{method} {path}");
        assert_eq!(reply.error_code(), "UNAUTHORIZED");
    }
}

/// Without UI Automation everything answers 501 before anything else is looked at.
#[tokio::test]
async fn without_ui_automation_all_six_are_501() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_uia_supported(false);
    h.media.set_running(false); // 501 wins over 409
    for (method, path, body) in ROUTES {
        let reply = send(&h.app, method.clone(), path, Some(&token), body).await;
        assert_eq!(reply.status, StatusCode::NOT_IMPLEMENTED, "{method} {path}");
        assert_eq!(reply.error_code(), "NOT_SUPPORTED", "{method} {path}");
    }
}

#[tokio::test]
async fn when_apple_music_is_closed_all_six_are_409() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_running(false);
    for (method, path, body) in ROUTES {
        let reply = send(&h.app, method.clone(), path, Some(&token), body).await;
        assert_eq!(reply.status, StatusCode::CONFLICT, "{method} {path}");
        assert_eq!(
            reply.error_code(),
            "APPLE_MUSIC_NOT_RUNNING",
            "{method} {path}"
        );
    }
}

// ---------- modes ----------

#[tokio::test]
async fn modes_are_read_and_set() {
    let h = harness();
    let token = paired_token(&h).await;

    let read = get(&h, &token, "/api/v1/player/modes").await;
    assert_eq!(read.status, StatusCode::OK);
    assert_eq!(
        read.json::<Value>(),
        json!({ "shuffle": false, "repeat": "off" })
    );

    let set = post(
        &h,
        &token,
        "/api/v1/player/modes",
        r#"{"shuffle":true,"repeat":"one"}"#,
    )
    .await;
    assert_eq!(set.status, StatusCode::OK);
    assert_eq!(
        set.json::<Value>(),
        json!({ "shuffle": true, "repeat": "one" })
    );
    // One field leaves the other alone; `{}` and explicit nulls just read.
    let only_repeat = post(&h, &token, "/api/v1/player/modes", r#"{"repeat":"all"}"#).await;
    assert_eq!(
        only_repeat.json::<Value>(),
        json!({ "shuffle": true, "repeat": "all" })
    );
    for body in ["{}", r#"{"shuffle":null,"repeat":null}"#] {
        let reply = post(&h, &token, "/api/v1/player/modes", body).await;
        assert_eq!(reply.status, StatusCode::OK, "{body}");
        assert_eq!(
            reply.json::<Modes>(),
            Modes {
                shuffle: true,
                repeat: Repeat::All
            }
        );
    }
}

#[tokio::test]
async fn bad_modes_bodies_are_400_before_anything_else() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_uia_supported(false); // would be 501
    h.media.set_running(false); // would be 409
    for body in [
        r#"{"repeat":"sometimes"}"#,
        r#"{"repeat":1}"#,
        r#"{"shuffle":"yes"}"#,
        r#"{"shuffle":1}"#,
        "not json",
        "",
        "[]",
    ] {
        let reply = post(&h, &token, "/api/v1/player/modes", body).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{body:?}");
        assert_eq!(reply.error_code(), "BAD_REQUEST", "{body:?}");
    }
}

// ---------- queue ----------

#[tokio::test]
async fn the_queue_is_listed_in_the_sample_shape() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_queue(vec![
        row(0, "First", QueueSource::Queue),
        row(1, "Second", QueueSource::Autoplay),
    ]);
    let reply = get(&h, &token, "/api/v1/queue").await;
    assert_eq!(reply.status, StatusCode::OK);
    let body: Value = reply.json();
    assert_eq!(body["items"].as_array().unwrap().len(), 2);
    assert_eq!(body["items"][0]["title"], "First");
    assert_eq!(body["items"][0]["source"], "queue");
    assert_eq!(body["items"][1]["source"], "autoplay");

    h.media.set_queue(vec![]);
    let empty = get(&h, &token, "/api/v1/queue").await;
    assert_eq!(empty.json::<Value>(), json!({ "items": [] }));
}

#[tokio::test]
async fn a_queue_row_is_played_by_index_and_title() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_queue(vec![
        row(0, "First", QueueSource::Queue),
        row(1, "Second", QueueSource::Queue),
    ]);
    let reply = post(
        &h,
        &token,
        "/api/v1/queue/play",
        r#"{"index":1,"title":"Second"}"#,
    )
    .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
    assert!(reply.body.is_empty());
    assert_eq!(h.media.queue_plays(), vec![1]);
}

/// The queue moved on since the client read it: refuse instead of skipping to the wrong song.
#[tokio::test]
async fn a_queue_that_moved_is_404_and_nothing_is_skipped() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media
        .set_queue(vec![row(0, "Now First", QueueSource::Queue)]);
    for body in [
        r#"{"index":0,"title":"What It Was Before"}"#,
        r#"{"index":5,"title":"Now First"}"#,
    ] {
        let reply = post(&h, &token, "/api/v1/queue/play", body).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{body}");
        assert_eq!(reply.error_code(), "NOT_FOUND");
    }
    assert!(h.media.queue_plays().is_empty());
}

/// Skips that end on a different song are a 500, not a success.
#[tokio::test]
async fn landing_on_the_wrong_song_is_500() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media
        .set_queue(vec![row(0, "Target", QueueSource::Queue)]);
    h.media.set_queue_lands_wrong(true);
    let reply = post(
        &h,
        &token,
        "/api/v1/queue/play",
        r#"{"index":0,"title":"Target"}"#,
    )
    .await;
    assert_eq!(reply.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(reply.error_code(), "INTERNAL");
}

#[tokio::test]
async fn bad_queue_play_bodies_are_400_before_anything_else() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_uia_supported(false);
    h.media.set_running(false);
    for body in [
        r#"{"index":-1,"title":"A"}"#,
        r#"{"index":0,"title":""}"#,
        r#"{"index":0,"title":"   "}"#,
        "[]",
        r#"[0,"A"]"#,
        r#"{"index":"0","title":"A"}"#,
        r#"{"index":1.5,"title":"A"}"#,
        r#"{"title":"A"}"#,
        r#"{"index":0}"#,
        "{}",
        "",
    ] {
        let reply = post(&h, &token, "/api/v1/queue/play", body).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{body:?}");
        assert_eq!(reply.error_code(), "BAD_REQUEST");
    }
}

// ---------- app search ----------

#[tokio::test]
async fn app_search_returns_the_results_of_the_asked_scope() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_app_results(
        Scope::Catalog,
        vec![result("Songs", 0, Kind::Song, "From The Catalog")],
    );
    h.media.set_app_results(
        Scope::Library,
        vec![result("Songs", 0, Kind::Song, "From The Library")],
    );

    let default_scope = get(&h, &token, "/api/v1/app-search?q=%20example%20").await;
    assert_eq!(default_scope.status, StatusCode::OK);
    assert_eq!(
        default_scope.json::<Value>()["results"][0]["title"],
        "From The Catalog"
    );
    let library = get(&h, &token, "/api/v1/app-search?q=example&scope=library").await;
    assert_eq!(
        library.json::<Value>()["results"][0]["title"],
        "From The Library"
    );
    assert_eq!(
        h.media.app_searches(),
        vec![
            ("example".to_owned(), Scope::Catalog),
            ("example".to_owned(), Scope::Library)
        ],
        "q is trimmed and the default scope is the catalog"
    );
}

#[tokio::test]
async fn no_match_is_200_with_an_empty_list() {
    let h = harness();
    let token = paired_token(&h).await;
    let reply = get(&h, &token, "/api/v1/app-search?q=nothing%20here").await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.json::<Value>(), json!({ "results": [] }));
}

#[tokio::test]
async fn bad_app_search_queries_are_400_before_anything_else() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_uia_supported(false);
    h.media.set_running(false);
    let too_long = format!("/api/v1/app-search?q={}", "a".repeat(201));
    for path in [
        "/api/v1/app-search",
        "/api/v1/app-search?q=",
        "/api/v1/app-search?q=a",
        "/api/v1/app-search?q=%20a%20",
        "/api/v1/app-search?q=ab&scope=everywhere",
        "/api/v1/app-search?q=ab&scope=",
        too_long.as_str(),
    ] {
        let reply = get(&h, &token, path).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{path}");
        assert_eq!(reply.error_code(), "BAD_REQUEST", "{path}");
    }
}

#[tokio::test]
async fn a_search_result_is_played_by_section_index_and_title() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_app_results(
        Scope::Library,
        vec![
            result("Songs", 0, Kind::Song, "First Song"),
            result("Songs", 1, Kind::Song, "Second Song"),
        ],
    );
    let body = json!({
        "q": "example", "scope": "library", "section": "Songs", "index": 1, "title": "Second Song"
    })
    .to_string();
    let reply = post(&h, &token, "/api/v1/app-search/play", &body).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
    assert!(reply.body.is_empty());
    let plays = h.media.app_plays();
    assert_eq!(plays.len(), 1);
    assert_eq!(
        (
            plays[0].section.as_str(),
            plays[0].index,
            plays[0].title.as_str()
        ),
        ("Songs", 1, "Second Song")
    );
}

#[tokio::test]
async fn a_result_that_moved_is_404() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_app_results(
        Scope::Library,
        vec![result("Songs", 0, Kind::Song, "Other Song")],
    );
    for (section, index, title) in [
        ("Songs", 0, "Song That Was There"),
        ("Songs", 7, "Other Song"),
        ("Albums", 0, "Other Song"),
    ] {
        let body = json!({
            "q": "example", "scope": "library", "section": section, "index": index, "title": title
        })
        .to_string();
        let reply = post(&h, &token, "/api/v1/app-search/play", &body).await;
        assert_eq!(
            reply.status,
            StatusCode::NOT_FOUND,
            "{section} {index} {title}"
        );
    }
    assert!(h.media.app_plays().is_empty());
}

/// First version: library songs, catalog songs and catalog albums play; the rest is 501.
#[tokio::test]
async fn kinds_that_cannot_be_started_yet_are_501() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_app_results(
        Scope::Catalog,
        vec![
            result("Songs", 0, Kind::Song, "A Song"),
            result("Albums", 0, Kind::Album, "An Album"),
            result("Artists", 0, Kind::Artist, "An Artist"),
            result("Playlists", 0, Kind::Playlist, "A Playlist"),
        ],
    );
    h.media.set_app_results(
        Scope::Library,
        vec![result("Albums", 0, Kind::Album, "A Library Album")],
    );
    let play = |scope: &str, section: &str, title: &str| {
        json!({
            "q": "example", "scope": scope, "section": section, "index": 0, "title": title
        })
        .to_string()
    };
    for (body, expected) in [
        (play("catalog", "Songs", "A Song"), StatusCode::NO_CONTENT),
        (
            play("catalog", "Albums", "An Album"),
            StatusCode::NO_CONTENT,
        ),
        (
            play("catalog", "Artists", "An Artist"),
            StatusCode::NOT_IMPLEMENTED,
        ),
        (
            play("catalog", "Playlists", "A Playlist"),
            StatusCode::NOT_IMPLEMENTED,
        ),
        (
            play("library", "Albums", "A Library Album"),
            StatusCode::NOT_IMPLEMENTED,
        ),
    ] {
        let reply = post(&h, &token, "/api/v1/app-search/play", &body).await;
        assert_eq!(reply.status, expected, "{body}");
    }
}

#[tokio::test]
async fn bad_app_search_play_bodies_are_400_before_anything_else() {
    let h = harness();
    let token = paired_token(&h).await;
    h.media.set_uia_supported(false);
    h.media.set_running(false);
    let good = json!({
        "q": "example", "scope": "library", "section": "Songs", "index": 0, "title": "A"
    });
    let mut cases: Vec<String> = vec![
        "{}".into(),
        "".into(),
        "[]".into(),
        r#"["example","library","Songs",0,"A"]"#.into(),
    ];
    for (field, bad) in [
        ("q", json!("a")),
        ("q", json!("   ")),
        ("q", json!("a".repeat(201))),
        ("scope", json!("everywhere")),
        ("scope", json!(1)),
        ("section", json!("")),
        ("section", json!("  ")),
        ("index", json!(-1)),
        ("index", json!("0")),
        ("title", json!("")),
        ("title", json!(" ")),
    ] {
        let mut body = good.clone();
        body[field] = bad;
        cases.push(body.to_string());
    }
    // Each required field missing on its own.
    for field in ["q", "scope", "section", "index", "title"] {
        let mut body = good.clone();
        body.as_object_mut().unwrap().remove(field);
        cases.push(body.to_string());
    }
    for body in cases {
        let reply = post(&h, &token, "/api/v1/app-search/play", &body).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(reply.error_code(), "BAD_REQUEST", "{body}");
    }
}

/// A request with no body at all, or the wrong content type, is a 400 on the new POST routes too.
#[tokio::test]
async fn missing_bodies_are_400_on_the_new_post_routes() {
    let h = harness();
    let token = paired_token(&h).await;
    for path in [
        "/api/v1/player/modes",
        "/api/v1/queue/play",
        "/api/v1/app-search/play",
    ] {
        let reply = send(&h.app, Method::POST, path, Some(&token), None).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{path}");
        assert_eq!(reply.error_code(), "BAD_REQUEST", "{path}");
    }
}

#[tokio::test]
async fn wrong_methods_are_405() {
    let h = harness();
    let token = paired_token(&h).await;
    for (method, path) in [
        (Method::DELETE, "/api/v1/player/modes"),
        (Method::POST, "/api/v1/queue"),
        (Method::GET, "/api/v1/queue/play"),
        (Method::POST, "/api/v1/app-search"),
        (Method::GET, "/api/v1/app-search/play"),
    ] {
        let reply = send(&h.app, method.clone(), path, Some(&token), None).await;
        assert_eq!(
            reply.status,
            StatusCode::METHOD_NOT_ALLOWED,
            "{method} {path}"
        );
        assert_eq!(reply.error_code(), "METHOD_NOT_ALLOWED");
    }
}

/// Every JSON body of the API is an object: an array (which serde could read by position) is a
/// `400` on the older POST routes too.
#[tokio::test]
async fn array_bodies_are_400_on_the_older_post_routes() {
    let h = harness();
    let token = paired_token(&h).await;
    for (path, body) in [
        ("/api/v1/volume", "[50]"),
        ("/api/v1/player/seek", "[1000]"),
        ("/api/v1/play/playlist", r#"["pl-1"]"#),
        (
            "/api/v1/play/track",
            r#"["https://music.apple.com/us/album/x/1?i=2"]"#,
        ),
    ] {
        let reply = post(&h, &token, path, body).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{path} {body}");
        assert_eq!(reply.error_code(), "BAD_REQUEST");
    }
}
