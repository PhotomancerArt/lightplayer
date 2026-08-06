//! `/b/{hash}` and `/t/{hash}`: hash verification, the package-hash rule for
//! trees, auth on writes, and immutable caching on reads.

mod edge_harness;

use axum::http::{StatusCode, header};
use edge_harness::{TestServer, body_bytes, body_text, header_value};
use lpc_cloud_api::request::HaveBlobs;
use lpc_cloud_api::response::MissingBlobs;
use lpc_cloud_api::{CloudRequest, CloudResponse};
use lpc_history::{ContentHash, TreeEntry, TreeManifest};
use lpfs::LpPathBuf;

/// The round trip, and the header that makes a content-addressed read free
/// the second time.
#[tokio::test]
async fn a_blob_round_trips_and_is_cached_forever() {
    let server = TestServer::new();
    let session = server.sign_in("yona@example.com").await;
    let bytes = b"a project file".to_vec();
    let hash = ContentHash::of(&bytes);

    let put = server
        .put(&format!("/b/{hash}"), bytes.clone(), Some(&session))
        .await;
    assert_eq!(put.status(), StatusCode::OK);
    assert_eq!(body_text(put).await, hash.to_string());

    let get = server.get(&format!("/b/{hash}")).await;
    assert_eq!(get.status(), StatusCode::OK);
    assert_eq!(
        header_value(&get, header::CACHE_CONTROL),
        Some("public, max-age=31536000, immutable")
    );
    assert_eq!(body_bytes(get).await, bytes);
}

/// The address never depends on what the uploader claimed. A store that
/// takes the caller's word for the hash can be made to lie.
#[tokio::test]
async fn a_blob_whose_body_does_not_hash_to_its_address_is_refused() {
    let server = TestServer::new();
    let session = server.sign_in("yona@example.com").await;
    let claimed = ContentHash::of(b"what I said");

    let response = server
        .put(
            &format!("/b/{claimed}"),
            b"what I sent".to_vec(),
            Some(&session),
        )
        .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    // and nothing was stored at the claimed address
    assert_eq!(
        server.get(&format!("/b/{claimed}")).await.status(),
        StatusCode::NOT_FOUND
    );
}

/// Reads are open (the hash is the capability, and an unfurler fetching
/// `og:image` has no session); writes are not.
#[tokio::test]
async fn uploading_requires_a_session_but_reading_does_not() {
    let server = TestServer::new();
    let bytes = b"anonymous upload".to_vec();
    let hash = ContentHash::of(&bytes);

    let anonymous = server.put(&format!("/b/{hash}"), bytes.clone(), None).await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

    let session = server.sign_in("yona@example.com").await;
    server
        .put(&format!("/b/{hash}"), bytes, Some(&session))
        .await;
    assert_eq!(
        server.get(&format!("/b/{hash}")).await.status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn a_hash_that_is_not_a_hash_is_a_400_and_a_missing_blob_is_a_404() {
    let server = TestServer::new();

    assert_eq!(
        server.get("/b/not-a-hash").await.status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        server
            .get(&format!("/b/{}", ContentHash::of(b"never uploaded")))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}

/// A tree lives at its **package** hash, which is not the hash of the JSON
/// that carries it — the whole reason `/t/` exists alongside `/b/`.
#[tokio::test]
async fn a_tree_is_addressed_by_its_package_hash() {
    let server = TestServer::new();
    let session = server.sign_in("yona@example.com").await;
    let manifest = manifest();
    let json = serde_json::to_vec(&manifest).unwrap();
    let package = manifest.package_hash();
    let json_hash = ContentHash::of(&json);
    assert_ne!(package, json_hash, "the premise of this test");

    let put = server
        .put(&format!("/t/{package}"), json.clone(), Some(&session))
        .await;
    assert_eq!(put.status(), StatusCode::OK);
    assert_eq!(body_text(put).await, package.to_string());

    // and it is not reachable at the JSON's own hash
    assert_eq!(
        server.get(&format!("/t/{json_hash}")).await.status(),
        StatusCode::NOT_FOUND
    );

    let get = server.get(&format!("/t/{package}")).await;
    assert_eq!(get.status(), StatusCode::OK);
    let fetched: TreeManifest = serde_json::from_slice(&body_bytes(get).await).unwrap();
    assert_eq!(fetched, manifest);
    // the receiver's rule, applied here as the client applies it
    assert_eq!(fetched.package_hash(), package);
}

/// Verification recomputes the package hash rather than hashing the body, so
/// a manifest offered at somebody else's address is refused.
#[tokio::test]
async fn a_tree_at_the_wrong_address_is_refused() {
    let server = TestServer::new();
    let session = server.sign_in("yona@example.com").await;
    let manifest = manifest();
    let json = serde_json::to_vec(&manifest).unwrap();
    let wrong = ContentHash::of(b"some other version");

    let response = server
        .put(&format!("/t/{wrong}"), json.clone(), Some(&session))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // the JSON's own hash is the tempting wrong answer, and it is refused too
    let response = server
        .put(
            &format!("/t/{}", ContentHash::of(&json)),
            json,
            Some(&session),
        )
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_tree_upload_requires_a_session_and_a_manifest() {
    let server = TestServer::new();
    let manifest = manifest();
    let json = serde_json::to_vec(&manifest).unwrap();
    let package = manifest.package_hash();

    assert_eq!(
        server
            .put(&format!("/t/{package}"), json, None)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );

    let session = server.sign_in("yona@example.com").await;
    assert_eq!(
        server
            .put(&format!("/t/{package}"), b"{}".to_vec(), Some(&session))
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
}

/// The two planes share one store, so an uploaded tree has to be visible to
/// the blob *index* — that is what a push checks before it accepts a commit.
#[tokio::test]
async fn an_uploaded_tree_is_in_the_blob_index() {
    let server = TestServer::new();
    let session = server.sign_in("yona@example.com").await;
    let manifest = manifest();
    let package = manifest.package_hash();
    server
        .put(
            &format!("/t/{package}"),
            serde_json::to_vec(&manifest).unwrap(),
            Some(&session),
        )
        .await;

    let reply = server
        .call(
            CloudRequest::HaveBlobs(HaveBlobs {
                hashes: vec![package, ContentHash::of(b"absent")],
            }),
            Some(&session),
        )
        .await;

    assert_eq!(
        reply.result,
        Ok(CloudResponse::MissingBlobs(MissingBlobs {
            hashes: vec![ContentHash::of(b"absent")],
        }))
    );
}

fn manifest() -> TreeManifest {
    TreeManifest::from_entries(vec![
        TreeEntry {
            path: LpPathBuf::from("/project.json"),
            hash: ContentHash::of(b"{}"),
        },
        TreeEntry {
            path: LpPathBuf::from("/shader.glsl"),
            hash: ContentHash::of(b"void main() {}"),
        },
    ])
    .unwrap()
}
