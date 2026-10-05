//! [`FirmwareUpstream`] over reqwest: GitHub releases, or whatever
//! `LP_CLOUD_FIRMWARE_UPSTREAM` points at.

use std::time::Duration;

use super::firmware_upstream::{
    FirmwareUpstream, UpstreamAsset, UpstreamError, UpstreamFuture, UpstreamReply,
};

/// The largest file the plane will fetch. A firmware piece is ~2 MB and a
/// merged image 4 MB; anything past this is not a firmware file.
pub const MAX_FIRMWARE_FILE_BYTES: usize = 16 * 1024 * 1024;

/// How long one upstream fetch may take, end to end.
pub const UPSTREAM_TIMEOUT: Duration = Duration::from_secs(30);

/// What the proxy calls itself upstream.
pub const UPSTREAM_USER_AGENT: &str = "lightplayer.app firmware proxy";

/// Release assets fetched from `{base}/download/…` and
/// `{base}/latest/download/…`, following GitHub's redirects to its blob host
/// (reqwest's default policy), with a timeout and a body cap.
pub struct GithubReleaseUpstream {
    base: String,
    client: reqwest::Client,
    body_limit: usize,
}

impl GithubReleaseUpstream {
    /// An upstream under `base` (an `http(s)` URL, no trailing slash — the
    /// config parser normalizes it).
    pub fn new(base: &str) -> Self {
        let client = reqwest::Client::builder()
            .timeout(UPSTREAM_TIMEOUT)
            .user_agent(UPSTREAM_USER_AGENT)
            .build()
            .expect("a reqwest client with rustls builds");
        Self {
            base: base.trim_end_matches('/').to_string(),
            client,
            body_limit: MAX_FIRMWARE_FILE_BYTES,
        }
    }

    /// The same upstream with a different body cap (tests).
    pub fn with_body_limit(mut self, limit: usize) -> Self {
        self.body_limit = limit;
        self
    }

    /// The URL an asset is fetched from.
    pub fn url(&self, asset: &UpstreamAsset) -> String {
        asset.url_under(&self.base)
    }

    async fn fetch_url(&self, url: String) -> UpstreamReply {
        let mut response = self.client.get(&url).send().await.map_err(classify)?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !status.is_success() {
            return Err(UpstreamError::Status(status.as_u16()));
        }
        let limit = self.body_limit;
        if response
            .content_length()
            .is_some_and(|length| length > limit as u64)
        {
            return Err(UpstreamError::TooLarge { limit });
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(classify)? {
            if body.len() + chunk.len() > limit {
                return Err(UpstreamError::TooLarge { limit });
            }
            body.extend_from_slice(&chunk);
        }
        Ok(Some(body))
    }
}

impl FirmwareUpstream for GithubReleaseUpstream {
    fn fetch(&self, asset: UpstreamAsset) -> UpstreamFuture<'_> {
        let url = self.url(&asset);
        Box::pin(self.fetch_url(url))
    }
}

fn classify(error: reqwest::Error) -> UpstreamError {
    if error.is_timeout() {
        UpstreamError::TimedOut
    } else {
        UpstreamError::Unreachable(error.to_string())
    }
}
