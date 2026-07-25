//! [`HttpSseTransport`]: the platform seam for the Anthropic provider.
//!
//! The provider core is transport-agnostic; platform code (web-sys fetch on
//! wasm, reqwest on host) implements this trait. All futures/streams are
//! runtime-neutral and `!Send` (the wasm implementations cannot be `Send`).

use core::pin::Pin;

use futures_util::{Future, Stream};

/// A boxed `!Send` future (transport results are polled locally).
pub type LocalBoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// Streamed response body chunks.
pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Vec<u8>, TransportError>>>>;

/// A POST request carrying an SSE-producing body.
#[derive(Clone, Debug)]
pub struct HttpRequest {
    pub url: String,
    /// Header name/value pairs (set verbatim).
    pub headers: Vec<(String, String)>,
    /// UTF-8 JSON body.
    pub body: String,
}

/// Response status plus the streamed body. Non-2xx responses still carry
/// their body (the API's JSON error), streamed the same way.
pub struct HttpResponse {
    pub status: u16,
    pub body: ByteStream,
}

/// A network-level failure (DNS, TLS, connection reset, fetch rejection).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportError {
    pub message: String,
}

impl TransportError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// POST + streamed response chunks, plus a platform sleep for the provider's
/// single-retry backoff (transports own everything executor-flavored).
pub trait HttpSseTransport {
    /// Send the request; resolve once response headers are available. The
    /// returned future and body stream are `'static`: implementations clone
    /// what they need. Dropping the body stream cancels the request.
    fn post(
        &self,
        req: HttpRequest,
    ) -> LocalBoxFuture<'static, Result<HttpResponse, TransportError>>;

    /// Sleep for `ms` milliseconds (retry backoff).
    fn sleep_ms(&self, ms: u32) -> LocalBoxFuture<'static, ()>;
}
