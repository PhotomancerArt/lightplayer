//! The byte stream the WebSocket server runs over.

use core::future::Future;

/// The peer is gone, or the stream failed: nothing more will be read or
/// written on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamClosed;

/// A reliable, ordered, full-duplex byte stream to one peer — what
/// [`super::WsConnection`] runs over. Today that is an embassy-net TCP
/// socket; a TLS session can implement it later and nothing in `ws/`
/// changes (Wi-Fi plan D3/MD12). No other file in `ws/` knows about TCP.
///
/// Every method is a runtime-neutral future (the sans-IO ADR's rule for
/// `async` at a seam): it may wait on the stream's own readiness, never on
/// a particular executor. The contract an implementation is held to:
///
/// - [`Self::read`] waits until at least one byte is available, copies up
///   to `buf.len()` bytes into `buf` and returns how many. `Ok(0)` (for a
///   non-empty `buf`) or `Err` means the peer is gone for good. Dropping
///   the future before it resolves must lose no bytes (embassy-net's
///   `TcpSocket::read` behaves so), because a caller may race a receive
///   against outbound traffic.
/// - [`Self::write_all`] resolves once every byte of `buf` has been
///   accepted by the stream (queued for sending is enough), or `Err` if it
///   cannot be. A dropped `write_all` may have sent a prefix.
/// - [`Self::close`] ends the stream gracefully: bytes already accepted are
///   still delivered, then the peer sees end-of-stream. It never fails;
///   after it the stream is not used again.
///
/// Timeouts are the implementation's business (embassy-net's socket
/// timeout, say): the WebSocket layer never reads a clock.
pub trait ByteStream {
    /// Read at least one byte into `buf`; `Ok(0)` or `Err` = the peer is gone.
    fn read(&mut self, buf: &mut [u8]) -> impl Future<Output = Result<usize, StreamClosed>>;

    /// Hand every byte of `buf` to the stream.
    fn write_all(&mut self, buf: &[u8]) -> impl Future<Output = Result<(), StreamClosed>>;

    /// Deliver what was written, then end the stream.
    fn close(&mut self) -> impl Future<Output = ()>;
}
