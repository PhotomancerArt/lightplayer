//! The page plane: share cards, the SPA fallback, and static files.
//!
//! The load-bearing property under test is that a share URL answers
//! **identically** — same status, same document — whether the project is
//! private, missing, or simply not a uid at all. Only a link-visible project
//! adds tags. Anything else would make the route an oracle for which project
//! uids exist.

mod edge_harness;

use axum::http::{StatusCode, header};
use edge_harness::{ASSET_BODY, ASSET_PATH, INDEX_HTML, TestServer, body_text, header_value};
use lpc_cloud_api::{CloudRequest, CloudResponse, SidecarMeta, Visibility};
use lpc_history::{
    ContentHash, EventKind, HistoryEvent, PrefixedUid, TreeEntry, TreeManifest, UidPrefix,
};
use lpfs::LpPathBuf;

/// The share card: the project's own name, its preview PNG as an absolute
/// blob URL, and the canonical link.
#[tokio::test]
async fn a_link_visible_project_gets_og_tags() {
    let server = TestServer::new();
    let session = server.sign_in("yona@example.com").await;
    let uid = uid(1);
    let preview = publish(&server, &session, uid, Visibility::Link, "zook-dome", true).await;

    let response = server.get(&format!("/p/zook-dome-{uid}")).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        header_value(&response, header::CONTENT_TYPE),
        Some("text/html; charset=utf-8")
    );
    assert_eq!(
        header_value(&response, header::CACHE_CONTROL),
        Some("no-cache")
    );

    let html = body_text(response).await;
    assert!(
        html.contains(r#"<meta property="og:title" content="Zook Dome">"#),
        "{html}"
    );
    assert!(html.contains(&format!(
        r#"<meta property="og:image" content="http://localhost:31415/b/{preview}">"#
    )));
    assert!(html.contains(&format!(
        r#"<meta property="og:url" content="http://localhost:31415/p/zook-dome-{uid}">"#
    )));
    assert!(html.contains("og:description"));
    // injected into the head of the real document, which is still there
    assert!(html.contains("<body>app</body>"));
    assert!(html.find("og:title").unwrap() < html.find("</head>").unwrap());
}

/// A private project must be indistinguishable from a uid that never
/// existed — including in what an unauthenticated share request gets back.
#[tokio::test]
async fn a_private_project_and_an_unknown_uid_are_the_same_answer() {
    let server = TestServer::new();
    let session = server.sign_in("yona@example.com").await;
    let private = uid(2);
    publish(
        &server,
        &session,
        private,
        Visibility::Private,
        "secret",
        true,
    )
    .await;

    let hidden = server.get(&format!("/p/secret-{private}")).await;
    let unknown = server.get(&format!("/p/whatever-{}", uid(3))).await;
    let nonsense = server.get("/p/not-a-share-link").await;

    assert_eq!(hidden.status(), StatusCode::OK);
    assert_eq!(unknown.status(), StatusCode::OK);
    assert_eq!(nonsense.status(), StatusCode::OK);

    let hidden = body_text(hidden).await;
    let unknown = body_text(unknown).await;
    let nonsense = body_text(nonsense).await;
    assert_eq!(hidden, unknown);
    assert_eq!(hidden, nonsense);
    assert_eq!(
        hidden, INDEX_HTML,
        "the plain document, tags and all absent"
    );
}

/// The slug is decoration (D24): the uid alone opens the card, and a stale
/// slug still does.
#[tokio::test]
async fn the_slug_in_a_share_link_is_ignored() {
    let server = TestServer::new();
    let session = server.sign_in("yona@example.com").await;
    let uid = uid(4);
    publish(&server, &session, uid, Visibility::Link, "zook-dome", false).await;

    let canonical = body_text(server.get(&format!("/p/zook-dome-{uid}")).await).await;
    let stale_slug = body_text(server.get(&format!("/p/old-name-{uid}")).await).await;
    let bare_uid = body_text(server.get(&format!("/p/{uid}")).await).await;

    assert!(canonical.contains("og:title"));
    assert_eq!(canonical, stale_slug);
    assert_eq!(canonical, bare_uid);
}

/// A project published but never pushed to has no preview, and a card with
/// an `og:image` pointing at nothing is worse than a card without one.
#[tokio::test]
async fn a_project_without_a_preview_gets_a_card_without_an_image() {
    let server = TestServer::new();
    let session = server.sign_in("yona@example.com").await;
    let uid = uid(5);
    publish(
        &server,
        &session,
        uid,
        Visibility::Link,
        "no-preview",
        false,
    )
    .await;

    let html = body_text(server.get(&format!("/p/no-preview-{uid}")).await).await;

    assert!(html.contains("og:title"));
    assert!(!html.contains("og:image"));
}

/// Every client route is a real 200 (D26) — that is the point of serving the
/// root document from the service rather than a static host.
#[tokio::test]
async fn client_routes_fall_back_to_the_document() {
    let server = TestServer::new();

    for path in ["/", "/sim/zook-dome", "/settings/devices", "/anything/deep"] {
        let response = server.get(path).await;
        assert_eq!(response.status(), StatusCode::OK, "for {path}");
        assert_eq!(
            header_value(&response, header::CACHE_CONTROL),
            Some("no-cache")
        );
        assert_eq!(body_text(response).await, INDEX_HTML, "for {path}");
    }
}

/// A hashed asset is served as itself, with the year-long cache its name
/// earns — and a missing one is a 404, not a document.
#[tokio::test]
async fn assets_are_served_and_missing_ones_are_not_the_document() {
    let server = TestServer::new();

    let asset = server.get(ASSET_PATH).await;
    assert_eq!(asset.status(), StatusCode::OK);
    assert_eq!(
        header_value(&asset, header::CONTENT_TYPE),
        Some("text/javascript; charset=utf-8")
    );
    assert_eq!(
        header_value(&asset, header::CACHE_CONTROL),
        Some("public, max-age=31536000, immutable")
    );
    assert_eq!(body_text(asset).await, ASSET_BODY);

    assert_eq!(
        server.get("/assets/missing-b2c3d4e5.js").await.status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn healthz_answers_without_touching_the_store() {
    let server = TestServer::new();
    let response = server.get("/healthz").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_text(response).await, "ok\n");
}

/// Publish a project and, optionally, give it a pushed preview PNG.
/// Everything goes through the real planes — no reaching behind the API.
async fn publish(
    server: &TestServer,
    session: &edge_harness::Session,
    uid: PrefixedUid,
    visibility: Visibility,
    slug: &str,
    with_preview: bool,
) -> ContentHash {
    let published = server
        .call(
            CloudRequest::PublishProject {
                uid,
                visibility,
                slug: slug.to_string(),
            },
            Some(session),
        )
        .await;
    assert!(published.result.is_ok(), "{:?}", published.result);

    let png = b"\x89PNG\r\n\x1a\npreview".to_vec();
    let preview = ContentHash::of(&png);
    if !with_preview {
        // still push a commit, so the sidecar carries the real display name
        push(server, session, uid, None).await;
        return preview;
    }

    server
        .put(&format!("/b/{preview}"), png, Some(session))
        .await;
    push(server, session, uid, Some(preview)).await;
    preview
}

/// One commit, so the project has a client-computed sidecar (D3) — which is
/// where the card's title comes from.
async fn push(
    server: &TestServer,
    session: &edge_harness::Session,
    uid: PrefixedUid,
    preview_png: Option<ContentHash>,
) {
    let manifest = TreeManifest::from_entries(vec![TreeEntry {
        path: LpPathBuf::from("/project.json"),
        hash: ContentHash::of(b"{}"),
    }])
    .unwrap();
    let tree = manifest.package_hash();
    server
        .put(
            &format!("/t/{tree}"),
            serde_json::to_vec(&manifest).unwrap(),
            Some(session),
        )
        .await;

    let pushed = server
        .call(
            CloudRequest::PushCommit {
                uid,
                parents: vec![],
                tree,
                // The first push of a project's line: an origin event and
                // the save that produced this version, which is the
                // smallest batch `validate_push_events` accepts.
                events: vec![
                    HistoryEvent {
                        at: 1_000.0,
                        kind: EventKind::Created,
                    },
                    HistoryEvent {
                        at: 1_001.0,
                        kind: EventKind::Saved { version: tree },
                    },
                ],
                sidecar: SidecarMeta {
                    name: "Zook Dome".to_string(),
                    format_version: 5,
                    preview_png,
                },
            },
            Some(session),
        )
        .await;
    assert!(
        matches!(pushed.result, Ok(CloudResponse::PushResult { .. })),
        "{:?}",
        pushed.result
    );
}

fn uid(seed: u8) -> PrefixedUid {
    PrefixedUid::mint(UidPrefix::Project, &[seed; 16])
}
