//! `GET`/`PUT /t/{hash}` — tree manifests, addressed by package hash.
//!
//! The wire body is the manifest's JSON, exactly as
//! [`TreeManifest`](lpc_history::TreeManifest) serializes. The address is
//! **not** that JSON's hash: it is
//! [`TreeManifest::package_hash`](lpc_history::TreeManifest::package_hash),
//! the canonical `lph1` hash over the entries, which is what a commit's
//! `tree` field carries and what `has_blob` is asked about on push.
//!
//! So verification here recomputes the *package* hash rather than hashing
//! the body — the same rule the client applies when it fetches (see
//! `lpa-cloud-client`'s `fetch_version`, which refuses a manifest whose
//! package hash is not the one it asked for). Storage then converts to the
//! canonical preimage, whose SHA-256 *is* the package hash, so the generic
//! content-addressed blob store files it at the right address with no side
//! index: [`crate::content::tree_preimage`].

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use lp_cloud_domain::BlobStore as _;
use lpc_cloud_api::Actor;
use lpc_history::TreeManifest;

use crate::app_state::AppState;
use crate::auth::session_cookie::session_token;
use crate::content::blob_route::parse_hash;
use crate::content::{IMMUTABLE_CACHE_CONTROL, tree_preimage};

/// Fetch the manifest stored at a package hash.
pub async fn get_tree(State(state): State<AppState>, Path(hash): Path<String>) -> Response {
    let Some(hash) = parse_hash(&hash) else {
        return (StatusCode::BAD_REQUEST, "not a content hash\n").into_response();
    };

    let Some(bytes) = state.with_service(move |core| core.blobs.get(hash)).await else {
        return (StatusCode::NOT_FOUND, "no such tree\n").into_response();
    };

    let manifest = match tree_preimage::decode(&bytes) {
        Ok(manifest) => manifest,
        // The address resolved but the bytes are not a tree — either a file
        // blob was requested on the tree plane, or this build no longer
        // understands the stored format. Not found, either way: there is no
        // tree here to hand over.
        Err(error) => {
            log::warn!("blob {hash} is not a tree preimage: {error}");
            return (StatusCode::NOT_FOUND, "no such tree\n").into_response();
        }
    };

    match serde_json::to_vec(&manifest) {
        Ok(json) => (
            [
                (header::CONTENT_TYPE, "application/json"),
                (header::CACHE_CONTROL, IMMUTABLE_CACHE_CONTROL),
            ],
            json,
        )
            .into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("could not serialize the tree: {error}\n"),
        )
            .into_response(),
    }
}

/// Upload a manifest. Idempotent, like every content-addressed write.
pub async fn put_tree(
    State(state): State<AppState>,
    Path(hash): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(claimed) = parse_hash(&hash) else {
        return (StatusCode::BAD_REQUEST, "not a content hash\n").into_response();
    };
    let manifest: TreeManifest = match serde_json::from_slice(&body) {
        Ok(manifest) => manifest,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                format!("not a TreeManifest: {error}\n"),
            )
                .into_response();
        }
    };

    let actual = manifest.package_hash();
    if actual != claimed {
        return (
            StatusCode::BAD_REQUEST,
            format!("manifest packages to {actual}, not {claimed}\n"),
        )
            .into_response();
    }

    let token = session_token(&headers);
    let stored = state
        .with_service(move |core| match core.actor_for(token.as_deref()) {
            Actor::Anonymous => None,
            Actor::User(_) => Some(core.store_blob(&tree_preimage::encode(&manifest))),
        })
        .await;

    match stored {
        // Belt and braces: the store derives the address from the preimage
        // and we derived it from the entries. They agree by construction
        // (`tree_preimage`'s round-trip test), and if they ever stopped, a
        // silently mis-filed tree would be far worse than a 500.
        Some(stored) if stored == claimed => (StatusCode::OK, stored.to_string()).into_response(),
        Some(stored) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("tree stored at {stored}, expected {claimed}\n"),
        )
            .into_response(),
        None => (
            StatusCode::UNAUTHORIZED,
            "a session is required to upload\n",
        )
            .into_response(),
    }
}
