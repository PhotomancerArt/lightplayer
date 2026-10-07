//! http → https now lives in the app (fly's `force_https` is off so the
//! relay's device leg can be plain HTTP). These tests are what stands
//! between that change and a plain-HTTP path to the whole site:
//!
//! - every route in `router.rs` — the list below is checked against the
//!   router's own source, so a new route without a case here fails — answers
//!   a request fly received over plain HTTP with `301` to the same host, path
//!   and query on https, whatever the method;
//! - `/relay/device`, and only it, upgrades over plain HTTP;
//! - requests that did not come through fly's proxy (no
//!   `X-Forwarded-Proto`), or came in over https, are served as before.

mod edge_harness;

use axum::http::{StatusCode, header};
use edge_harness::{TestServer, header_value};
use futures_util::SinkExt as _;
use lp_cloud_server::app_state::AppState;
use lp_cloud_server::config::ServerConfig;
use lp_cloud_server::page::static_site::StaticSite;
use lp_cloud_server::ports::{AnyBlobStore, AnyMetaStore};
use lp_cloud_server::router::build_router;
use lp_cloud_store_mem::{MemBlobStore, MemMetaStore};
use lpc_relay::{RelayFrame, RelayHello};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;

/// One request per route (and per method a route answers), as the router
/// declares them. `pattern` is the route as written in `router.rs`;
/// `request` is a concrete path (with a query, to prove it survives).
const ROUTES: &[(&str, &str, &str)] = &[
    ("/api", "POST", "/api"),
    ("/b/{hash}", "GET", "/b/abc?x=1"),
    ("/b/{hash}", "PUT", "/b/abc"),
    ("/t/{hash}", "GET", "/t/abc"),
    ("/t/{hash}", "PUT", "/t/abc"),
    ("/p/{*share}", "GET", "/p/prjabc/dome?ref=card"),
    ("/auth/google", "GET", "/auth/google?next=/home"),
    (
        "CALLBACK_PATH",
        "GET",
        "/auth/google/callback?code=c&state=s",
    ),
    ("/auth/guest", "POST", "/auth/guest"),
    ("/auth/logout", "POST", "/auth/logout"),
    ("/auth/dev", "GET", "/auth/dev?email=a@example.com"),
    (
        "/firmware/{target}/releases",
        "GET",
        "/firmware/esp32c6-4mb/releases?x=1",
    ),
    (
        "/firmware/{target}/releases",
        "HEAD",
        "/firmware/esp32c6-4mb/releases",
    ),
    (
        "/firmware/{target}/releases",
        "OPTIONS",
        "/firmware/esp32c6-4mb/releases",
    ),
    (
        "/firmware/{target}/{release}/{file}",
        "GET",
        "/firmware/esp32c6/latest/app.bin",
    ),
    (
        "/firmware/{target}/{release}/{file}",
        "HEAD",
        "/firmware/esp32c6/latest/app.bin",
    ),
    (
        "/firmware/{target}/{release}/{file}",
        "OPTIONS",
        "/firmware/esp32c6/latest/app.bin",
    ),
    ("/relay/board/{id}", "GET", "/relay/board/10bda3b08e30"),
    ("/healthz", "GET", "/healthz"),
    ("fallback", "GET", "/"),
    ("fallback", "GET", "/home/devices?tab=wifi"),
    ("fallback", "GET", "/assets/app-a1b2c3d4.js"),
];

#[tokio::test]
async fn every_route_redirects_plain_http_to_https_with_its_path_and_query() {
    let server = TestServer::new();
    for (_, method, path) in ROUTES {
        let response = server
            .send(
                method,
                path,
                &[("x-forwarded-proto", "http"), ("host", "lightplayer.app")],
            )
            .await;
        assert_eq!(
            response.status(),
            StatusCode::MOVED_PERMANENTLY,
            "{method} {path}"
        );
        assert_eq!(
            header_value(&response, header::LOCATION),
            Some(format!("https://lightplayer.app{path}").as_str()),
            "{method} {path}"
        );
        assert!(
            response.headers().get(header::SET_COOKIE).is_none(),
            "{method} {path}: a redirect sets nothing"
        );
    }
}

/// The walk above must cover the router: every `.route(` in `router.rs`
/// has a case. A new route added without one fails here.
#[test]
fn the_walk_covers_every_route_in_the_router() {
    let source = include_str!("../src/router.rs");
    let mut declared = Vec::new();
    // `.route(` then the pattern, possibly on the next line, up to its comma.
    for (_, rest) in source
        .match_indices(".route(")
        .map(|(at, _)| source.split_at(at + 7))
    {
        let pattern = rest.split(',').next().unwrap().trim();
        let pattern = match pattern.trim_matches('"') {
            "google_auth::CALLBACK_PATH" => "CALLBACK_PATH",
            "lpc_relay::RELAY_DEVICE_PATH" => "RELAY_DEVICE_PATH",
            other => other,
        };
        declared.push(pattern.to_string());
    }
    assert!(source.contains(".fallback("), "the router has a fallback");
    assert!(declared.len() >= 12, "parsed the router: {declared:?}");
    for pattern in &declared {
        if pattern == "RELAY_DEVICE_PATH" {
            continue; // the one exception, tested below over a real socket
        }
        assert!(
            ROUTES.iter().any(|(route, _, _)| route == pattern),
            "route {pattern} has no case in the redirect walk"
        );
    }
}

#[tokio::test]
async fn plain_http_is_redirected_on_any_host_it_came_in_on() {
    let server = TestServer::new();
    let response = server
        .send(
            "GET",
            "/healthz",
            &[
                ("x-forwarded-proto", "HTTP"),
                ("host", "lightplayer.fly.dev"),
            ],
        )
        .await;
    assert_eq!(response.status(), StatusCode::MOVED_PERMANENTLY);
    assert_eq!(
        header_value(&response, header::LOCATION),
        Some("https://lightplayer.fly.dev/healthz")
    );
}

#[tokio::test]
async fn https_and_unproxied_requests_are_served_as_before() {
    let server = TestServer::new();
    for headers in [
        vec![("x-forwarded-proto", "https"), ("host", "lightplayer.app")],
        vec![("host", "lightplayer.app")],
        vec![],
    ] {
        let response = server.send("GET", "/healthz", &headers).await;
        assert_eq!(response.status(), StatusCode::OK, "{headers:?}");
    }
}

#[tokio::test]
async fn the_device_leg_upgrades_over_plain_http() {
    let config = ServerConfig::from_vars(|name| match name {
        "LP_CLOUD_STORE" | "LP_CLOUD_BLOBS" => Some("mem".to_string()),
        _ => None,
    })
    .unwrap();
    let state = AppState::new(
        config,
        AnyMetaStore::new(MemMetaStore::new()),
        AnyBlobStore::new(MemBlobStore::new()),
        StaticSite::open(None),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, build_router(state)).await.unwrap() });

    let mut request = format!("ws://127.0.0.1:{port}/relay/device")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("x-forwarded-proto", "http".parse().unwrap());
    let (mut socket, response) = tokio_tungstenite::connect_async(request)
        .await
        .expect("the device leg upgrades over plain HTTP");
    assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
    let hello = RelayHello::new([1; 6], "Lamp", 39, None, vec![[1; 16]]);
    socket
        .send(Message::Binary(RelayFrame::Hello(hello).encode()))
        .await
        .expect("and speaks the relay protocol");

    let mut browser = format!("ws://127.0.0.1:{port}/relay/board/010101010101")
        .into_client_request()
        .unwrap();
    browser
        .headers_mut()
        .insert("x-forwarded-proto", "http".parse().unwrap());
    let refused = tokio_tungstenite::connect_async(browser).await;
    assert!(
        refused.is_err(),
        "the browser leg is not the exception: plain HTTP is redirected, not upgraded"
    );
}
