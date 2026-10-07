//! An embassy-net TCP socket as the WebSocket's byte stream: what the LAN
//! endpoint accepts on and the relay's device leg dials out on.

use embassy_net::tcp::TcpSocket;
use fw_esp32_common::net::ws::{ByteStream, StreamClosed};

/// A TCP socket as the WebSocket's byte stream.
pub struct TcpStream<'a>(pub TcpSocket<'a>);

impl ByteStream for TcpStream<'_> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, StreamClosed> {
        self.0.read(buf).await.map_err(|_| StreamClosed)
    }

    async fn write_all(&mut self, mut buf: &[u8]) -> Result<(), StreamClosed> {
        while !buf.is_empty() {
            match self.0.write(buf).await {
                Ok(0) | Err(_) => return Err(StreamClosed),
                Ok(n) => buf = &buf[n..],
            }
        }
        Ok(())
    }

    async fn close(&mut self) {
        let _ = self.0.flush().await;
        self.0.close();
        let _ = self.0.flush().await;
    }
}
