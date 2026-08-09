//! `GET`/`PUT /b/{hash}` — file blobs.
//!
//! # Reading is open, writing is not
//!
//! A `GET` answers to anybody. That is not an oversight: a 256-bit content
//! hash *is* the capability, the blob index holds no project association to
//! check against, and the anonymous reader is a real user — a link unfurler
//! fetching `og:image`, and the no-login viewer the whole feature exists for
//! (D24). What the hash does not grant is discovery: you cannot list, and
//! you cannot guess.
//!
//! A `PUT` answers to anybody too — since API v3, an anonymous holder of an
//! `Access::Edit` link may push, and a push's blobs travel this plane, so a
//! session gate here would break the very flow the domain allows. The write
//! is hash-verified and idempotent (same bytes, same address; nothing is
//! ever overwritten), which bounds the damage to unmetered storage — a real
//! surface, owned by `docs/debt/cloud-abuse-quota-posture.md` (quotas before
//! any public announcement), not by an auth check that would contradict the
//! access model.
//!
//! # Verification
//!
//! `PUT` recomputes SHA-256 over the body and refuses a mismatch, so the
//! address never depends on what the uploader claimed. The store's own `put`
//! then derives the address a second time from the same bytes — the port is
//! shaped so a caller *cannot* name an address (see
//! [`BlobStore`](lp_cloud_domain::BlobStore)).

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use lp_cloud_domain::BlobStore as _;
use lpc_history::ContentHash;

use crate::app_state::AppState;
use crate::content::{IMMUTABLE_CACHE_CONTROL, content_type};

/// Fetch a blob's bytes.
pub async fn get_blob(State(state): State<AppState>, Path(hash): Path<String>) -> Response {
    let Some(hash) = parse_hash(&hash) else {
        return (StatusCode::BAD_REQUEST, "not a content hash\n").into_response();
    };

    match state.with_service(move |core| core.blobs.get(hash)).await {
        Some(bytes) => (
            [
                (header::CONTENT_TYPE, content_type::sniff(&bytes)),
                (header::CACHE_CONTROL, IMMUTABLE_CACHE_CONTROL),
            ],
            bytes,
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "no such blob\n").into_response(),
    }
}

/// Upload a blob. Idempotent: the same bytes land at the same address.
pub async fn put_blob(
    State(state): State<AppState>,
    Path(hash): Path<String>,
    body: Bytes,
) -> Response {
    let Some(claimed) = parse_hash(&hash) else {
        return (StatusCode::BAD_REQUEST, "not a content hash\n").into_response();
    };
    let actual = ContentHash::of(&body);
    if actual != claimed {
        return (
            StatusCode::BAD_REQUEST,
            format!("body hashes to {actual}, not {claimed}\n"),
        )
            .into_response();
    }

    let stored = state.with_service(move |core| core.store_blob(&body)).await;
    (StatusCode::OK, stored.to_string()).into_response()
}

/// A hash out of a URL segment, or `None` if it is not one.
pub(crate) fn parse_hash(raw: &str) -> Option<ContentHash> {
    raw.parse().ok()
}
