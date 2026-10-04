//! The fetch port the store client reads through. Studio's is `fetch`
//! (gloo-net); tests use a map.

use std::fmt;

use crate::engine_cache::LocalBoxFuture;

/// Why a fetch has no answer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FetchError {
    /// The network failed, or the server answered 5xx: try again later.
    Offline(String),
    /// The server answered something that is neither bytes nor a 404.
    Protocol(String),
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Offline(why) => write!(f, "the firmware store is unreachable: {why}"),
            Self::Protocol(why) => write!(f, "the firmware store answered oddly: {why}"),
        }
    }
}

impl std::error::Error for FetchError {}

/// GET an absolute URL. Redirects are followed (a `latest` lookup is a
/// 302).
pub trait FirmwareFetch {
    /// `Ok(None)` is a 404. `Err` is offline, a 5xx, or a protocol error.
    fn get(&self, url: &str) -> LocalBoxFuture<'_, Result<Option<Vec<u8>>, FetchError>>;
}
