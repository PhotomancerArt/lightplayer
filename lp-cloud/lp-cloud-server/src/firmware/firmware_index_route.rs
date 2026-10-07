//! `GET|HEAD|OPTIONS /firmware/{target}/releases` — the release index and
//! its headers.
//!
//! | Answer | When | `Cache-Control` |
//! |---|---|---|
//! | 200 | the index, format 1 (`application/json`) | `public, max-age=60` |
//! | 304 | `If-None-Match` names the ETag | same |
//! | 404 | a target outside the grammar, or one no installable release carries | `public, max-age=60` |
//! | 502 / 504 | no good copy of the releases list and upstream failed (504: timed out) | `no-store` |
//! | 204 | `OPTIONS` | — |
//!
//! Like the lookup, **every** answer carries `Access-Control-Allow-Origin: *`
//! and `X-Content-Type-Options: nosniff`, never credentials. The ETag is
//! the quoted SHA-256 of the index bytes. The index changes as releases are
//! published, so it is cached for a minute, not forever; the server's own
//! copy of the list is revalidated every five minutes and served stale for
//! up to a day while GitHub fails.

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::Response;
use lp_cloud_domain::Clock as _;
use lpc_firmware_release::TargetName;

use super::firmware_route::{
    SHORT_CACHE_CONTROL, common_headers, failed, if_none_match, not_found,
};
use super::release_index_cache::RenderedReleaseIndex;
use crate::app_state::AppState;
use crate::ports::SystemClock;

/// `GET` (and `HEAD`) the release index of `target`.
pub async fn get_release_index(
    State(state): State<AppState>,
    Path(target): Path<String>,
    headers: HeaderMap,
) -> Response {
    let Some(target) = TargetName::parse(&target) else {
        return not_found("no such target");
    };
    match state
        .firmware()
        .release_index(&target, SystemClock.now())
        .await
    {
        Ok(index) => answer(&headers, &index),
        Err(failure) => failed(failure),
    }
}

fn answer(headers: &HeaderMap, index: &RenderedReleaseIndex) -> Response {
    let not_modified = if_none_match(headers, &index.sha256);
    let mut response = if not_modified {
        let mut response = Response::new(Body::empty());
        *response.status_mut() = StatusCode::NOT_MODIFIED;
        response
    } else {
        Response::new(Body::from(index.bytes.clone()))
    };
    let h = response.headers_mut();
    common_headers(h);
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(SHORT_CACHE_CONTROL),
    );
    h.insert(
        header::ETAG,
        HeaderValue::from_str(&format!("\"{}\"", index.sha256))
            .expect("hex is a valid header value"),
    );
    if !not_modified {
        h.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        h.insert(header::CONTENT_LENGTH, HeaderValue::from(index.bytes.len()));
    }
    response
}
