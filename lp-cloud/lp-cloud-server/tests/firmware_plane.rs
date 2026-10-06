//! The firmware plane: `/firmware/{target}/{release}/{file}`.
//!
//! The router is driven with `oneshot` against an in-process
//! [`FirmwareUpstream`] stub that records every asset it is asked for, so
//! each test can say exactly how many upstream calls an answer cost — the
//! property that matters most is that a 404 decided by the grammar costs
//! none. The release is a synthetic one built with `lpc-firmware-release`'s
//! own types over small byte strings; no firmware is needed.
//!
//! The last tests drive [`GithubReleaseUpstream`] itself against a loopback
//! stub server (the `google_auth.rs` pattern): its two URL forms, a redirect
//! and the body cap.

mod edge_harness;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Redirect};
use axum::routing::get;
use edge_harness::{
    BUNDLE_FIRMWARE_MANIFEST_BODY, BUNDLE_FIRMWARE_MANIFEST_PATH, BUNDLE_UPDATE_FILES, TestServer,
    body_bytes, body_text, header_value,
};
use lp_cloud_server::firmware::firmware_upstream::{
    FirmwareUpstream, UpstreamAsset, UpstreamError, UpstreamFuture,
};
use lp_cloud_server::firmware::github_release_upstream::GithubReleaseUpstream;
use lpc_firmware_release::{
    EncodedPieceFile, Encoding1, EncodingEntry, OtaManifest, PackageRef, PieceFile, ReleaseVersion,
    Requires, sha256_hex,
};

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
    UpstreamAsset::Release {
        version: ReleaseVersion::parse(VERSION).unwrap(),
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
            version: VERSION.into(),
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
        stub.put(
            release_asset("ota-manifest.json"),
            self.manifest_bytes.clone(),
        );
        stub.put(
            latest_asset("ota-manifest.json"),
            self.manifest_bytes.clone(),
        );
        for (file, bytes) in &self.files {
            stub.put(release_asset(file), bytes.clone());
        }
        stub
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
