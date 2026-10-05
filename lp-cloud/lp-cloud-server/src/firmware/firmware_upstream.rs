//! Where released firmware comes from: the port the firmware plane fetches
//! through.

use std::fmt;
use std::future::Future;
use std::pin::Pin;

use lpc_firmware_release::ReleaseVersion;

/// The two upstream URL forms — and the only two. The plane never builds an
/// upstream path from a raw request segment (no SSRF): a version has parsed
/// as a [`ReleaseVersion`] and an asset name is `<target>.<file>` from parsed
/// parts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpstreamAsset {
    /// `{base}/download/v{version}/{asset}`.
    Release {
        /// The release.
        version: ReleaseVersion,
        /// The asset name, `<target>.<file>`.
        asset: String,
    },
    /// `{base}/latest/download/{asset}`: GitHub's "Latest" release, which is
    /// the newest release that carries firmware.
    Latest {
        /// The asset name, `<target>.<file>`.
        asset: String,
    },
}

impl UpstreamAsset {
    /// The URL of this asset under a releases base (no trailing slash).
    pub fn url_under(&self, base: &str) -> String {
        match self {
            Self::Release { version, asset } => {
                format!("{base}/download/{}/{asset}", version.tag())
            }
            Self::Latest { asset } => format!("{base}/latest/download/{asset}"),
        }
    }
}

/// Why upstream did not answer with bytes or a clean 404.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpstreamError {
    /// No answer at all (DNS, connect, TLS, a dropped body).
    Unreachable(String),
    /// No answer in time.
    TimedOut,
    /// The body is larger than the plane accepts.
    TooLarge {
        /// The cap, in bytes.
        limit: usize,
    },
    /// Any status other than success and 404.
    Status(u16),
}

impl fmt::Display for UpstreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreachable(e) => write!(f, "upstream unreachable: {e}"),
            Self::TimedOut => f.write_str("upstream timed out"),
            Self::TooLarge { limit } => write!(f, "upstream body larger than {limit} bytes"),
            Self::Status(code) => write!(f, "upstream answered {code}"),
        }
    }
}

impl std::error::Error for UpstreamError {}

/// What one fetch answers: the bytes, `None` for a 404, or an error.
pub type UpstreamReply = Result<Option<Vec<u8>>, UpstreamError>;

/// A boxed, sendable fetch.
pub type UpstreamFuture<'a> = Pin<Box<dyn Future<Output = UpstreamReply> + Send + 'a>>;

/// Fetch one release asset. Production is [`GithubReleaseUpstream`]; the
/// route tests use an in-process stub that counts its calls.
///
/// [`GithubReleaseUpstream`]: super::github_release_upstream::GithubReleaseUpstream
pub trait FirmwareUpstream: Send + Sync {
    /// Fetch `asset`. `Ok(None)` is upstream's 404.
    fn fetch(&self, asset: UpstreamAsset) -> UpstreamFuture<'_>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_url_forms() {
        let base = "https://github.com/PhotomancerArt/lightplayer/releases";
        let release = UpstreamAsset::Release {
            version: ReleaseVersion::parse("2026.10.05-3").unwrap(),
            asset: "esp32c6-4mb.engine.z".into(),
        };
        assert_eq!(
            release.url_under(base),
            "https://github.com/PhotomancerArt/lightplayer/releases/download/v2026.10.05-3/esp32c6-4mb.engine.z"
        );
        let latest = UpstreamAsset::Latest {
            asset: "esp32c6-4mb.ota-manifest.json".into(),
        };
        assert_eq!(
            latest.url_under(base),
            "https://github.com/PhotomancerArt/lightplayer/releases/latest/download/esp32c6-4mb.ota-manifest.json"
        );
    }
}
