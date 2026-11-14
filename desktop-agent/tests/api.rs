//! HTTP-level tests: drive the real router in memory (no sockets, no Apple Music, no tray).

mod common;

use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

use am_remote_agent::api::pair::PairResponse;
use am_remote_agent::api::StatusResponse;
use am_remote_agent::config::ConfigStore;
use common::*;

// ---------- /pair ----------

#[tokio::test]
async fn right_code_returns_a_token_and_rotates_the_code() {
    let h = harness();
    let old_code = h.state.pairing_code();

    let reply = pair_with(&h, &old_code).await;
    assert_eq!(reply.status, StatusCode::OK);
    let token = reply.json::<PairResponse>().token;
    assert_eq!(token.len(), 43);

    let new_code = h.state.pairing_code();
    assert_ne!(new_code, old_code, "the code is single use");
    assert_eq!(pair_with(&h, &old_code).await.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn pairing_saves_the_device_to_the_config_file() {
    let h = harness();
    let token = paired_token(&h).await;

    let reopened = ConfigStore::open(h.config_path.clone()).unwrap();
    let devices = reopened.devices();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].name, "pc-test");
    assert_eq!(devices[0].token, token);
}

/// Contract C-002: `device_name` is trimmed, cut to 64 characters, and empty becomes
/// "Unknown device". It is never an error.
#[tokio::test]
async fn device_names_are_cleaned_when_pairing() {
    let h = harness();
    let long = "x".repeat(200);
    let cases = [
        ("  Pixel 8  ", "Pixel 8".to_owned()),
        ("", "Unknown device".to_owned()),
        ("   ", "Unknown device".to_owned()),
        (long.as_str(), "x".repeat(64)),
    ];
    for (sent, expected) in cases {
        let body = serde_json::json!({ "code": h.state.pairing_code(), "device_name": sent });
        let reply = send(
            &h.app,
            Method::POST,
            "/api/v1/pair",
            None,
            Some(&body.to_string()),
        )
        .await;
        assert_eq!(reply.status, StatusCode::OK, "{sent:?}");
        let stored = h.state.config.devices().pop().expect("a device was saved");
        assert_eq!(stored.name, expected, "{sent:?}");
    }
    assert_eq!(h.state.config.devices().len(), 4);
}

/// Contract C-004: `device_name` is optional. Missing, `null`, not a string or empty all become
/// "Unknown device", and none of them is an error.
#[tokio::test]
async fn a_missing_or_odd_device_name_is_accepted_as_unknown_device() {
    let h = harness();
    for device_name in [
        None,
        Some(serde_json::Value::Null),
        Some(serde_json::json!(42)),
        Some(serde_json::json!(["a"])),
        Some(serde_json::json!({ "x": 1 })),
        Some(serde_json::json!("")),
    ] {
        let mut body = serde_json::json!({ "code": h.state.pairing_code() });
        if let Some(value) = &device_name {
            body["device_name"] = value.clone();
        }
        let reply = send(
            &h.app,
            Method::POST,
            "/api/v1/pair",
            None,
            Some(&body.to_string()),
        )
        .await;
        assert_eq!(reply.status, StatusCode::OK, "{device_name:?}");
        let stored = h.state.config.devices().pop().expect("a device was saved");
        assert_eq!(stored.name, "Unknown device", "{device_name:?}");
    }
}

/// Contract C-004: a malformed `/pair` request is `400`, never counts toward the 5 attempts, and
/// is answered `400` even while locked. A well-formed wrong code does count.
#[tokio::test]
async fn malformed_pair_requests_do_not_count_toward_the_lockout() {
    let h = harness();
    let right = h.state.pairing_code();
    let wrong = if right == "000000" {
        "111111"
    } else {
        "000000"
    };
    let malformed = [
        r#"{"code":"#,                        // invalid JSON
        r#"{"device_name":"Pixel"}"#,         // missing code
        r#"{"code":123456}"#,                 // code is not a string
        r#"{"code":null,"device_name":"x"}"#, // null code
    ];

    // Many malformed requests: the right code still works afterwards.
    for _ in 0..3 {
        for body in malformed {
            let reply = send(&h.app, Method::POST, "/api/v1/pair", None, Some(body)).await;
            assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{body}");
            assert_eq!(reply.error_code(), "BAD_REQUEST");
        }
    }

    // Lock /pair with 5 well-formed wrong codes.
    for _ in 0..5 {
        assert_eq!(pair_with(&h, wrong).await.status, StatusCode::FORBIDDEN);
    }
    // While locked: a malformed request is still 400, a well-formed one (even the right code) 403.
    for body in malformed {
        let reply = send(&h.app, Method::POST, "/api/v1/pair", None, Some(body)).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "locked: {body}");
    }
    let locked = pair_with(&h, &right).await;
    assert_eq!(locked.status, StatusCode::FORBIDDEN);
    assert_eq!(locked.error_code(), "BAD_PAIRING_CODE");
    assert!(h.state.config.devices().is_empty());
}

#[tokio::test]
async fn wrong_code_is_403_bad_pairing_code() {
    let h = harness();
    let wrong = if h.state.pairing_code() == "000000" {
        "000001"
    } else {
        "000000"
    };
    let reply = pair_with(&h, wrong).await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN);
    assert_eq!(reply.error_code(), "BAD_PAIRING_CODE");
}

#[tokio::test]
async fn sixth_attempt_is_locked_even_with_the_right_code() {
    let h = harness();
    let right = h.state.pairing_code();
    let wrong = if right == "000000" {
        "000001"
    } else {
        "000000"
    };

    for attempt in 1..=5 {
        let reply = pair_with(&h, wrong).await;
        assert_eq!(reply.status, StatusCode::FORBIDDEN, "attempt {attempt}");
        assert_eq!(reply.error_code(), "BAD_PAIRING_CODE");
    }
    let locked = pair_with(&h, &right).await;
    assert_eq!(locked.status, StatusCode::FORBIDDEN, "locked, still 403");
    assert_eq!(locked.error_code(), "BAD_PAIRING_CODE");
    assert!(h.state.config.devices().is_empty(), "nothing was paired");
}

#[tokio::test]
async fn bad_pair_bodies_are_400_bad_request() {
    let h = harness();
    for body in [
        "{ not json",
        "",
        r#"{"device_name":"x"}"#,
        r#"{"code":123456,"device_name":"x"}"#,
    ] {
        let reply = send(&h.app, Method::POST, "/api/v1/pair", None, Some(body)).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "body {body:?}");
        assert_eq!(reply.error_code(), "BAD_REQUEST", "body {body:?}");
    }
}

#[tokio::test]
async fn pair_without_json_content_type_is_a_json_error() {
    let h = harness();
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/pair")
        .body(Body::from(r#"{"code":"123456","device_name":"x"}"#))
        .unwrap();
    let response = h.app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["error"]["code"], "BAD_REQUEST");
}

/// Contract C-004: a request with no JSON body at all, an empty body, or the wrong content type is
/// `400 BAD_REQUEST` on every endpoint that reads a body (they all use the same extractor).
#[tokio::test]
async fn missing_bodies_and_wrong_content_types_are_400_everywhere() {
    let h = harness();
    let token = paired_token(&h).await;

    for path in [
        "/api/v1/volume",
        "/api/v1/player/seek",
        "/api/v1/play/track",
        "/api/v1/play/playlist",
        "/api/v1/pair",
    ] {
        let auth = format!("Bearer {token}");
        // (content type, body): no body at all, an empty JSON body, valid JSON as plain text.
        for (content_type, body) in [
            (None, ""),
            (Some("application/json"), ""),
            (Some("text/plain"), r#"{"level":10}"#),
        ] {
            let mut builder = Request::builder()
                .method(Method::POST)
                .uri(path)
                .header(header::AUTHORIZATION, &auth);
            if let Some(content_type) = content_type {
                builder = builder.header(header::CONTENT_TYPE, content_type);
            }
            let request = builder.body(Body::from(body)).unwrap();
            let response = h.app.clone().oneshot(request).await.unwrap();
            let status = response.status();
            let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            assert_eq!(status, StatusCode::BAD_REQUEST, "{path} {content_type:?}");
            let json: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(json["error"]["code"], "BAD_REQUEST", "{path}");
        }
    }
}

// ---------- auth and /status ----------

#[tokio::test]
async fn status_without_a_token_is_401() {
    let h = harness();
    let reply = send(&h.app, Method::GET, "/api/v1/status", None, None).await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    assert_eq!(reply.error_code(), "UNAUTHORIZED");
}

#[tokio::test]
async fn status_with_a_wrong_token_or_scheme_is_401() {
    let h = harness();
    let token = paired_token(&h).await;

    let wrong = send(&h.app, Method::GET, "/api/v1/status", Some("nope"), None).await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);

    let basic = Request::builder()
        .uri("/api/v1/status")
        .header(header::AUTHORIZATION, format!("Basic {token}"))
        .body(Body::empty())
        .unwrap();
    let response = h.app.clone().oneshot(basic).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn status_with_a_token_is_200() {
    let h = harness();
    let token = paired_token(&h).await;

    let reply = send(&h.app, Method::GET, "/api/v1/status", Some(&token), None).await;
    assert_eq!(reply.status, StatusCode::OK);
    let status: StatusResponse = reply.json();
    assert_eq!(status.agent_version, env!("CARGO_PKG_VERSION"));
    assert!(status.apple_music_running);

    h.media.set_running(false);
    let reply = send(&h.app, Method::GET, "/api/v1/status", Some(&token), None).await;
    assert!(!reply.json::<StatusResponse>().apple_music_running);
}

#[tokio::test]
async fn bearer_scheme_is_case_insensitive() {
    let h = harness();
    let token = paired_token(&h).await;
    let request = Request::builder()
        .uri("/api/v1/status")
        .header(header::AUTHORIZATION, format!("bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let response = h.app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn a_revoked_token_gets_401() {
    let h = harness();
    let token = paired_token(&h).await;
    assert_eq!(
        send(&h.app, Method::GET, "/api/v1/status", Some(&token), None)
            .await
            .status,
        StatusCode::OK
    );

    assert!(h.state.config.revoke(&token).unwrap());
    let reply = send(&h.app, Method::GET, "/api/v1/status", Some(&token), None).await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    assert_eq!(reply.error_code(), "UNAUTHORIZED");
}

#[tokio::test]
async fn query_token_is_accepted_only_on_ws() {
    let h = harness();
    let token = paired_token(&h).await;

    // A valid query token passes auth; a plain GET (no upgrade headers) then gets the JSON 400.
    let ws_ok = send(
        &h.app,
        Method::GET,
        &format!("/api/v1/ws?token={token}"),
        None,
        None,
    )
    .await;
    assert_eq!(ws_ok.status, StatusCode::BAD_REQUEST);
    assert_eq!(ws_ok.error_code(), "BAD_REQUEST");
    let ws_bad = send(&h.app, Method::GET, "/api/v1/ws?token=nope", None, None).await;
    assert_eq!(ws_bad.status, StatusCode::UNAUTHORIZED);

    let elsewhere = send(
        &h.app,
        Method::GET,
        &format!("/api/v1/status?token={token}"),
        None,
        None,
    )
    .await;
    assert_eq!(
        elsewhere.status,
        StatusCode::UNAUTHORIZED,
        "query tokens only work on /ws"
    );
}

// ---------- fallbacks ----------

#[tokio::test]
async fn unknown_route_is_a_json_404() {
    let h = harness();
    let token = paired_token(&h).await;
    for uri in ["/api/v1/nope", "/nope", "/"] {
        let reply = send(&h.app, Method::GET, uri, Some(&token), None).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{uri}");
        assert_eq!(reply.error_code(), "NOT_FOUND", "{uri}");
    }
}

#[tokio::test]
async fn unknown_route_without_a_token_does_not_leak_and_is_401() {
    let h = harness();
    let reply = send(&h.app, Method::GET, "/api/v1/nope", None, None).await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    assert_eq!(reply.error_code(), "UNAUTHORIZED");
}

#[tokio::test]
async fn wrong_method_is_a_json_405() {
    let h = harness();
    let token = paired_token(&h).await;
    for (method, uri) in [
        (Method::POST, "/api/v1/status"),
        (Method::DELETE, "/api/v1/status"),
        (Method::GET, "/api/v1/pair"),
    ] {
        let reply = send(&h.app, method.clone(), uri, Some(&token), None).await;
        assert_eq!(
            reply.status,
            StatusCode::METHOD_NOT_ALLOWED,
            "{method} {uri}"
        );
        assert_eq!(reply.error_code(), "METHOD_NOT_ALLOWED", "{method} {uri}");
    }
}
