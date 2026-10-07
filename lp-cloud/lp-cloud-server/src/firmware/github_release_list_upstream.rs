//! [`ReleaseListUpstream`] over reqwest: GitHub's REST releases list, or
//! whatever `LP_CLOUD_FIRMWARE_RELEASES_LIST` points at.

use std::time::Duration;

use reqwest::header::{ACCEPT, AUTHORIZATION, ETAG, IF_NONE_MATCH};

use super::firmware_upstream::UpstreamError;
use super::github_release_upstream::UPSTREAM_USER_AGENT;
use super::release_list_upstream::{
    ReleaseListFetch, ReleaseListFuture, ReleaseListReply, ReleaseListUpstream,
};
use crate::config::GithubToken;

/// The largest list body accepted. One page of 100 releases measured
/// 663,531 bytes on 2026-10-07 with about twenty of them carrying eleven
/// firmware assets each; a full page of such releases is a few MB.
pub const MAX_RELEASE_LIST_BYTES: usize = 4 * 1024 * 1024;

/// How long one list fetch may take, end to end.
pub const RELEASE_LIST_TIMEOUT: Duration = Duration::from_secs(30);

/// Below this many requests left in GitHub's rate-limit window, each
/// answer is logged at warn: the signal to set `LP_CLOUD_GITHUB_TOKEN`.
pub const RATE_LIMIT_WARN_BELOW: u64 = 10;

/// The list fetched from one URL, with GitHub's JSON media type, its
/// API version, `If-None-Match` when an ETag is held, and the optional
/// token. The token goes to this URL **only**: the download host's
/// [`GithubReleaseUpstream`](super::github_release_upstream::GithubReleaseUpstream)
/// never holds one.
pub struct GithubReleaseListUpstream {
    url: String,
    token: Option<GithubToken>,
    client: reqwest::Client,
    body_limit: usize,
}

impl GithubReleaseListUpstream {
    /// A list upstream at `url` (an `http(s)` URL — the config parser checks
    /// it), sending `token` when there is one.
    pub fn new(url: &str, token: Option<GithubToken>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(RELEASE_LIST_TIMEOUT)
            .user_agent(UPSTREAM_USER_AGENT)
            .build()
            .expect("a reqwest client with rustls builds");
        Self {
            url: url.to_string(),
            token,
            client,
            body_limit: MAX_RELEASE_LIST_BYTES,
        }
    }

    /// The same upstream with a different body cap (tests).
    pub fn with_body_limit(mut self, limit: usize) -> Self {
        self.body_limit = limit;
        self
    }

    async fn fetch(&self, etag: Option<String>) -> ReleaseListReply {
        let mut request = self
            .client
            .get(&self.url)
            .header(ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(token) = &self.token {
            request = request.header(AUTHORIZATION, format!("Bearer {}", token.expose()));
        }
        if let Some(etag) = etag {
            request = request.header(IF_NONE_MATCH, etag);
        }
        let mut response = request.send().await.map_err(classify)?;
        warn_on_low_rate_limit(response.headers());
        let status = response.status();
        if status == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(ReleaseListFetch::NotModified);
        }
        if !status.is_success() {
            return Err(UpstreamError::Status(status.as_u16()));
        }
        let etag = response
            .headers()
            .get(ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let limit = self.body_limit;
        if response
            .content_length()
            .is_some_and(|length| length > limit as u64)
        {
            return Err(UpstreamError::TooLarge { limit });
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(classify)? {
            if bytes.len() + chunk.len() > limit {
                return Err(UpstreamError::TooLarge { limit });
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(ReleaseListFetch::Fresh { bytes, etag })
    }
}

impl ReleaseListUpstream for GithubReleaseListUpstream {
    fn list(&self, etag: Option<String>) -> ReleaseListFuture<'_> {
        Box::pin(self.fetch(etag))
    }
}

fn warn_on_low_rate_limit(headers: &reqwest::header::HeaderMap) {
    let remaining = headers
        .get("x-ratelimit-remaining")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    if let Some(remaining) = remaining
        && remaining < RATE_LIMIT_WARN_BELOW
    {
        log::warn!(
            "firmware: the GitHub releases list has {remaining} requests left in its rate-limit window (set LP_CLOUD_GITHUB_TOKEN to raise it)"
        );
    }
}

fn classify(error: reqwest::Error) -> UpstreamError {
    if error.is_timeout() {
        UpstreamError::TimedOut
    } else {
        UpstreamError::Unreachable(error.to_string())
    }
}
