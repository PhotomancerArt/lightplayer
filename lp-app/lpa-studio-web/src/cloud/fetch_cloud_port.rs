//! The browser's `CloudPort`: `fetch` against the deployed service.
//!
//! The mirror image of `lpa-cloud-client`'s `InProcessCloud` — same trait,
//! same semantics, an HTTP wire in between. Both planes the trait names have
//! a route on the server (`lp-cloud-server`'s `router.rs`):
//!
//! | Method | Wire |
//! |---|---|
//! | `call` | `POST /api`, a `CloudCall` in, a `CloudReply` out |
//! | `get_blob` / `put_blob` | `GET` / `PUT /b/<hash>`, raw bytes |
//! | `get_tree` / `put_tree` | `GET` / `PUT /t/<hash>`, manifest JSON at the **package** hash |
//!
//! # Same-origin, and why that is the whole auth story
//!
//! Every URL here is **relative**. The session is a cookie the server set on
//! its own origin, so a same-origin request carries it with no header, no
//! token in JS, and no CORS preflight — which is also why dev runs through
//! the `Dioxus.toml` `[[web.proxy]]` entries rather than pointing at
//! `localhost:2812` directly (Q14): a cross-origin dev setup would need a
//! cookie posture the deployed one does not have, and would therefore be
//! testing something else.
//!
//! # What is a transport error here
//!
//! [`TransportError`] means "the conversation did not happen"; a service that
//! considered the request and said no answers `200 OK` with a `CloudError`
//! inside the reply, and never appears here. So:
//!
//! - `fetch` itself failing (no network, DNS, refused connection) ⇒
//!   [`TransportError::Offline`].
//! - `502`/`503`/`504` ⇒ [`TransportError::Offline`] too: a proxy answering
//!   for a service it could not reach is the offline case wearing a status
//!   code.
//! - `404` on a content-plane `GET` ⇒ [`TransportError::MissingBlob`].
//! - anything else non-2xx, and every decode failure ⇒
//!   [`TransportError::Protocol`]. That deliberately includes the `400` a
//!   service too old to decode this build's `CloudCall` returns: it is a
//!   version disagreement the *envelope* never got to express, so it cannot
//!   come back as a typed `CloudError`.
//!
//! # Addresses are recomputed, never taken on trust
//!
//! Both `get_` methods rehash what came back and refuse a mismatch, exactly
//! as the server does on the way in. A content-addressed store whose reader
//! believes the label is not content-addressed.

use lpa_cloud_client::cloud_port::{CloudPort, TransportError};
use lpc_cloud_api::{CloudCall, CloudReply};
use lpc_history::{ContentHash, TreeManifest};

/// The control plane's one door.
pub const API_PATH: &str = "/api";
/// The file-blob plane, `<prefix><hash>`.
pub const BLOB_PREFIX: &str = "/b/";
/// The tree plane, `<prefix><package hash>`.
pub const TREE_PREFIX: &str = "/t/";

/// A `CloudPort` that talks to whatever service served this page.
///
/// Stateless by construction: the session lives in a cookie the browser
/// attaches, so there is nothing for this type to hold and every instance is
/// the same instance.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FetchCloudPort;

impl FetchCloudPort {
    /// The port. (A constructor rather than the unit literal so call sites
    /// read the same if this ever grows a base path.)
    pub fn new() -> Self {
        Self
    }
}

impl CloudPort for FetchCloudPort {
    async fn call(&self, call: CloudCall) -> Result<CloudReply, TransportError> {
        let body = serde_json::to_string(&call)
            .map_err(|error| protocol("POST /api", format_args!("unencodable call: {error}")))?;
        let response = gloo_net::http::Request::post(API_PATH)
            .header("content-type", "application/json")
            .body(body)
            .map_err(|error| protocol("POST /api", format_args!("unbuildable request: {error}")))?
            .send()
            .await
            .map_err(|_| TransportError::Offline)?;
        let text = read_text("POST /api", response).await?;
        serde_json::from_str(&text)
            .map_err(|error| protocol("POST /api", format_args!("not a CloudReply: {error}")))
    }

    async fn get_blob(&self, hash: ContentHash) -> Result<Vec<u8>, TransportError> {
        let url = format!("{BLOB_PREFIX}{hash}");
        let response = gloo_net::http::Request::get(&url)
            .send()
            .await
            .map_err(|_| TransportError::Offline)?;
        if response.status() == 404 {
            return Err(TransportError::MissingBlob(hash));
        }
        if !response.ok() {
            return Err(status_error(&url, response.status()));
        }
        let bytes = response
            .binary()
            .await
            .map_err(|error| protocol(&url, format_args!("unreadable body: {error}")))?;
        let actual = ContentHash::of(&bytes);
        if actual != hash {
            return Err(protocol(&url, format_args!("body hashes to {actual}")));
        }
        Ok(bytes)
    }

    async fn put_blob(&self, bytes: &[u8]) -> Result<ContentHash, TransportError> {
        // The address is derived here, from the bytes — the server derives it
        // again and refuses a mismatch, so neither side takes the other's
        // word for where this belongs.
        let hash = ContentHash::of(bytes);
        let url = format!("{BLOB_PREFIX}{hash}");
        let response = gloo_net::http::Request::put(&url)
            .header("content-type", "application/octet-stream")
            .body(js_sys::Uint8Array::from(bytes))
            .map_err(|error| protocol(&url, format_args!("unbuildable request: {error}")))?
            .send()
            .await
            .map_err(|_| TransportError::Offline)?;
        let stored = read_text(&url, response).await?;
        confirm_stored_at(&url, &stored, hash)
    }

    async fn get_tree(&self, package_hash: ContentHash) -> Result<TreeManifest, TransportError> {
        let url = format!("{TREE_PREFIX}{package_hash}");
        let response = gloo_net::http::Request::get(&url)
            .send()
            .await
            .map_err(|_| TransportError::Offline)?;
        if response.status() == 404 {
            return Err(TransportError::MissingBlob(package_hash));
        }
        if !response.ok() {
            return Err(status_error(&url, response.status()));
        }
        let text = response
            .text()
            .await
            .map_err(|error| protocol(&url, format_args!("unreadable body: {error}")))?;
        let manifest: TreeManifest = serde_json::from_str(&text)
            .map_err(|error| protocol(&url, format_args!("not a TreeManifest: {error}")))?;
        // The tree's address is the package hash, not the JSON's hash — so
        // the check is a repackage, not a rehash.
        let actual = manifest.package_hash();
        if actual != package_hash {
            return Err(protocol(
                &url,
                format_args!("manifest packages to {actual}"),
            ));
        }
        Ok(manifest)
    }

    async fn put_tree(&self, manifest: &TreeManifest) -> Result<ContentHash, TransportError> {
        let hash = manifest.package_hash();
        let url = format!("{TREE_PREFIX}{hash}");
        let body = serde_json::to_string(manifest)
            .map_err(|error| protocol(&url, format_args!("unencodable manifest: {error}")))?;
        let response = gloo_net::http::Request::put(&url)
            .header("content-type", "application/json")
            .body(body)
            .map_err(|error| protocol(&url, format_args!("unbuildable request: {error}")))?
            .send()
            .await
            .map_err(|_| TransportError::Offline)?;
        let stored = read_text(&url, response).await?;
        confirm_stored_at(&url, &stored, hash)
    }
}

/// A non-2xx status, sorted into the two families the trait names.
fn status_error(what: &str, status: u16) -> TransportError {
    // A gateway status is the service being unreachable *through* something
    // that is reachable — the retryable family, not a protocol violation.
    if matches!(status, 502 | 503 | 504) {
        TransportError::Offline
    } else {
        TransportError::Protocol(format!("{what}: HTTP {status}"))
    }
}

fn protocol(what: &str, detail: core::fmt::Arguments<'_>) -> TransportError {
    TransportError::Protocol(format!("{what}: {detail}"))
}

/// Status-check a response and read its body as text.
async fn read_text(
    what: &str,
    response: gloo_net::http::Response,
) -> Result<String, TransportError> {
    if !response.ok() {
        return Err(status_error(what, response.status()));
    }
    response
        .text()
        .await
        .map_err(|error| protocol(what, format_args!("unreadable body: {error}")))
}

/// A `PUT` answers with the address it filed the body at. It must be the one
/// we derived, or one of the two sides is computing addresses wrongly and a
/// silently mis-filed object is far worse than a refused upload.
fn confirm_stored_at(
    what: &str,
    answer: &str,
    expected: ContentHash,
) -> Result<ContentHash, TransportError> {
    let stored: ContentHash = answer
        .trim()
        .parse()
        .map_err(|_| protocol(what, format_args!("answer is not a content hash")))?;
    if stored == expected {
        Ok(expected)
    } else {
        Err(protocol(what, format_args!("stored at {stored} instead")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The paths this client uses are the ones `lp-cloud-server`'s router
    /// defines. A rename on either side should break a test, not a deploy.
    #[test]
    fn plane_paths_match_the_server_routes() {
        let hash = ContentHash::of(b"content");
        assert_eq!(API_PATH, "/api");
        assert_eq!(format!("{BLOB_PREFIX}{hash}"), format!("/b/{hash}"));
        assert_eq!(format!("{TREE_PREFIX}{hash}"), format!("/t/{hash}"));
    }

    /// Gateway statuses are the offline family; everything else non-2xx is a
    /// protocol violation, including the 400 a stale service returns for a
    /// `CloudCall` it cannot decode.
    #[test]
    fn statuses_sort_into_the_right_family() {
        assert_eq!(status_error("POST /api", 503), TransportError::Offline);
        assert_eq!(status_error("POST /api", 502), TransportError::Offline);
        assert!(matches!(
            status_error("POST /api", 400),
            TransportError::Protocol(_)
        ));
        assert!(matches!(
            status_error("POST /api", 401),
            TransportError::Protocol(_)
        ));
    }

    #[test]
    fn a_put_answer_naming_another_address_is_refused() {
        let ours = ContentHash::of(b"ours");
        let theirs = ContentHash::of(b"theirs");
        assert_eq!(
            confirm_stored_at("PUT /b/…", &ours.to_string(), ours),
            Ok(ours)
        );
        assert!(matches!(
            confirm_stored_at("PUT /b/…", &theirs.to_string(), ours),
            Err(TransportError::Protocol(_))
        ));
        assert!(matches!(
            confirm_stored_at("PUT /b/…", "ok", ours),
            Err(TransportError::Protocol(_))
        ));
    }
}
