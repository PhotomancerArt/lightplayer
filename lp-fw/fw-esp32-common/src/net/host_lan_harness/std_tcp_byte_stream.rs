//! A std TCP socket as the WebSocket server's [`ByteStream`]: the host
//! harness's stand-in for the C6's embassy-net socket.
//!
//! The socket is non-blocking and every future answers `Pending` on
//! `WouldBlock` without registering a waker: the harness drives these
//! futures with its polling `block_on` ([`super::harness_block_on`]), which
//! polls again a moment later. A dropped `read` loses no bytes (nothing is
//! taken from the socket until it can be returned), as the trait requires.

extern crate std;

use core::future::poll_fn;
use core::task::Poll;
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::time::{Duration, Instant};

use crate::net::ws::{ByteStream, StreamClosed};

/// One accepted connection.
pub struct StdTcpByteStream(TcpStream);

impl StdTcpByteStream {
    /// Wrap `stream`, making it non-blocking.
    pub fn new(stream: TcpStream) -> std::io::Result<Self> {
        stream.set_nonblocking(true)?;
        stream.set_nodelay(true)?;
        Ok(Self(stream))
    }
}

impl ByteStream for StdTcpByteStream {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, StreamClosed> {
        poll_fn(|_| match self.0.read(buf) {
            Ok(0) => Poll::Ready(Err(StreamClosed)),
            Ok(n) => Poll::Ready(Ok(n)),
            Err(e) if is_retry(e.kind()) => Poll::Pending,
            Err(_) => Poll::Ready(Err(StreamClosed)),
        })
        .await
    }

    async fn write_all(&mut self, mut buf: &[u8]) -> Result<(), StreamClosed> {
        poll_fn(|_| {
            while !buf.is_empty() {
                match self.0.write(buf) {
                    Ok(0) => return Poll::Ready(Err(StreamClosed)),
                    Ok(n) => buf = &buf[n..],
                    Err(e) if is_retry(e.kind()) => return Poll::Pending,
                    Err(_) => return Poll::Ready(Err(StreamClosed)),
                }
            }
            Poll::Ready(Ok(()))
        })
        .await
    }

    /// Half-close, then read out what the peer still sends (its close
    /// frame's echo) until it closes too, for at most [`LINGER`]: dropping a
    /// socket with unread bytes resets it, and a reset can cost the peer the
    /// close frame it has not read yet.
    async fn close(&mut self) {
        let _ = self.0.flush();
        let _ = self.0.shutdown(Shutdown::Write);
        let until = Instant::now() + LINGER;
        let _ = self.0.set_nonblocking(false);
        let _ = self.0.set_read_timeout(Some(Duration::from_millis(50)));
        let mut sink = [0u8; 256];
        while Instant::now() < until {
            match self.0.read(&mut sink) {
                Ok(0) => break,
                Ok(_) => {}
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(_) => break,
            }
        }
    }
}

/// How long a close waits for the peer to close its half.
const LINGER: Duration = Duration::from_secs(1);

fn is_retry(kind: ErrorKind) -> bool {
    matches!(kind, ErrorKind::WouldBlock | ErrorKind::Interrupted)
}
