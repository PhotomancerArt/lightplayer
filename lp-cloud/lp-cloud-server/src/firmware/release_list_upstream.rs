//! Where the list of releases comes from: the port the release index fetches
//! through.
//!
//! The download host (`releases/download/…`) answers one asset at a time and
//! has no list, so the index needs a second source: GitHub's REST releases
//! list. It is fetched as one page and revalidated with its ETag, so an
//! unchanged list costs a `304` that GitHub does not count against its rate
//! limit.

use std::future::Future;
use std::pin::Pin;

use super::firmware_upstream::UpstreamError;

/// What one list fetch answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseListFetch {
    /// A new body, and the ETag upstream gave it (if any).
    Fresh {
        /// The response body: GitHub's releases JSON.
        bytes: Vec<u8>,
        /// The `ETag` header, exactly as sent (weak tags included).
        etag: Option<String>,
    },
    /// `304`: the list named by the ETag sent is still current.
    NotModified,
}

/// What one list fetch answers: a body, a `304`, or an error.
pub type ReleaseListReply = Result<ReleaseListFetch, UpstreamError>;

/// A boxed, sendable list fetch.
pub type ReleaseListFuture<'a> = Pin<Box<dyn Future<Output = ReleaseListReply> + Send + 'a>>;

/// Fetch the releases list. Production is [`GithubReleaseListUpstream`]; the
/// route tests use an in-process stub that counts its calls.
///
/// [`GithubReleaseListUpstream`]: super::github_release_list_upstream::GithubReleaseListUpstream
pub trait ReleaseListUpstream: Send + Sync {
    /// Fetch the list, conditionally when `etag` is the one last received.
    fn list(&self, etag: Option<String>) -> ReleaseListFuture<'_>;
}
