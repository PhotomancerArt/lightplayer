//! The browser's [`FirmwareFetch`]: a `fetch` of an absolute store URL.
//!
//! Cross-origin from the beta channel and a local `studio-dev`, so it sends
//! no credentials (the store is public and answers `Access-Control-Allow-
//! Origin: *`), follows redirects (`latest` is a 302), and leaves the
//! browser's HTTP cache on its default: the store marks versioned paths
//! immutable and `latest` for 60 s.
//!
//! The status mapping follows `cloud/fetch_cloud_port.rs`: a network
//! failure or any 5xx is `Offline` (retry later), 404 is "not in the store"
//! (`Ok(None)`), anything else non-2xx is `Protocol`. Verification of what
//! comes back is the store client's (`lpa_firmware_store::FirmwareStore`).

use lpa_firmware_store::{FetchError, FirmwareFetch, LocalBoxFuture};
use web_sys::{RequestCache, RequestCredentials, RequestRedirect};

/// `fetch` against the firmware store's origin (stateless).
#[derive(Clone, Copy, Debug, Default)]
pub struct WebFirmwareFetch;

impl FirmwareFetch for WebFirmwareFetch {
    fn get(&self, url: &str) -> LocalBoxFuture<'_, Result<Option<Vec<u8>>, FetchError>> {
        let url = url.to_string();
        Box::pin(async move {
            let response = gloo_net::http::Request::get(&url)
                .cache(RequestCache::Default)
                .credentials(RequestCredentials::Omit)
                .redirect(RequestRedirect::Follow)
                .send()
                .await
                .map_err(|error| FetchError::Offline(format!("{url}: {error}")))?;
            let status = response.status();
            if status == 404 {
                return Ok(None);
            }
            if status >= 500 {
                return Err(FetchError::Offline(format!("{url}: HTTP {status}")));
            }
            if !response.ok() {
                return Err(FetchError::Protocol(format!("{url}: HTTP {status}")));
            }
            response
                .binary()
                .await
                .map(Some)
                .map_err(|error| FetchError::Protocol(format!("{url}: unreadable body: {error}")))
        })
    }
}
