//! `GET|HEAD|OPTIONS /firmware/{target}/{release}/{file}` — every answer and
//! its headers.
//!
//! | Answer | When | `Cache-Control` |
//! |---|---|---|
//! | 200 | the manifest (exact upstream bytes) or a checked file | `public, max-age=31536000, immutable` |
//! | 304 | `If-None-Match` names the ETag | same |
//! | 302 | `latest` → `/firmware/{target}/{version}/{file}` | `public, max-age=60` |
//! | 404 | the grammar refuses it (dev build, reserved word, bad target or file), no such release, a file the manifest does not name | `public, max-age=60` |
//! | 502 / 504 | upstream unreachable, failing, too large, or sending bytes that do not check out (504: timed out) | `no-store` |
//! | 204 | `OPTIONS` | — |
//!
//! **Every** answer, errors and redirects included, carries
//! `Access-Control-Allow-Origin: *` and `X-Content-Type-Options: nosniff`:
//! the bytes are public and verified by hash, and the beta channel and local
//! dev Studios are other origins. No credentials, ever. A 200's `ETag` is the
//! quoted SHA-256 of the bytes. `HEAD` is axum's: the same answer, no body.
//!
//! A 404 decided by the grammar or by a manifest already in hand never asks
//! upstream.

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::Response;
use lp_cloud_domain::{BlobStore as _, Clock as _};
use lpc_firmware_release::{
    FirmwareLookupPath, OTA_MANIFEST_FILE, ReleaseSelector, ReleaseVersion,
};
use lpc_history::ContentHash;

use super::firmware_plane::LookupFailure;
use super::firmware_upstream::UpstreamError;
use crate::app_state::AppState;
use crate::content::IMMUTABLE_CACHE_CONTROL;
use crate::ports::SystemClock;

/// `Cache-Control` for a redirect and for a 404.
pub const SHORT_CACHE_CONTROL: &str = "public, max-age=60";

/// `Cache-Control` for an upstream failure: try again next time.
pub const NO_STORE: &str = "no-store";

/// `GET` (and `HEAD`) one lookup.
pub async fn get_firmware(
    State(state): State<AppState>,
    Path((target, release, file)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> Response {
    let lookup = match FirmwareLookupPath::from_segments(&target, &release, &file) {
        Ok(lookup) => lookup,
        Err(refused) => return not_found(refused.reason()),
    };
    let now = SystemClock.now();
    let plane = state.firmware();
    let target = lookup.target;
    let file = lookup.file;

    let (version, build_id): (ReleaseVersion, _) = match lookup.release {
        ReleaseSelector::Reserved(_) => return not_found("reserved for a future channel"),
        ReleaseSelector::Latest => {
            return match plane.resolve_latest(&target, now).await {
                Ok(version) => redirect(&format!("/firmware/{target}/{version}/{file}")),
                Err(failure) => failed(failure),
            };
        }
        ReleaseSelector::Version(version) => (version, None),
        ReleaseSelector::BuildId(build_id) => (build_id.version().clone(), Some(build_id)),
    };
    let cached = match plane.manifest(&target, &version, now).await {
        Ok(cached) => cached,
        Err(failure) => return failed(failure),
    };
    // The manifest has no `buildId` field: the build id is derived
    // (`version+commit[..12]`), and a build id names exactly one build.
    if let Some(build_id) = build_id
        && cached.manifest.build_id() != build_id.to_string()
    {
        return not_found("no such build");
    }

    if file == OTA_MANIFEST_FILE {
        return found(&headers, &file, &cached.sha256, || cached.bytes.clone());
    }
    let Some(entry) = cached.manifest.file(&file) else {
        return not_found("no such file");
    };
    let sha256 = entry.sha256.to_string();
    if if_none_match(&headers, &sha256) {
        return not_modified(&sha256);
    }
    let Ok(hash) = sha256.parse::<ContentHash>() else {
        return failed(LookupFailure::BadUpstream(format!(
            "manifest hash {sha256} for {file}"
        )));
    };

    if let Some(bytes) = state.with_service(move |core| core.blobs.get(hash)).await {
        return found(&headers, &file, &sha256, || bytes);
    }
    let bytes = match plane
        .fetch_file(&target, &version, &cached.manifest, &file)
        .await
    {
        Ok(bytes) => bytes,
        Err(failure) => return failed(failure),
    };
    let stored = bytes.clone();
    state
        .with_service(move |core| core.blobs.put(&stored))
        .await;
    found(&headers, &file, &sha256, || bytes)
}

/// `OPTIONS`: a preflight answer (no credentials, `GET`/`HEAD` only).
pub async fn options_firmware() -> Response {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::NO_CONTENT;
    let h = response.headers_mut();
    common_headers(h);
    h.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, HEAD"),
    );
    h.insert(
        header::ALLOW,
        HeaderValue::from_static("GET, HEAD, OPTIONS"),
    );
    response
}

fn found(
    headers: &HeaderMap,
    file: &str,
    sha256: &str,
    bytes: impl FnOnce() -> Vec<u8>,
) -> Response {
    if if_none_match(headers, sha256) {
        return not_modified(sha256);
    }
    let bytes = bytes();
    let length = bytes.len();
    let mut response = Response::new(Body::from(bytes));
    let h = response.headers_mut();
    common_headers(h);
    immutable_headers(h, sha256);
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(content_type(file)),
    );
    h.insert(header::CONTENT_LENGTH, HeaderValue::from(length));
    response
}

fn not_modified(sha256: &str) -> Response {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::NOT_MODIFIED;
    let h = response.headers_mut();
    common_headers(h);
    immutable_headers(h, sha256);
    response
}

fn redirect(location: &str) -> Response {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::FOUND;
    let h = response.headers_mut();
    common_headers(h);
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(SHORT_CACHE_CONTROL),
    );
    h.insert(
        header::LOCATION,
        HeaderValue::from_str(location).expect("a lookup path is a valid header value"),
    );
    response
}

pub(super) fn not_found(reason: &str) -> Response {
    text(StatusCode::NOT_FOUND, SHORT_CACHE_CONTROL, reason)
}

pub(super) fn failed(failure: LookupFailure) -> Response {
    match failure {
        LookupFailure::NotFound(reason) => not_found(reason),
        LookupFailure::Upstream(UpstreamError::TimedOut) => {
            log::warn!("firmware: upstream timed out");
            text(StatusCode::GATEWAY_TIMEOUT, NO_STORE, "upstream timed out")
        }
        LookupFailure::Upstream(error) => {
            log::warn!("firmware: {error}");
            text(StatusCode::BAD_GATEWAY, NO_STORE, "upstream failed")
        }
        LookupFailure::BadUpstream(detail) => {
            log::warn!("firmware: upstream bytes refused: {detail}");
            text(
                StatusCode::BAD_GATEWAY,
                NO_STORE,
                "upstream sent bytes that do not match the release manifest",
            )
        }
    }
}

fn text(status: StatusCode, cache_control: &'static str, reason: &str) -> Response {
    let mut response = Response::new(Body::from(format!("{reason}\n")));
    *response.status_mut() = status;
    let h = response.headers_mut();
    common_headers(h);
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(cache_control),
    );
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}

pub(super) fn common_headers(h: &mut HeaderMap) {
    h.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
}

fn immutable_headers(h: &mut HeaderMap, sha256: &str) {
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(IMMUTABLE_CACHE_CONTROL),
    );
    h.insert(
        header::ETAG,
        HeaderValue::from_str(&format!("\"{sha256}\"")).expect("hex is a valid header value"),
    );
}

fn content_type(file: &str) -> &'static str {
    if file.ends_with(".json") {
        "application/json"
    } else {
        "application/octet-stream"
    }
}

/// Whether `If-None-Match` names this ETag (or `*`). Weak tags compare equal:
/// the bytes behind an address never change.
pub(super) fn if_none_match(headers: &HeaderMap, sha256: &str) -> bool {
    let Some(value) = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
    else {
        return false;
    };
    value.split(',').map(str::trim).any(|tag| {
        let tag = tag.strip_prefix("W/").unwrap_or(tag);
        tag == "*" || tag.trim_matches('"') == sha256
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn if_none_match_reads_lists_weak_tags_and_star() {
        let sha = "ab".repeat(32);
        let with = |value: &str| {
            let mut h = HeaderMap::new();
            h.insert(header::IF_NONE_MATCH, HeaderValue::from_str(value).unwrap());
            h
        };
        assert!(if_none_match(&with(&format!("\"{sha}\"")), &sha));
        assert!(if_none_match(&with(&format!("\"x\", W/\"{sha}\"")), &sha));
        assert!(if_none_match(&with("*"), &sha));
        assert!(!if_none_match(&with("\"other\""), &sha));
        assert!(!if_none_match(&HeaderMap::new(), &sha));
    }
}
