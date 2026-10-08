//! The firmware plane: `/firmware/{target}/{release}/{file}` and the release
//! index, `/api/v1/firmware/{target}/releases`.
//!
//! The router is driven with `oneshot` against an in-process
//! [`FirmwareUpstream`] stub that records every asset it is asked for, so
//! each test can say exactly how many upstream calls an answer cost — the
//! property that matters most is that a 404 decided by the grammar costs
//! none. The release is a synthetic one built with `lpc-firmware-release`'s
//! own types over small byte strings; no firmware is needed.
//!
//! The release index tests add a [`ReleaseListUpstream`] stub answering a
//! GitHub-shaped releases list with an ETag. The ones about time drive
//! [`FirmwarePlane`] directly with a chosen `now`; the ones about headers go
//! through the router.
//!
//! The last tests drive [`GithubReleaseUpstream`] and
//! [`GithubReleaseListUpstream`] themselves against a loopback stub server
//! (the `google_auth.rs` pattern): URL forms, a redirect, the body cap, the
//! conditional list fetch and where the token goes.

mod edge_harness;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect};
use axum::routing::get;
use edge_harness::{
    BUNDLE_FIRMWARE_MANIFEST_BODY, BUNDLE_FIRMWARE_MANIFEST_PATH, BUNDLE_UPDATE_FILES, INDEX_HTML,
    TestServer, body_bytes, body_text, header_value,
};
use lp_cloud_server::config::GithubToken;
use lp_cloud_server::firmware::firmware_plane::{FirmwarePlane, LookupFailure};
use lp_cloud_server::firmware::firmware_upstream::{
    FirmwareUpstream, UpstreamAsset, UpstreamError, UpstreamFuture,
};
use lp_cloud_server::firmware::github_release_list_upstream::GithubReleaseListUpstream;
use lp_cloud_server::firmware::github_release_upstream::GithubReleaseUpstream;
use lp_cloud_server::firmware::release_index_cache::RELEASE_LIST_STALE_SECONDS;
use lp_cloud_server::firmware::release_list_upstream::{
    ReleaseListFetch, ReleaseListFuture, ReleaseListUpstream,
};
use lpc_cloud_api::response::UserInfo;
use lpc_cloud_api::{Actor, CloudRequest, CloudResponse};
use lpc_firmware_release::{
    EncodedPieceFile, Encoding1, EncodingEntry, OtaManifest, PackageRef, PieceFile, ReleaseIndex,
    ReleaseVersion, Requires, TargetName, sha256_hex,
};
use serde_json::{Value, json};

const TARGET: &str = "esp32c6-4mb";
const VERSION: &str = "2026.10.05-3";
const COMMIT: &str = "103285d5d05e0c2223260f7a3ccf69eb32cf40cc";
const BUILD_ID: &str = "2026.10.05-3+103285d5d05e";

// ---------------------------------------------------------------------------
// The lookup
// ---------------------------------------------------------------------------

/// 1. The manifest by version: the exact upstream bytes, open to any origin,
///    cacheable forever, its ETag the SHA-256 of those bytes.
#[tokio::test]
async fn a_version_serves_the_manifest_byte_exact() {
    let release = Release::new();
    let (server, upstream) = release.serve();

    let response = server
        .get(&format!("/firmware/{TARGET}/{VERSION}/ota-manifest.json"))
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_firmware_headers(&response, &release.manifest_bytes);
    assert_eq!(
        header_value(&response, header::CONTENT_TYPE),
        Some("application/json")
    );
    assert_eq!(body_bytes(response).await, release.manifest_bytes);
    assert_eq!(upstream.calls(), vec![release_asset("ota-manifest.json")]);
}

/// 2. A file: fetched once, checked, stored; the second request is answered
///    from the blob store without asking upstream.
#[tokio::test]
async fn a_file_is_fetched_once_then_served_from_the_blob_store() {
    let release = Release::new();
    let (server, upstream) = release.serve();
    let path = format!("/firmware/{TARGET}/{VERSION}/engine.bin");

    let first = server.get(&path).await;
    assert_eq!(first.status(), StatusCode::OK);
    assert_firmware_headers(&first, &release.files["engine.bin"]);
    assert_eq!(
        header_value(&first, header::CONTENT_TYPE),
        Some("application/octet-stream")
    );
    assert_eq!(body_bytes(first).await, release.files["engine.bin"]);
    let after_first = upstream.calls().len();
    assert_eq!(after_first, 2, "the manifest, then the file");

    let second = server.get(&path).await;
    assert_eq!(second.status(), StatusCode::OK);
    assert_eq!(body_bytes(second).await, release.files["engine.bin"]);
    assert_eq!(upstream.calls().len(), after_first, "no upstream call");

    // Every file the manifest names is served, byte-equal.
    for (file, bytes) in &release.files {
        let response = server
            .get(&format!("/firmware/{TARGET}/{VERSION}/{file}"))
            .await;
        assert_eq!(response.status(), StatusCode::OK, "{file}");
        assert_eq!(&body_bytes(response).await, bytes, "{file}");
    }
}

/// 3. Upstream sends the wrong bytes: 502, `no-store`, nothing stored; once
///    upstream is right again, the same request succeeds.
#[tokio::test]
async fn wrong_upstream_bytes_are_a_502_and_are_never_stored() {
    let release = Release::new();
    let (server, upstream) = release.serve();
    let path = format!("/firmware/{TARGET}/{VERSION}/core.z");
    let good = release.files["core.z"].clone();
    let mut tampered = good.clone();
    tampered[0] ^= 0xff;
    upstream.put(release_asset("core.z"), tampered);

    let response = server.get(&path).await;
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(
        header_value(&response, header::CACHE_CONTROL),
        Some("no-store")
    );
    assert_eq!(
        header_value(&response, header::ACCESS_CONTROL_ALLOW_ORIGIN),
        Some("*")
    );

    upstream.put(release_asset("core.z"), good.clone());
    let calls_before = upstream.calls().len();
    let response = server.get(&path).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_bytes(response).await, good);
    assert_eq!(
        upstream.calls().len(),
        calls_before + 1,
        "the bad bytes were not cached: the file was fetched again"
    );

    // A wrong length is refused the same way.
    let release = Release::new();
    let (server, upstream) = release.serve();
    upstream.put(release_asset("engine.bin"), b"short".to_vec());
    let response = server
        .get(&format!("/firmware/{TARGET}/{VERSION}/engine.bin"))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
}

/// 4. `latest` redirects to the version path for 60 s; within the TTL a
///    second `latest` costs no upstream call, and following the redirect is
///    answered from the manifest `latest` already cached.
#[tokio::test]
async fn latest_redirects_to_the_version_and_is_remembered() {
    let release = Release::new();
    let (server, upstream) = release.serve();

    let response = server
        .get(&format!("/firmware/{TARGET}/latest/ota-manifest.json"))
        .await;
    assert_eq!(response.status(), StatusCode::FOUND);
    assert_eq!(
        header_value(&response, header::LOCATION),
        Some(format!("/firmware/{TARGET}/{VERSION}/ota-manifest.json").as_str())
    );
    assert_eq!(
        header_value(&response, header::CACHE_CONTROL),
        Some("public, max-age=60")
    );
    assert_eq!(
        header_value(&response, header::ACCESS_CONTROL_ALLOW_ORIGIN),
        Some("*")
    );
    assert_eq!(upstream.calls(), vec![latest_asset("ota-manifest.json")]);

    let again = server
        .get(&format!("/firmware/{TARGET}/latest/engine.z"))
        .await;
    assert_eq!(again.status(), StatusCode::FOUND);
    assert_eq!(
        header_value(&again, header::LOCATION),
        Some(format!("/firmware/{TARGET}/{VERSION}/engine.z").as_str())
    );
    let followed = server
        .get(&format!("/firmware/{TARGET}/{VERSION}/ota-manifest.json"))
        .await;
    assert_eq!(body_bytes(followed).await, release.manifest_bytes);
    assert_eq!(
        upstream.calls().len(),
        1,
        "latest and its manifest were cached"
    );
}

/// 5. A build id equal to the manifest's derived one is served; a build id
///    whose commit differs is a 404; a 7- or 40-hex suffix never reaches
///    upstream.
#[tokio::test]
async fn build_ids_must_match_the_manifest() {
    let release = Release::new();
    let (server, upstream) = release.serve();

    for build_id in [BUILD_ID.to_string(), BUILD_ID.replace('+', "%2B")] {
        let response = server
            .get(&format!("/firmware/{TARGET}/{build_id}/engine.bin"))
            .await;
        assert_eq!(response.status(), StatusCode::OK, "{build_id}");
        assert_eq!(body_bytes(response).await, release.files["engine.bin"]);
    }

    let response = server
        .get(&format!(
            "/firmware/{TARGET}/{VERSION}+aaaaaaaaaaaa/engine.bin"
        ))
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let calls = upstream.calls().len();
    for suffix in ["103285d", COMMIT] {
        let response = server
            .get(&format!("/firmware/{TARGET}/{VERSION}+{suffix}/engine.bin"))
            .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{suffix}");
    }
    assert_eq!(upstream.calls().len(), calls, "no upstream call");
}

/// 6. Everything the grammar or the manifest refuses is a 404 that costs no
///    upstream call: dev versions, a `v` prefix, a bad target, reserved
///    words, a file the manifest does not name, a fourth segment.
#[tokio::test]
async fn refused_lookups_never_reach_upstream() {
    let release = Release::new();
    let (server, upstream) = release.serve();
    let cases = [
        (
            format!("/firmware/{TARGET}/abc1234/ota-manifest.json"),
            "dev builds are not in the store",
        ),
        (
            format!("/firmware/{TARGET}/abc1234-dirty-101500PT/ota-manifest.json"),
            "dev builds are not in the store",
        ),
        (
            format!("/firmware/{TARGET}/v{VERSION}/ota-manifest.json"),
            "no such release",
        ),
        (
            format!("/firmware/ESP32C6/{VERSION}/ota-manifest.json"),
            "no such target",
        ),
        (
            format!("/firmware/{TARGET}/stable/ota-manifest.json"),
            "reserved for a future channel",
        ),
        (
            format!("/firmware/{TARGET}/beta/engine.bin"),
            "reserved for a future channel",
        ),
        (
            format!("/firmware/{TARGET}/{VERSION}/..manifest"),
            "no such file",
        ),
    ];
    for (path, reason) in &cases {
        let response = server.get(path).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
        assert_eq!(
            header_value(&response, header::CACHE_CONTROL),
            Some("public, max-age=60"),
            "{path}"
        );
        assert_eq!(
            header_value(&response, header::ACCESS_CONTROL_ALLOW_ORIGIN),
            Some("*"),
            "{path}"
        );
        assert_eq!(body_text(response).await, format!("{reason}\n"), "{path}");
    }
    assert_eq!(upstream.calls(), Vec::<UpstreamAsset>::new());

    // A four-segment path is not this route at all — the static fallback
    // answers it (the SPA document) — and never reaches upstream either.
    let response = server
        .get(&format!("/firmware/{TARGET}/{VERSION}/a/b"))
        .await;
    assert!(
        response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none(),
        "not a firmware answer"
    );
    assert_eq!(upstream.calls(), Vec::<UpstreamAsset>::new());

    // A file the manifest does not name: the manifest is fetched, the file
    // never is.
    let response = server
        .get(&format!("/firmware/{TARGET}/{VERSION}/secrets.txt"))
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(body_text(response).await, "no such file\n");
    assert_eq!(upstream.calls(), vec![release_asset("ota-manifest.json")]);
}

/// 7. Upstream has no such release: 404 with `max-age=60`, and the miss is
///    remembered — a second request within 60 s does not ask again.
#[tokio::test]
async fn an_upstream_404_is_a_remembered_404() {
    let (server, upstream) = StubUpstream::empty().serve();
    let path = format!("/firmware/{TARGET}/{VERSION}/ota-manifest.json");

    for _ in 0..2 {
        let response = server.get(&path).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            header_value(&response, header::CACHE_CONTROL),
            Some("public, max-age=60")
        );
        assert_eq!(body_text(response).await, "no such release\n");
    }
    assert_eq!(upstream.calls().len(), 1);

    for _ in 0..2 {
        let response = server
            .get(&format!("/firmware/{TARGET}/latest/ota-manifest.json"))
            .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
    assert_eq!(upstream.calls().len(), 2);
}

/// 8. Upstream fails or times out: 502 / 504, `no-store`, ACAO present —
///    and the failure is not remembered.
#[tokio::test]
async fn upstream_failures_are_502_or_504_and_not_cached() {
    let release = Release::new();
    for (error, status) in [
        (
            UpstreamError::Unreachable("connection refused".into()),
            StatusCode::BAD_GATEWAY,
        ),
        (UpstreamError::Status(500), StatusCode::BAD_GATEWAY),
        (
            UpstreamError::TooLarge { limit: 16 },
            StatusCode::BAD_GATEWAY,
        ),
        (UpstreamError::TimedOut, StatusCode::GATEWAY_TIMEOUT),
    ] {
        let (server, upstream) = release.serve();
        upstream.fail_with(Some(error.clone()));
        let path = format!("/firmware/{TARGET}/{VERSION}/ota-manifest.json");

        let response = server.get(&path).await;
        assert_eq!(response.status(), status, "{error}");
        assert_eq!(
            header_value(&response, header::CACHE_CONTROL),
            Some("no-store")
        );
        assert_eq!(
            header_value(&response, header::ACCESS_CONTROL_ALLOW_ORIGIN),
            Some("*")
        );

        upstream.fail_with(None);
        assert_eq!(server.get(&path).await.status(), StatusCode::OK, "{error}");
        assert_eq!(upstream.calls().len(), 2);
    }

    // A manifest that does not validate, or names another target or
    // version, is a 502 too.
    let mut wrong = release.manifest.clone();
    wrong.version = "2026.10.05-4".into();
    let stub = release.stub();
    stub.put(release_asset("ota-manifest.json"), wrong.to_json_bytes());
    stub.put(
        release_asset("engine.bin"),
        release.files["engine.bin"].clone(),
    );
    let server = TestServer::with_firmware_upstream(stub.clone());
    let response = server
        .get(&format!("/firmware/{TARGET}/{VERSION}/engine.bin"))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    stub.put(
        release_asset("ota-manifest.json"),
        b"{\"format\":1}".to_vec(),
    );
    let response = server
        .get(&format!("/firmware/{TARGET}/{VERSION}/ota-manifest.json"))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
}

/// 9. `HEAD` answers GET's headers with no body; `If-None-Match` is a 304;
///    `OPTIONS` is a preflight answer.
#[tokio::test]
async fn head_conditional_and_options() {
    let release = Release::new();
    let (server, upstream) = release.serve();
    let path = format!("/firmware/{TARGET}/{VERSION}/engine.bin");
    let engine = &release.files["engine.bin"];

    let head = server.send("HEAD", &path, &[]).await;
    assert_eq!(head.status(), StatusCode::OK);
    assert_firmware_headers(&head, engine);
    assert_eq!(
        header_value(&head, header::CONTENT_LENGTH),
        Some(engine.len().to_string().as_str())
    );
    assert!(body_bytes(head).await.is_empty());

    let etag = format!("\"{}\"", sha256_hex(engine));
    let calls = upstream.calls().len();
    let not_modified = server
        .send("GET", &path, &[("if-none-match", etag.as_str())])
        .await;
    assert_eq!(not_modified.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(
        header_value(&not_modified, header::ETAG),
        Some(etag.as_str())
    );
    assert_eq!(
        header_value(&not_modified, header::ACCESS_CONTROL_ALLOW_ORIGIN),
        Some("*")
    );
    assert_eq!(upstream.calls().len(), calls);

    let options = server
        .send(
            "OPTIONS",
            &path,
            &[
                ("origin", "https://beta.lightplayer.app"),
                ("access-control-request-method", "GET"),
            ],
        )
        .await;
    assert_eq!(options.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        header_value(&options, header::ACCESS_CONTROL_ALLOW_ORIGIN),
        Some("*")
    );
    assert_eq!(
        header_value(&options, header::ACCESS_CONTROL_ALLOW_METHODS),
        Some("GET, HEAD")
    );
    assert_eq!(
        header_value(&options, header::ALLOW),
        Some("GET, HEAD, OPTIONS")
    );
    assert!(
        options
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
            .is_none()
    );
}

/// 10. The Studio bundle's own `/firmware/<target>/manifest.json` (two
///     segments) is still the static fallback's.
#[tokio::test]
async fn the_bundles_own_firmware_files_are_still_static() {
    let (server, upstream) = Release::new().serve();
    let response = server.get(BUNDLE_FIRMWARE_MANIFEST_PATH).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_text(response).await, BUNDLE_FIRMWARE_MANIFEST_BODY);
    assert!(upstream.calls().is_empty());
}

/// 11. The bundle's own update files (`ota-manifest.json`, `core.z`,
///     `engine.z`) sit beside its package manifest, two segments deep, and
///     the static bundle answers them — not the lookup. One level further
///     down (`<target>/ota/…`, where they first shipped) is the lookup's,
///     and `ota` is a reserved word there: a 404 that never reaches the
///     bundle (`docs/defects/2026-10-06-the-bundles-ota-files-are-shadowed-
///     by-the-firmware-lookup.md`).
#[tokio::test]
async fn the_bundles_update_files_beside_its_manifest_are_static() {
    let (server, upstream) = Release::new().serve();
    for (path, body) in BUNDLE_UPDATE_FILES {
        let response = server.get(path).await;
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        assert!(
            response
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .is_none(),
            "{path}: not a firmware answer"
        );
        assert_eq!(body_text(response).await, body, "{path}");
    }
    assert!(upstream.calls().is_empty());

    let response = server
        .get(&format!("/firmware/{TARGET}/ota/ota-manifest.json"))
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        header_value(&response, header::ACCESS_CONTROL_ALLOW_ORIGIN),
        Some("*"),
        "the lookup's answer, not the bundle's"
    );
    assert_eq!(body_text(response).await, "reserved for a future channel\n");
    assert!(upstream.calls().is_empty());
}

// ---------------------------------------------------------------------------
// The release index
// ---------------------------------------------------------------------------

/// 12. The index lists the target's complete releases newest first by
///     number (`-10` above `-9`), skipping drafts, prereleases, tags that are
///     not `v<release version>` and releases of other targets; every
///     answer header is the index's.
#[tokio::test]
async fn the_index_lists_complete_releases_newest_first() {
    let (nine, ten) = (Release::at("2026.10.06-9"), Release::at("2026.10.06-10"));
    let upstream = StubUpstream::empty();
    nine.put_into(&upstream);
    ten.put_into(&upstream);
    let list = StubReleaseList::new(github_list(&[
        listed(&nine, Some("2026-10-06T18:26:23Z")),
        listed(&ten, None),
        json!({ "tag_name": "v2026.10.06-11", "draft": true, "prerelease": false,
                "assets": asset_list(&ten.asset_names()) }),
        json!({ "tag_name": "v2026.10.06-12", "draft": false, "prerelease": true,
                "assets": asset_list(&ten.asset_names()) }),
        json!({ "tag_name": "nightly", "draft": false, "prerelease": false,
                "assets": asset_list(&ten.asset_names()) }),
        json!({ "tag_name": "v2026.10.06-13", "draft": false, "prerelease": false,
                "assets": asset_list(&["esp32s3-8mb.package.json".to_string()]) }),
    ]));
    let server = TestServer::with_firmware_upstreams(upstream.clone(), list.clone());

    let response = server
        .get(&format!("/api/v1/firmware/{TARGET}/releases"))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        header_value(&response, header::CONTENT_TYPE),
        Some("application/json")
    );
    assert_eq!(
        header_value(&response, header::CACHE_CONTROL),
        Some("public, max-age=60")
    );
    assert_eq!(
        header_value(&response, header::ACCESS_CONTROL_ALLOW_ORIGIN),
        Some("*")
    );
    assert_eq!(
        header_value(&response, header::X_CONTENT_TYPE_OPTIONS),
        Some("nosniff")
    );
    let etag = header_value(&response, header::ETAG).map(str::to_string);
    let bytes = body_bytes(response).await;
    assert_eq!(etag, Some(format!("\"{}\"", sha256_hex(&bytes))));

    let index = ReleaseIndex::parse_valid(&bytes).unwrap();
    assert_eq!(index.target, TARGET);
    assert_eq!(versions(&index), ["2026.10.06-10", "2026.10.06-9"]);
    assert_eq!(index.releases[0].published_at, None);
    assert_eq!(
        index.releases[1].published_at.as_deref(),
        Some("2026-10-06T18:26:23Z")
    );
    assert_eq!(index.releases[1].commit, COMMIT);
    assert_eq!(index.releases[1].wire_proto, 36);
    assert_eq!(
        bytes,
        index.to_json_bytes(),
        "served as the format writes it"
    );

    // Only the two candidates' manifests were fetched, and the list once.
    assert_eq!(
        sorted(upstream.calls()),
        sorted(vec![
            asset_at("2026.10.06-9", "ota-manifest.json"),
            asset_at("2026.10.06-10", "ota-manifest.json"),
        ])
    );
    assert_eq!(list.calls(), vec![None]);
}

/// 13. A release whose upload is still running (one file not yet
///     `uploaded`, or missing) is left out; once the list shows it complete,
///     it is listed.
#[tokio::test]
async fn a_release_still_uploading_is_listed_once_complete() {
    let (nine, ten) = (Release::at("2026.10.06-9"), Release::at("2026.10.06-10"));
    let upstream = StubUpstream::empty();
    nine.put_into(&upstream);
    ten.put_into(&upstream);
    let mut uploading = listed(&ten, None);
    for asset in uploading["assets"].as_array_mut().unwrap() {
        if asset["name"] == format!("{TARGET}.engine.z") {
            asset["state"] = json!("starter");
        }
    }
    let mut missing = listed(&ten, None);
    missing["assets"]
        .as_array_mut()
        .unwrap()
        .retain(|asset| asset["name"] != format!("{TARGET}.package.json"));
    let list = StubReleaseList::new(github_list(&[listed(&nine, None), uploading]));
    let plane = FirmwarePlane::new(upstream.clone(), list.clone());
    let target = target();

    let index = index_at(&plane, &target, 0.0).await;
    assert_eq!(versions(&index), ["2026.10.06-9"]);

    list.set(github_list(&[listed(&nine, None), missing]));
    let index = index_at(&plane, &target, 301.0).await;
    assert_eq!(versions(&index), ["2026.10.06-9"]);

    list.set(github_list(&[listed(&nine, None), listed(&ten, None)]));
    let index = index_at(&plane, &target, 602.0).await;
    assert_eq!(versions(&index), ["2026.10.06-10", "2026.10.06-9"]);
}

/// 14. A manifest that fails verification (here: it names another version)
///     drops only its own entry; the index is still served.
#[tokio::test]
async fn a_manifest_that_fails_verification_drops_only_its_entry() {
    let (nine, ten) = (Release::at("2026.10.06-9"), Release::at("2026.10.06-10"));
    let upstream = StubUpstream::empty();
    nine.put_into(&upstream);
    ten.put_into(&upstream);
    upstream.put(
        asset_at("2026.10.06-10", "ota-manifest.json"),
        nine.manifest_bytes.clone(),
    );
    let list = StubReleaseList::new(github_list(&[listed(&nine, None), listed(&ten, None)]));
    let server = TestServer::with_firmware_upstreams(upstream, list);

    let response = server
        .get(&format!("/api/v1/firmware/{TARGET}/releases"))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let index = ReleaseIndex::parse_valid(&body_bytes(response).await).unwrap();
    assert_eq!(versions(&index), ["2026.10.06-9"]);
}

/// 15. The list is trusted for 300 s (no upstream call), then revalidated
///     with its ETag; a 304 keeps the bytes and refetches no manifest.
#[tokio::test]
async fn the_list_is_held_for_its_ttl_then_revalidated_by_etag() {
    let nine = Release::at("2026.10.06-9");
    let upstream = StubUpstream::empty();
    nine.put_into(&upstream);
    let list = StubReleaseList::new(github_list(&[listed(&nine, None)]));
    let plane = FirmwarePlane::new(upstream.clone(), list.clone());
    let target = target();

    let first = plane.release_index(&target, 1000.0).await.unwrap();
    assert_eq!(list.calls(), vec![None]);
    let manifests = upstream.calls().len();

    let within = plane.release_index(&target, 1299.0).await.unwrap();
    assert_eq!(list.calls().len(), 1, "no upstream call within the TTL");
    assert_eq!(within.bytes, first.bytes);

    let after = plane.release_index(&target, 1300.0).await.unwrap();
    assert_eq!(
        list.calls(),
        vec![None, Some(list.etag())],
        "one conditional call"
    );
    assert_eq!(list.answers(), ["fresh", "not-modified"]);
    assert_eq!(after.bytes, first.bytes);
    assert_eq!(upstream.calls().len(), manifests, "no manifest refetched");

    // A fresh body with the same releases (GitHub's ETag also moves with
    // download counts) is not a rebuild either.
    list.set_etag_only("W/\"moved\"");
    let same = plane.release_index(&target, 1600.0).await.unwrap();
    assert!(Arc::ptr_eq(&same, &after), "the rendered index was kept");
}

/// 16. Upstream down: the last good list is served for up to a day; past
///     that, or with no copy at all, it is a 502 (504 on a timeout),
///     `no-store`.
#[tokio::test]
async fn upstream_down_serves_the_last_good_index_then_fails() {
    let nine = Release::at("2026.10.06-9");
    let upstream = StubUpstream::empty();
    nine.put_into(&upstream);
    let list = StubReleaseList::new(github_list(&[listed(&nine, None)]));
    let plane = FirmwarePlane::new(upstream.clone(), list.clone());
    let target = target();

    let good = plane.release_index(&target, 0.0).await.unwrap();
    list.fail_with(Some(UpstreamError::Status(403)));
    let stale = plane.release_index(&target, 301.0).await.unwrap();
    assert_eq!(stale.bytes, good.bytes);
    let calls = list.calls().len();
    plane.release_index(&target, 330.0).await.unwrap();
    assert_eq!(list.calls().len(), calls, "a failure pauses retries");
    assert!(
        plane
            .release_index(&target, RELEASE_LIST_STALE_SECONDS - 1.0)
            .await
            .is_ok()
    );
    assert_eq!(
        plane
            .release_index(&target, RELEASE_LIST_STALE_SECONDS + 1.0)
            .await
            .err(),
        Some(LookupFailure::Upstream(UpstreamError::Status(403)))
    );

    for (error, status) in [
        (UpstreamError::Status(403), StatusCode::BAD_GATEWAY),
        (
            UpstreamError::Unreachable("connection refused".into()),
            StatusCode::BAD_GATEWAY,
        ),
        (UpstreamError::TimedOut, StatusCode::GATEWAY_TIMEOUT),
    ] {
        let list = StubReleaseList::new(github_list(&[listed(&nine, None)]));
        list.fail_with(Some(error.clone()));
        let server = TestServer::with_firmware_upstreams(upstream.clone(), list.clone());
        let response = server
            .get(&format!("/api/v1/firmware/{TARGET}/releases"))
            .await;
        assert_eq!(response.status(), status, "{error}");
        assert_eq!(
            header_value(&response, header::CACHE_CONTROL),
            Some("no-store")
        );
        assert_eq!(
            header_value(&response, header::ACCESS_CONTROL_ALLOW_ORIGIN),
            Some("*")
        );
        list.fail_with(None);
        let response = server
            .get(&format!("/api/v1/firmware/{TARGET}/releases"))
            .await;
        assert_eq!(response.status(), StatusCode::OK, "not remembered");
    }

    // A body that is not the list is refused the same way, never cached.
    let list = StubReleaseList::new(br#"{"message":"API rate limit exceeded"}"#.to_vec());
    let server = TestServer::with_firmware_upstreams(upstream, list);
    let response = server
        .get(&format!("/api/v1/firmware/{TARGET}/releases"))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
}

/// 17. A target no release carries is a 404 that fetches no manifest; one
///     outside the grammar does not even fetch the list.
#[tokio::test]
async fn an_unknown_target_is_a_404_with_no_manifest_fetch() {
    let nine = Release::at("2026.10.06-9");
    let upstream = StubUpstream::empty();
    nine.put_into(&upstream);
    let list = StubReleaseList::new(github_list(&[listed(&nine, None)]));
    let server = TestServer::with_firmware_upstreams(upstream.clone(), list.clone());

    let response = server.get("/api/v1/firmware/esp32s3-8mb/releases").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        header_value(&response, header::CACHE_CONTROL),
        Some("public, max-age=60")
    );
    assert_eq!(
        header_value(&response, header::ACCESS_CONTROL_ALLOW_ORIGIN),
        Some("*")
    );
    assert_eq!(
        body_text(response).await,
        "no release carries this target\n"
    );
    assert!(upstream.calls().is_empty());
    assert_eq!(list.calls().len(), 1);

    let response = server.get("/api/v1/firmware/ESP32C6/releases").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(body_text(response).await, "no such target\n");
    assert_eq!(list.calls().len(), 1, "no list call for a bad target");
}

/// 18. `HEAD`, `If-None-Match` → 304, and `OPTIONS` on the index.
#[tokio::test]
async fn head_conditional_and_options_on_the_index() {
    let nine = Release::at("2026.10.06-9");
    let upstream = StubUpstream::empty();
    nine.put_into(&upstream);
    let list = StubReleaseList::new(github_list(&[listed(&nine, None)]));
    let server = TestServer::with_firmware_upstreams(upstream, list);
    let path = format!("/api/v1/firmware/{TARGET}/releases");

    let get = server.get(&path).await;
    let etag = header_value(&get, header::ETAG).unwrap().to_string();
    let length = body_bytes(get).await.len();

    let head = server.send("HEAD", &path, &[]).await;
    assert_eq!(head.status(), StatusCode::OK);
    assert_eq!(header_value(&head, header::ETAG), Some(etag.as_str()));
    assert_eq!(
        header_value(&head, header::CONTENT_LENGTH),
        Some(length.to_string().as_str())
    );
    assert!(body_bytes(head).await.is_empty());

    let not_modified = server
        .send("GET", &path, &[("if-none-match", etag.as_str())])
        .await;
    assert_eq!(not_modified.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(
        header_value(&not_modified, header::ETAG),
        Some(etag.as_str())
    );
    assert_eq!(
        header_value(&not_modified, header::CACHE_CONTROL),
        Some("public, max-age=60")
    );
    assert_eq!(
        header_value(&not_modified, header::ACCESS_CONTROL_ALLOW_ORIGIN),
        Some("*")
    );

    let options = server
        .send(
            "OPTIONS",
            &path,
            &[
                ("origin", "https://beta.lightplayer.app"),
                ("access-control-request-method", "GET"),
            ],
        )
        .await;
    assert_eq!(options.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        header_value(&options, header::ACCESS_CONTROL_ALLOW_ORIGIN),
        Some("*")
    );
    assert_eq!(
        header_value(&options, header::ACCESS_CONTROL_ALLOW_METHODS),
        Some("GET, HEAD")
    );
}

/// 19. The index is only at `/api/v1/…`: `/firmware/<target>/releases` is
///     what it was before the index existed — two segments after
///     `/firmware/`, the page fallback's (here the SPA document) — and asks
///     upstream for nothing.
#[tokio::test]
async fn firmware_target_releases_is_not_the_index() {
    let nine = Release::at("2026.10.06-9");
    let upstream = StubUpstream::empty();
    nine.put_into(&upstream);
    let list = StubReleaseList::new(github_list(&[listed(&nine, None)]));
    let server = TestServer::with_firmware_upstreams(upstream.clone(), list.clone());

    let response = server.get(&format!("/firmware/{TARGET}/releases")).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none(),
        "the fallback's answer, not a firmware one"
    );
    assert_eq!(body_text(response).await, INDEX_HTML, "the SPA document");
    assert!(list.calls().is_empty(), "no releases list fetched");
    assert!(upstream.calls().is_empty(), "no manifest fetched");

    let response = server
        .get(&format!("/api/v1/firmware/{TARGET}/releases"))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    ReleaseIndex::parse_valid(&body_bytes(response).await).unwrap();
}

/// 20. The account API at `POST /api` is untouched by the index under
///     `/api/v1/`: an anonymous `WhoAmI` is still answered there.
#[tokio::test]
async fn the_account_api_beside_the_index_is_unchanged() {
    let server = TestServer::new();
    let reply = server.call(CloudRequest::WhoAmI, None).await;
    assert_eq!(
        reply.result,
        Ok(CloudResponse::UserInfo(UserInfo {
            actor: Actor::Anonymous
        }))
    );
}

// ---------------------------------------------------------------------------
// GithubReleaseUpstream against a loopback stub
// ---------------------------------------------------------------------------

/// The real reqwest client: both URL forms, GitHub's redirect from
/// `latest/download` followed, a 404 as `None`, and the body cap.
#[tokio::test]
async fn the_github_upstream_builds_both_url_forms_and_follows_redirects() {
    let base = StubGithub::start().await;
    let upstream = GithubReleaseUpstream::new(&format!("{base}/releases"));
    let version = ReleaseVersion::parse(VERSION).unwrap();

    let release = upstream
        .fetch(UpstreamAsset::Release {
            version: version.clone(),
            asset: format!("{TARGET}.ota-manifest.json"),
        })
        .await;
    assert_eq!(release, Ok(Some(b"the manifest".to_vec())));

    let latest = upstream
        .fetch(UpstreamAsset::Latest {
            asset: format!("{TARGET}.ota-manifest.json"),
        })
        .await;
    assert_eq!(
        latest,
        Ok(Some(b"the manifest".to_vec())),
        "redirect followed"
    );

    let missing = upstream
        .fetch(UpstreamAsset::Release {
            version: version.clone(),
            asset: format!("{TARGET}.nothing"),
        })
        .await;
    assert_eq!(missing, Ok(None));

    let failing = upstream
        .fetch(UpstreamAsset::Release {
            version: version.clone(),
            asset: format!("{TARGET}.fails"),
        })
        .await;
    assert_eq!(failing, Err(UpstreamError::Status(500)));

    let capped = GithubReleaseUpstream::new(&format!("{base}/releases")).with_body_limit(1024);
    let big = capped
        .fetch(UpstreamAsset::Release {
            version,
            asset: format!("{TARGET}.big"),
        })
        .await;
    assert_eq!(big, Err(UpstreamError::TooLarge { limit: 1024 }));
}

/// A connection that cannot be made is `Unreachable`, never a panic.
#[tokio::test]
async fn an_unreachable_upstream_is_an_error() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/releases", listener.local_addr().unwrap());
    drop(listener);
    let upstream = GithubReleaseUpstream::new(&base);
    let reply = upstream
        .fetch(UpstreamAsset::Latest {
            asset: format!("{TARGET}.ota-manifest.json"),
        })
        .await;
    assert!(
        matches!(reply, Err(UpstreamError::Unreachable(_))),
        "{reply:?}"
    );
}

/// The real list client: GitHub's media type, a conditional fetch with the
/// ETag it was given (a 304 is `NotModified`), an error status, the body
/// cap — and the token goes to the list URL and never to the download host.
#[tokio::test]
async fn the_github_list_upstream_is_conditional_and_alone_carries_the_token() {
    let stub = StubGithubApi::start().await;
    let token = GithubToken::new("t0k3n");
    let list = GithubReleaseListUpstream::new(&format!("{}/api/releases", stub.base), Some(token));

    let fresh = list.list(None).await;
    assert_eq!(
        fresh,
        Ok(ReleaseListFetch::Fresh {
            bytes: StubGithubApi::BODY.as_bytes().to_vec(),
            etag: Some(StubGithubApi::ETAG.to_string()),
        })
    );
    let again = list.list(Some(StubGithubApi::ETAG.to_string())).await;
    assert_eq!(again, Ok(ReleaseListFetch::NotModified));

    let downloads = GithubReleaseUpstream::new(&format!("{}/releases", stub.base));
    let manifest = downloads
        .fetch(UpstreamAsset::Release {
            version: ReleaseVersion::parse(VERSION).unwrap(),
            asset: format!("{TARGET}.ota-manifest.json"),
        })
        .await;
    assert_eq!(manifest, Ok(Some(b"the manifest".to_vec())));

    let seen = stub.seen();
    assert_eq!(seen.len(), 3);
    for request in &seen[..2] {
        assert_eq!(request.path, "/api/releases");
        assert_eq!(request.authorization.as_deref(), Some("Bearer t0k3n"));
        assert_eq!(
            request.accept.as_deref(),
            Some("application/vnd.github+json")
        );
    }
    assert_eq!(seen[0].if_none_match, None);
    assert_eq!(seen[1].if_none_match.as_deref(), Some(StubGithubApi::ETAG));
    assert!(seen[2].path.starts_with("/releases/download/"));
    assert_eq!(
        seen[2].authorization, None,
        "the download host never sees the token"
    );

    let unauthenticated =
        GithubReleaseListUpstream::new(&format!("{}/api/releases", stub.base), None);
    unauthenticated.list(None).await.unwrap();
    assert_eq!(stub.seen()[3].authorization, None);

    let failing = GithubReleaseListUpstream::new(&format!("{}/api/limited", stub.base), None);
    assert_eq!(failing.list(None).await, Err(UpstreamError::Status(403)));

    let capped = GithubReleaseListUpstream::new(&format!("{}/api/releases", stub.base), None)
        .with_body_limit(1);
    assert_eq!(
        capped.list(None).await,
        Err(UpstreamError::TooLarge { limit: 1 })
    );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn assert_firmware_headers(response: &axum::http::Response<axum::body::Body>, bytes: &[u8]) {
    assert_eq!(
        header_value(response, header::ACCESS_CONTROL_ALLOW_ORIGIN),
        Some("*")
    );
    assert_eq!(
        header_value(response, header::CACHE_CONTROL),
        Some("public, max-age=31536000, immutable")
    );
    assert_eq!(
        header_value(response, header::ETAG),
        Some(format!("\"{}\"", sha256_hex(bytes)).as_str())
    );
    assert_eq!(
        header_value(response, header::X_CONTENT_TYPE_OPTIONS),
        Some("nosniff")
    );
}

fn release_asset(file: &str) -> UpstreamAsset {
    asset_at(VERSION, file)
}

fn asset_at(version: &str, file: &str) -> UpstreamAsset {
    UpstreamAsset::Release {
        version: ReleaseVersion::parse(version).unwrap(),
        asset: format!("{TARGET}.{file}"),
    }
}

fn latest_asset(file: &str) -> UpstreamAsset {
    UpstreamAsset::Latest {
        asset: format!("{TARGET}.{file}"),
    }
}

/// A synthetic release: six small files and the manifest that names them.
struct Release {
    manifest: OtaManifest,
    manifest_bytes: Vec<u8>,
    files: BTreeMap<&'static str, Vec<u8>>,
}

impl Release {
    fn new() -> Self {
        Self::at(VERSION)
    }

    /// The same synthetic release, published as `version`.
    fn at(version: &str) -> Self {
        let core = vec![0xc0; 5000];
        let engine = vec![0xe0; 300];
        let core_z = vec![0x7a; 30];
        let engine_z = vec![0x7b; 20];
        let package = br#"{"schemaVersion":2,"firmwareId":"esp32c6-4mb"}"#.to_vec();
        let image = vec![0x1e; 4096];
        let piece = |file: &str, bytes: &[u8]| PieceFile {
            file: file.into(),
            length: bytes.len() as u64,
            sha256: sha256_hex(bytes),
        };
        let manifest = OtaManifest {
            format: 1,
            target: TARGET.into(),
            chip: "esp32c6".into(),
            version: version.into(),
            commit: COMMIT.into(),
            wire_proto: 36,
            requires: Requires {
                layout: 1,
                loader: 1,
            },
            core: piece("core.bin", &core),
            engine: piece("engine.bin", &engine),
            encodings: vec![EncodingEntry::deflate1(Encoding1 {
                id: 1,
                codec: "deflate-raw".into(),
                chunk_bytes: 4096,
                window_bytes: 32768,
                core: EncodedPieceFile {
                    file: "core.z".into(),
                    length: core_z.len() as u64,
                    sha256: sha256_hex(&core_z),
                    chunks: vec![30, 0],
                },
                engine: EncodedPieceFile {
                    file: "engine.z".into(),
                    length: engine_z.len() as u64,
                    sha256: sha256_hex(&engine_z),
                    chunks: vec![20],
                },
            })],
            package: PackageRef {
                file: "package.json".into(),
                length: package.len() as u64,
                sha256: sha256_hex(&package),
                image: piece("fw-esp32c6-merged.bin", &image),
            },
        };
        manifest.validate().expect("the synthetic release is valid");
        let files = BTreeMap::from([
            ("core.bin", core),
            ("engine.bin", engine),
            ("core.z", core_z),
            ("engine.z", engine_z),
            ("package.json", package),
            ("fw-esp32c6-merged.bin", image),
        ]);
        Self {
            manifest_bytes: manifest.to_json_bytes(),
            manifest,
            files,
        }
    }

    /// A stub upstream holding this release (as its version and as latest).
    fn stub(&self) -> Arc<StubUpstream> {
        let stub = StubUpstream::empty();
        self.put_into(&stub);
        stub.put(
            latest_asset("ota-manifest.json"),
            self.manifest_bytes.clone(),
        );
        stub
    }

    /// Put this release's manifest and files into `stub`, under its version.
    fn put_into(&self, stub: &StubUpstream) {
        let version = &self.manifest.version;
        stub.put(
            asset_at(version, "ota-manifest.json"),
            self.manifest_bytes.clone(),
        );
        for (file, bytes) in &self.files {
            stub.put(asset_at(version, file), bytes.clone());
        }
    }

    /// Every asset name this release uploads: its manifest and each file.
    fn asset_names(&self) -> Vec<String> {
        std::iter::once("ota-manifest.json")
            .chain(self.files.keys().copied())
            .map(|file| format!("{TARGET}.{file}"))
            .collect()
    }

    fn serve(&self) -> (TestServer, Arc<StubUpstream>) {
        self.stub().serve()
    }
}

/// An upstream that answers from a map, records every asset asked for, and
/// fails on demand.
#[derive(Default)]
struct StubUpstream {
    assets: Mutex<Vec<(UpstreamAsset, Vec<u8>)>>,
    calls: Mutex<Vec<UpstreamAsset>>,
    failure: Mutex<Option<UpstreamError>>,
}

impl StubUpstream {
    fn empty() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn put(&self, asset: UpstreamAsset, bytes: Vec<u8>) {
        let mut assets = self.assets.lock().unwrap();
        assets.retain(|(held, _)| *held != asset);
        assets.push((asset, bytes));
    }

    fn fail_with(&self, failure: Option<UpstreamError>) {
        *self.failure.lock().unwrap() = failure;
    }

    fn calls(&self) -> Vec<UpstreamAsset> {
        self.calls.lock().unwrap().clone()
    }

    fn serve(self: Arc<Self>) -> (TestServer, Arc<Self>) {
        (TestServer::with_firmware_upstream(self.clone()), self)
    }
}

impl FirmwareUpstream for StubUpstream {
    fn fetch(&self, asset: UpstreamAsset) -> UpstreamFuture<'_> {
        self.calls.lock().unwrap().push(asset.clone());
        let reply = match self.failure.lock().unwrap().clone() {
            Some(error) => Err(error),
            None => Ok(self
                .assets
                .lock()
                .unwrap()
                .iter()
                .find(|(held, _)| *held == asset)
                .map(|(_, bytes)| bytes.clone())),
        };
        Box::pin(async move { reply })
    }
}

/// A loopback server laid out like GitHub releases.
struct StubGithub;

impl StubGithub {
    async fn start() -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a loopback port");
        let base = format!("http://{}", listener.local_addr().unwrap());
        let router = Router::new()
            .route(
                "/releases/download/v2026.10.05-3/esp32c6-4mb.ota-manifest.json",
                get(|| async { "the manifest" }),
            )
            .route(
                "/releases/latest/download/esp32c6-4mb.ota-manifest.json",
                get(|| async {
                    Redirect::to("/releases/download/v2026.10.05-3/esp32c6-4mb.ota-manifest.json")
                }),
            )
            .route(
                "/releases/download/v2026.10.05-3/esp32c6-4mb.fails",
                get(|| async { StatusCode::INTERNAL_SERVER_ERROR.into_response() }),
            )
            .route(
                "/releases/download/v2026.10.05-3/esp32c6-4mb.big",
                get(|| async { vec![0u8; 4096] }),
            );
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        base
    }
}

fn target() -> TargetName {
    TargetName::parse(TARGET).unwrap()
}

/// The index `plane` answers for `target` at `now`, parsed and validated.
async fn index_at(plane: &FirmwarePlane, target: &TargetName, now: f64) -> ReleaseIndex {
    let rendered = plane.release_index(target, now).await.unwrap();
    ReleaseIndex::parse_valid(&rendered.bytes).unwrap()
}

fn versions(index: &ReleaseIndex) -> Vec<&str> {
    index.releases.iter().map(|e| e.version.as_str()).collect()
}

fn sorted(mut assets: Vec<UpstreamAsset>) -> Vec<UpstreamAsset> {
    assets.sort_by_key(|asset| format!("{asset:?}"));
    assets
}

/// A GitHub releases-list body.
fn github_list(releases: &[Value]) -> Vec<u8> {
    serde_json::to_vec(&Value::Array(releases.to_vec())).unwrap()
}

/// `release` as GitHub lists it once its upload is done: published, tagged
/// `v<version>`, every asset `uploaded` — with the noise GitHub sends beside
/// it, which the reader ignores.
fn listed(release: &Release, published_at: Option<&str>) -> Value {
    json!({
        "tag_name": format!("v{}", release.manifest.version),
        "draft": false,
        "prerelease": false,
        "published_at": published_at,
        "body": "release notes",
        "author": { "login": "github-actions[bot]" },
        "assets": asset_list(&release.asset_names()),
    })
}

fn asset_list(names: &[String]) -> Value {
    Value::Array(
        names
            .iter()
            .map(|name| json!({ "name": name, "state": "uploaded", "download_count": 7 }))
            .collect(),
    )
}

/// A releases list that answers a body with an ETag (a 304 when that ETag
/// comes back), records every ETag it is sent, and fails on demand.
struct StubReleaseList {
    body: Mutex<Vec<u8>>,
    etag: Mutex<String>,
    calls: Mutex<Vec<Option<String>>>,
    answers: Mutex<Vec<&'static str>>,
    failure: Mutex<Option<UpstreamError>>,
}

impl StubReleaseList {
    fn new(body: Vec<u8>) -> Arc<Self> {
        let list = Arc::new(Self {
            body: Mutex::new(Vec::new()),
            etag: Mutex::new(String::new()),
            calls: Mutex::new(Vec::new()),
            answers: Mutex::new(Vec::new()),
            failure: Mutex::new(None),
        });
        list.set(body);
        list
    }

    /// A new body, and an ETag that follows it.
    fn set(&self, body: Vec<u8>) {
        *self.etag.lock().unwrap() = format!("W/\"{}\"", sha256_hex(&body));
        *self.body.lock().unwrap() = body;
    }

    /// The same body under a new ETag.
    fn set_etag_only(&self, etag: &str) {
        *self.etag.lock().unwrap() = etag.to_string();
    }

    fn etag(&self) -> String {
        self.etag.lock().unwrap().clone()
    }

    fn calls(&self) -> Vec<Option<String>> {
        self.calls.lock().unwrap().clone()
    }

    fn answers(&self) -> Vec<&'static str> {
        self.answers.lock().unwrap().clone()
    }

    fn fail_with(&self, failure: Option<UpstreamError>) {
        *self.failure.lock().unwrap() = failure;
    }
}

impl ReleaseListUpstream for StubReleaseList {
    fn list(&self, etag: Option<String>) -> ReleaseListFuture<'_> {
        self.calls.lock().unwrap().push(etag.clone());
        let current = self.etag();
        let failure = self.failure.lock().unwrap().clone();
        let reply = match failure {
            Some(error) => Err(error),
            None if etag.as_deref() == Some(current.as_str()) => {
                self.answers.lock().unwrap().push("not-modified");
                Ok(ReleaseListFetch::NotModified)
            }
            None => {
                self.answers.lock().unwrap().push("fresh");
                Ok(ReleaseListFetch::Fresh {
                    bytes: self.body.lock().unwrap().clone(),
                    etag: Some(current),
                })
            }
        };
        Box::pin(async move { reply })
    }
}

/// One request the GitHub API stub saw.
#[derive(Debug, Clone)]
struct SeenRequest {
    path: String,
    authorization: Option<String>,
    accept: Option<String>,
    if_none_match: Option<String>,
}

/// A loopback server standing in for both GitHub hosts: the REST list (with
/// an ETag, a 304 and a rate-limited path) and the release download host,
/// recording each request's headers.
struct StubGithubApi {
    base: String,
    seen: Arc<Mutex<Vec<SeenRequest>>>,
}

impl StubGithubApi {
    const BODY: &str = "[]";
    const ETAG: &str = "W/\"list-v1\"";
    const DOWNLOAD: &str = "/releases/download/v2026.10.05-3/esp32c6-4mb.ota-manifest.json";

    async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a loopback port");
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen: Arc<Mutex<Vec<SeenRequest>>> = Arc::default();
        let router = Router::new()
            .route("/api/releases", get(Self::answer))
            .route("/api/limited", get(Self::answer))
            .route(Self::DOWNLOAD, get(Self::answer))
            .with_state(Arc::clone(&seen));
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        Self { base, seen }
    }

    async fn answer(
        axum::extract::State(seen): axum::extract::State<Arc<Mutex<Vec<SeenRequest>>>>,
        uri: axum::http::Uri,
        headers: HeaderMap,
    ) -> axum::response::Response {
        let get = |name: header::HeaderName| {
            headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        };
        let request = SeenRequest {
            path: uri.path().to_string(),
            authorization: get(header::AUTHORIZATION),
            accept: get(header::ACCEPT),
            if_none_match: get(header::IF_NONE_MATCH),
        };
        seen.lock().unwrap().push(request.clone());
        match request.path.as_str() {
            "/api/releases" if request.if_none_match.as_deref() == Some(Self::ETAG) => {
                StatusCode::NOT_MODIFIED.into_response()
            }
            "/api/releases" => ([(header::ETAG, Self::ETAG)], Self::BODY).into_response(),
            "/api/limited" => StatusCode::FORBIDDEN.into_response(),
            _ => "the manifest".into_response(),
        }
    }

    fn seen(&self) -> Vec<SeenRequest> {
        self.seen.lock().unwrap().clone()
    }
}
