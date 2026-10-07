//! One WebSocket connection, driven over a [`ByteStream`]: the server end
//! of the board's `/link` route ([`WsConnection::accept`]), or the client
//! end of the relay's device leg ([`WsConnection::connect`]).
//!
//! The receive buffer is borrowed, fixed-size and caller-owned (the
//! firmware pools buffers at bring-up and allocates nothing per
//! connection). Messages are reassembled **in place** in it, so a received
//! message is a borrow of that buffer — no second copy.
//!
//! The two ends differ only where RFC 6455 makes them: a client masks every
//! frame it sends with a fresh key from its entropy and refuses a masked
//! frame from the server; a server does the opposite. Everything else —
//! pings answered, the close handshake, fragments — is the same code.

use super::byte_stream::{ByteStream, StreamClosed};
use super::ws_client_handshake::{ClientHandshake, ClientKey, client_request, parse_response};
use super::ws_frame::{
    CloseCode, Decoded, FrameDecoder, MAX_CLIENT_HEADER, MAX_CONTROL_PAYLOAD, Opcode, apply_mask,
    client_header, server_header,
};
use super::ws_handshake::{Handshake, MAX_REQUEST, Refusal, parse_request, upgrade_response};

/// Receive-buffer bytes beyond the message cap: room for a whole control
/// frame (a ping arriving between the fragments of a message already at
/// the cap) — which also covers the longest data frame header.
pub const RX_OVERHEAD: usize = {
    let control = 2 + 4 + MAX_CONTROL_PAYLOAD;
    if control > MAX_CLIENT_HEADER {
        control
    } else {
        MAX_CLIENT_HEADER
    }
};

/// Why a connection ended. Whichever it was, the stream has been closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WsClosed {
    /// The peer sent a close frame with this code ([`CloseCode::NO_STATUS`]
    /// if it carried none); it was echoed.
    Peer(CloseCode),
    /// The peer broke the protocol; a close frame with this code was sent.
    Protocol(CloseCode),
    /// The stream ended or failed, or a write on it was abandoned midway
    /// (a dropped `recv`/`send` future), leaving the framing unknowable.
    Stream,
}

/// Why [`WsConnection::accept`] did not upgrade. The stream has been closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceptError {
    /// The request was answered with this refusal.
    Refused(Refusal),
    /// The stream ended or failed before the handshake completed.
    Stream,
}

/// Why [`WsConnection::connect`] did not upgrade. The stream has been
/// closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectError {
    /// The server answered this status (0: not HTTP), or a `101` without
    /// the upgrade headers and accept value the key implies.
    Refused { status: u16 },
    /// The request did not fit the receive buffer.
    RequestTooLong,
    /// The stream ended or failed before the handshake completed.
    Stream,
}

/// What [`WsConnection::recv_event`] heard.
#[derive(Debug, PartialEq, Eq)]
pub enum WsEvent<'m> {
    /// A whole binary message, borrowing the receive buffer.
    Message(&'m [u8]),
    /// A ping (answered) or a pong: the peer is alive, nothing to deliver.
    Control,
}

/// A server-side WebSocket connection carrying binary messages.
///
/// The largest message it takes is `rx.len() - RX_OVERHEAD`; give it a
/// buffer of `max_message + RX_OVERHEAD` bytes (and at least
/// [`MAX_REQUEST`] if requests that long should be read).
///
/// Cancellation: [`Self::recv`] keeps its framing state in the connection,
/// so it may be dropped while it waits for bytes (racing it against
/// outbound traffic) and called again. It also writes — a pong, a close
/// echo — and [`Self::send`] writes; dropping either mid-write would leave
/// a partial frame on the wire, so the connection then refuses further use
/// ([`WsClosed::Stream`]) instead of corrupting the stream.
pub struct WsConnection<'a, S: ByteStream> {
    stream: S,
    rx: &'a mut [u8],
    /// Valid bytes in `rx`: the message so far, then undecoded input.
    filled: usize,
    /// Reassembled payload of the open message, at `rx[..message_len]`.
    message_len: usize,
    /// Length of the message last returned by `recv`, dropped on the next.
    returned: usize,
    decoder: FrameDecoder,
    /// Set across every write; still set on entry = a write was abandoned.
    writing: bool,
    closed: Option<WsClosed>,
    /// A client's entropy, for each frame's masking key; `None` on a server.
    mask: Option<fn(&mut [u8])>,
}

/// What the receive loop stopped on: a message (its length, at the start
/// of the buffer), or a control frame the caller asked to hear about.
enum Next {
    Message(usize),
    Control,
}

impl<'a, S: ByteStream> WsConnection<'a, S> {
    /// Read the upgrade request from `stream` and answer it: `101` and a
    /// connection, or the refusal (then close). Bytes the client sent after
    /// its request are kept as the first frames.
    pub async fn accept(mut stream: S, rx: &'a mut [u8]) -> Result<Self, AcceptError> {
        let request_cap = rx.len().min(MAX_REQUEST);
        let mut filled = 0;
        let refusal = loop {
            match parse_request(&rx[..filled]) {
                Handshake::Incomplete if filled < request_cap => {
                    match stream.read(&mut rx[filled..request_cap]).await {
                        Ok(n) if n > 0 => filled += n,
                        _ => {
                            stream.close().await;
                            return Err(AcceptError::Stream);
                        }
                    }
                }
                Handshake::Incomplete => break Refusal::TooLarge,
                Handshake::Refuse { refusal, .. } => break refusal,
                Handshake::Upgrade { key, consumed } => {
                    let response = upgrade_response(key);
                    if stream.write_all(&response).await.is_err() {
                        stream.close().await;
                        return Err(AcceptError::Stream);
                    }
                    rx.copy_within(consumed..filled, 0);
                    let max_message = rx.len().saturating_sub(RX_OVERHEAD);
                    return Ok(Self {
                        stream,
                        rx,
                        filled: filled - consumed,
                        message_len: 0,
                        returned: 0,
                        decoder: FrameDecoder::new(max_message),
                        writing: false,
                        closed: None,
                        mask: None,
                    });
                }
            }
        };
        let _ = stream.write_all(refusal.response()).await;
        stream.close().await;
        Err(AcceptError::Refused(refusal))
    }

    /// Open a client connection: write the upgrade request for `path` with
    /// `host` as its `Host` (`host:port` off port 80), read the `101`, and
    /// check its accept value. `entropy` makes the key, and every frame's
    /// masking key after it. The request is written from `rx` (so it must
    /// fit there); bytes the server sent after its answer are kept as the
    /// first frames.
    pub async fn connect(
        mut stream: S,
        rx: &'a mut [u8],
        host: &str,
        path: &str,
        entropy: fn(&mut [u8]),
    ) -> Result<Self, ConnectError> {
        let key = ClientKey::fresh(entropy);
        let Ok(len) = client_request(rx, host, path, &key) else {
            stream.close().await;
            return Err(ConnectError::RequestTooLong);
        };
        if stream.write_all(&rx[..len]).await.is_err() {
            stream.close().await;
            return Err(ConnectError::Stream);
        }
        let response_cap = rx.len().min(MAX_REQUEST);
        let mut filled = 0;
        loop {
            match parse_response(&rx[..filled], &key) {
                ClientHandshake::Incomplete if filled < response_cap => {
                    match stream.read(&mut rx[filled..response_cap]).await {
                        Ok(n) if n > 0 => filled += n,
                        _ => {
                            stream.close().await;
                            return Err(ConnectError::Stream);
                        }
                    }
                }
                ClientHandshake::Incomplete => {
                    stream.close().await;
                    return Err(ConnectError::Refused { status: 0 });
                }
                ClientHandshake::Refused { status } => {
                    stream.close().await;
                    return Err(ConnectError::Refused { status });
                }
                ClientHandshake::Upgraded { consumed } => {
                    rx.copy_within(consumed..filled, 0);
                    let max_message = rx.len().saturating_sub(RX_OVERHEAD);
                    return Ok(Self {
                        stream,
                        rx,
                        filled: filled - consumed,
                        message_len: 0,
                        returned: 0,
                        decoder: FrameDecoder::for_client(max_message),
                        writing: false,
                        closed: None,
                        mask: Some(entropy),
                    });
                }
            }
        }
    }

    /// The largest message this connection receives.
    pub fn max_message(&self) -> usize {
        self.decoder.max_message()
    }

    /// The next whole binary message, reassembled from its fragments. Pings
    /// are answered and pongs dropped on the way; a close is echoed and
    /// ends the connection. The message borrows the receive buffer until
    /// the next call.
    pub async fn recv(&mut self) -> Result<&[u8], WsClosed> {
        match self.next(false).await? {
            Next::Message(len) => Ok(&self.rx[..len]),
            Next::Control => unreachable!("controls are not surfaced"),
        }
    }

    /// As [`Self::recv`], but a ping (answered) or a pong is reported as
    /// [`WsEvent::Control`] too: the relay's device leg counts them as the
    /// hub being alive.
    pub async fn recv_event(&mut self) -> Result<WsEvent<'_>, WsClosed> {
        match self.next(true).await? {
            Next::Message(len) => Ok(WsEvent::Message(&self.rx[..len])),
            Next::Control => Ok(WsEvent::Control),
        }
    }

    /// The receive loop behind [`Self::recv`] and [`Self::recv_event`].
    async fn next(&mut self, surface_control: bool) -> Result<Next, WsClosed> {
        self.ensure_open().await?;
        if self.returned > 0 {
            self.drop_range(0, self.returned);
            self.returned = 0;
        }
        loop {
            let base = self.message_len;
            let frame = match self.decoder.decode(&mut self.rx[base..self.filled]) {
                Decoded::Frame(frame) => frame,
                Decoded::Error(code) => return Err(self.fail(code).await),
                Decoded::Incomplete => {
                    // Unreachable with `rx.len() >= RX_OVERHEAD`: the
                    // decoder refuses any frame that would not fit.
                    if self.filled == self.rx.len() {
                        return Err(self.fail(CloseCode::MESSAGE_TOO_BIG).await);
                    }
                    match self.stream.read(&mut self.rx[self.filled..]).await {
                        Ok(n) if n > 0 => self.filled += n,
                        _ => return Err(self.end(WsClosed::Stream).await),
                    }
                    continue;
                }
            };
            let payload = base + frame.payload.start..base + frame.payload.end;
            let frame_end = base + frame.frame_len;
            match frame.opcode {
                Opcode::Binary | Opcode::Continuation => {
                    let len = payload.len();
                    self.rx.copy_within(payload, base);
                    self.message_len += len;
                    self.drop_range(self.message_len, frame_end);
                    if frame.fin {
                        let len = self.message_len;
                        self.message_len = 0;
                        self.returned = len;
                        return Ok(Next::Message(len));
                    }
                }
                Opcode::Ping => {
                    let pong = &self.rx[payload];
                    if write_frame(
                        &mut self.stream,
                        &mut self.writing,
                        self.mask,
                        Opcode::Pong,
                        pong,
                    )
                    .await
                    .is_err()
                    {
                        return Err(self.end(WsClosed::Stream).await);
                    }
                    self.drop_range(base, frame_end);
                    if surface_control {
                        return Ok(Next::Control);
                    }
                }
                Opcode::Pong => {
                    self.drop_range(base, frame_end);
                    if surface_control {
                        return Ok(Next::Control);
                    }
                }
                Opcode::Close => {
                    let code = match payload.len() {
                        0 => CloseCode::NO_STATUS,
                        1 => return Err(self.fail(CloseCode::PROTOCOL_ERROR).await),
                        _ => CloseCode(u16::from_be_bytes([
                            self.rx[payload.start],
                            self.rx[payload.start + 1],
                        ])),
                    };
                    let _ = self.write_close(code).await;
                    return Err(self.end(WsClosed::Peer(code)).await);
                }
            }
        }
    }

    /// Send `payload` as one binary frame.
    pub async fn send(&mut self, payload: &[u8]) -> Result<(), WsClosed> {
        self.ensure_open().await?;
        if write_frame(
            &mut self.stream,
            &mut self.writing,
            self.mask,
            Opcode::Binary,
            payload,
        )
        .await
        .is_err()
        {
            return Err(self.end(WsClosed::Stream).await);
        }
        Ok(())
    }

    /// Close with `code`, then end the stream. The peer's answering close is
    /// not awaited: there is nothing left to tell it.
    pub async fn close(mut self, code: CloseCode) {
        if self.closed.is_none() {
            if !self.writing {
                let _ = self.write_close(code).await;
            }
            self.stream.close().await;
        }
    }

    /// `Err` once the connection has ended, or if a write was abandoned.
    async fn ensure_open(&mut self) -> Result<(), WsClosed> {
        if self.closed.is_none() && self.writing {
            self.end(WsClosed::Stream).await;
        }
        self.closed.map_or(Ok(()), Err)
    }

    /// Send a close frame for `code`, then end with [`WsClosed::Protocol`].
    async fn fail(&mut self, code: CloseCode) -> WsClosed {
        let _ = self.write_close(code).await;
        self.end(WsClosed::Protocol(code)).await
    }

    /// Close the stream and remember why.
    async fn end(&mut self, why: WsClosed) -> WsClosed {
        self.stream.close().await;
        self.closed = Some(why);
        why
    }

    async fn write_close(&mut self, code: CloseCode) -> Result<(), StreamClosed> {
        let bytes = code.0.to_be_bytes();
        let payload: &[u8] = if code == CloseCode::NO_STATUS {
            &[]
        } else {
            &bytes
        };
        write_frame(
            &mut self.stream,
            &mut self.writing,
            self.mask,
            Opcode::Close,
            payload,
        )
        .await
    }

    /// Remove `rx[from..to]`, shifting the input after it down.
    fn drop_range(&mut self, from: usize, to: usize) {
        self.rx.copy_within(to..self.filled, from);
        self.filled -= to - from;
    }
}

/// Bytes a client masks at a time on its way out (a stack copy: the
/// caller's payload is borrowed, and masking is in place).
const MASK_CHUNK: usize = 64;

/// One final frame — a server's, unmasked, or with `mask` (a client's
/// entropy) masked under a fresh key; `writing` stays set if it does not
/// complete.
async fn write_frame<S: ByteStream>(
    stream: &mut S,
    writing: &mut bool,
    mask: Option<fn(&mut [u8])>,
    opcode: Opcode,
    payload: &[u8],
) -> Result<(), StreamClosed> {
    *writing = true;
    match mask {
        None => {
            stream
                .write_all(server_header(opcode, payload.len()).as_bytes())
                .await?;
            stream.write_all(payload).await?;
        }
        Some(entropy) => {
            let mut key = [0u8; 4];
            entropy(&mut key);
            stream
                .write_all(client_header(opcode, payload.len(), key).as_bytes())
                .await?;
            let mut chunk = [0u8; MASK_CHUNK];
            for (index, part) in payload.chunks(MASK_CHUNK).enumerate() {
                let out = &mut chunk[..part.len()];
                out.copy_from_slice(part);
                apply_mask(out, key, index * MASK_CHUNK);
                stream.write_all(out).await?;
            }
        }
    }
    *writing = false;
    Ok(())
}

#[cfg(test)]
mod tests {
    extern crate std;

    use core::pin::pin;
    use core::task::{Context, Poll, Waker};
    use std::collections::VecDeque;
    use std::io::{Read, Write};
    use std::net::{Shutdown, TcpListener, TcpStream};
    use std::string::String;
    use std::thread::{self, JoinHandle};
    use std::time::Duration;
    use std::vec::Vec;
    use std::{format, vec};

    use tungstenite::Message;
    use tungstenite::protocol::frame::Frame as TFrame;
    use tungstenite::protocol::frame::coding::{Data, OpCode};

    use super::*;

    const KEY: &str = "dGhlIHNhbXBsZSBub25jZQ==";

    #[test]
    fn real_client_round_trips_every_length_form() {
        let (port, server) = spawn_echo_server(65536 + RX_OVERHEAD);
        let mut ws = connect(port, "/link").unwrap();
        for len in [0usize, 1, 125, 126, 1000, 65535, 65536] {
            let payload = pattern(len);
            ws.send(Message::binary(payload.clone())).unwrap();
            assert_eq!(ws.read().unwrap(), Message::binary(payload), "length {len}");
        }
        ws.send(Message::Ping(b"are you there".to_vec().into()))
            .unwrap();
        assert_eq!(
            ws.read().unwrap(),
            Message::Pong(b"are you there".to_vec().into())
        );
        ws.send(Message::Pong(b"unsolicited".to_vec().into()))
            .unwrap();
        ws.send(Message::binary(vec![7u8; 3])).unwrap();
        assert_eq!(ws.read().unwrap(), Message::binary(vec![7u8; 3]));

        ws.close(None).unwrap();
        let (codes, last) = read_to_end(&mut ws);
        assert_eq!(codes, vec![CloseCode::NO_STATUS.0]);
        assert!(
            matches!(last, tungstenite::Error::ConnectionClosed),
            "{last:?}"
        );
        drop(ws);
        assert_eq!(
            server.join().unwrap(),
            Ok(WsClosed::Peer(CloseCode::NO_STATUS))
        );
    }

    /// The client half against a real server (tungstenite): every length
    /// form both ways, masked out and unmasked in; a server's ping is
    /// answered and heard; the close handshake ends it.
    #[test]
    fn the_client_round_trips_every_length_form_with_a_real_server() {
        let (port, server) = spawn_tungstenite_echo("/relay/device");
        let socket = TcpStream::connect(("127.0.0.1", port)).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut rx = vec![0u8; 65536 + RX_OVERHEAD];
        let host = format!("127.0.0.1:{port}");
        block_on(async {
            let mut ws = WsConnection::connect(
                TcpByteStream(socket),
                &mut rx,
                &host,
                "/relay/device",
                test_entropy,
            )
            .await
            .expect("the upgrade");
            for len in [0usize, 1, 125, 126, 1000, 65535, 65536] {
                let payload = pattern(len);
                ws.send(&payload).await.unwrap();
                assert_eq!(ws.recv().await.unwrap(), payload.as_slice(), "length {len}");
            }
            // The echo server pings after the word "ping".
            ws.send(b"ping").await.unwrap();
            assert_eq!(ws.recv_event().await.unwrap(), WsEvent::Control);
            assert_eq!(
                ws.recv_event().await.unwrap(),
                WsEvent::Message(b"ping".as_slice())
            );
            ws.close(CloseCode::NORMAL).await;
        });
        let (host_seen, pongs, close) = server.join().unwrap();
        assert_eq!(host_seen, host, "the Host header names host:port");
        assert_eq!(pongs, 1, "the ping was answered");
        assert_eq!(close, Some(1000));
    }

    #[test]
    fn the_client_and_the_server_halves_talk_to_each_other() {
        let (port, server) = spawn_echo_server(2048 + RX_OVERHEAD);
        let socket = TcpStream::connect(("127.0.0.1", port)).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut rx = vec![0u8; 2048 + RX_OVERHEAD];
        block_on(async {
            let mut ws = WsConnection::connect(
                TcpByteStream(socket),
                &mut rx,
                "board",
                "/link",
                test_entropy,
            )
            .await
            .expect("the board's server upgrades the board's client");
            for len in [0usize, 125, 126, 2048] {
                ws.send(&pattern(len)).await.unwrap();
                assert_eq!(ws.recv().await.unwrap(), pattern(len).as_slice());
            }
            ws.close(CloseCode::NORMAL).await;
        });
        assert_eq!(
            server.join().unwrap(),
            Ok(WsClosed::Peer(CloseCode::NORMAL))
        );
    }

    #[test]
    fn a_refused_upgrade_is_its_status() {
        let (port, server) = spawn_echo_server(1024 + RX_OVERHEAD);
        let socket = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let mut rx = vec![0u8; 1024 + RX_OVERHEAD];
        let result = block_on(WsConnection::connect(
            TcpByteStream(socket),
            &mut rx,
            "board",
            "/relay/device",
            test_entropy,
        ));
        assert_eq!(
            result.map(|_| ()),
            Err(ConnectError::Refused { status: 404 }),
            "the board's server serves /link only"
        );
        let _ = server.join();
    }

    #[test]
    fn a_masked_server_frame_is_refused_by_the_client() {
        let key = ClientKey::fresh(test_entropy);
        let mut response = std::format!(
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
             Sec-WebSocket-Accept: {}\r\n\r\n",
            core::str::from_utf8(&key.expected_accept()).unwrap()
        )
        .into_bytes();
        response.extend(client_frame(0x82, b"masked"));
        let mut stream = MemStream {
            input: vec![response].into(),
            output: Vec::new(),
        };
        let mut rx = vec![0u8; 1024];
        let result = block_on(async {
            let mut conn =
                WsConnection::connect(&mut stream, &mut rx, "hub", "/relay/device", test_entropy)
                    .await
                    .expect("the upgrade");
            conn.recv().await.map(|_| ()).unwrap_err()
        });
        assert_eq!(result, WsClosed::Protocol(CloseCode::PROTOCOL_ERROR));
        // The client's own close frame is masked: mask bit set, 4-byte key.
        let close = &stream.output[stream.output.len() - 8..];
        assert_eq!(&close[..2], &[0x88, 0x82]);
    }

    #[test]
    fn real_client_close_with_code_is_echoed() {
        let (port, server) = spawn_echo_server(1024 + RX_OVERHEAD);
        let mut ws = connect(port, "/link").unwrap();
        ws.close(Some(tungstenite::protocol::CloseFrame {
            code: 1000.into(),
            reason: "bye".into(),
        }))
        .unwrap();
        let (codes, last) = read_to_end(&mut ws);
        assert_eq!(codes, vec![1000]);
        assert!(
            matches!(last, tungstenite::Error::ConnectionClosed),
            "{last:?}"
        );
        drop(ws);
        assert_eq!(
            server.join().unwrap(),
            Ok(WsClosed::Peer(CloseCode::NORMAL))
        );
    }

    #[test]
    fn real_client_fragments_are_reassembled_around_a_ping() {
        let (port, server) = spawn_echo_server(1024 + RX_OVERHEAD);
        let mut ws = connect(port, "/link").unwrap();
        let whole = pattern(700);
        let frames = [
            TFrame::message(whole[..300].to_vec(), OpCode::Data(Data::Binary), false),
            TFrame::ping(b"mid".to_vec()),
            TFrame::message(
                whole[300..301].to_vec(),
                OpCode::Data(Data::Continue),
                false,
            ),
            TFrame::message(whole[301..].to_vec(), OpCode::Data(Data::Continue), true),
        ];
        for frame in frames {
            ws.send(Message::Frame(frame)).unwrap();
        }
        assert_eq!(ws.read().unwrap(), Message::Pong(b"mid".to_vec().into()));
        assert_eq!(ws.read().unwrap(), Message::binary(whole));
        drop(ws);
        assert_eq!(server.join().unwrap(), Ok(WsClosed::Stream));
    }

    #[test]
    fn real_client_text_is_refused_with_1003() {
        let (port, server) = spawn_echo_server(1024 + RX_OVERHEAD);
        let mut ws = connect(port, "/link").unwrap();
        ws.send(Message::text("hello")).unwrap();
        assert_eq!(read_to_end(&mut ws).0, vec![1003]);
        drop(ws);
        assert_eq!(
            server.join().unwrap(),
            Ok(WsClosed::Protocol(CloseCode::UNSUPPORTED_DATA))
        );
    }

    #[test]
    fn real_client_oversize_message_is_refused_with_1009() {
        let (port, server) = spawn_echo_server(1024 + RX_OVERHEAD);
        let mut ws = connect(port, "/link").unwrap();
        ws.send(Message::binary(pattern(1024))).unwrap();
        assert_eq!(
            ws.read().unwrap(),
            Message::binary(pattern(1024)),
            "the cap itself fits"
        );
        ws.send(Message::binary(pattern(1025))).unwrap();
        assert_eq!(read_to_end(&mut ws).0, vec![1009]);
        drop(ws);
        assert_eq!(
            server.join().unwrap(),
            Ok(WsClosed::Protocol(CloseCode::MESSAGE_TOO_BIG))
        );
    }

    #[test]
    fn real_client_wrong_path_gets_404() {
        let (port, server) = spawn_echo_server(1024 + RX_OVERHEAD);
        match connect(port, "/other") {
            Err(tungstenite::Error::Http(response)) => assert_eq!(response.status(), 404),
            other => panic!("{:?}", other.map(|_| ())),
        }
        assert_eq!(
            server.join().unwrap(),
            Err(AcceptError::Refused(Refusal::NotFound))
        );
    }

    #[test]
    fn unmasked_client_frame_is_refused_with_1002() {
        let mut input = vec![request(&valid_headers()).into_bytes()];
        input.push(vec![0x82, 0x02, 1, 2]);
        let (result, output) = run(input, 1024, async |conn| {
            let mut conn = conn.unwrap();
            conn.recv().await.map(|_| ()).unwrap_err()
        });
        assert_eq!(result, WsClosed::Protocol(CloseCode::PROTOCOL_ERROR));
        assert!(output.ends_with(&[0x88, 0x02, 0x03, 0xEA]), "{output:?}");
    }

    #[test]
    fn missing_key_is_400_and_version_8_is_426() {
        let mut no_key = valid_headers();
        no_key.retain(|h| !h.starts_with("Sec-WebSocket-Key"));
        let mut v8 = valid_headers();
        v8.retain(|h| !h.starts_with("Sec-WebSocket-Version"));
        v8.push("Sec-WebSocket-Version: 8");
        for (headers, refusal) in [
            (no_key, Refusal::BadRequest),
            (v8, Refusal::UpgradeRequired),
        ] {
            let (result, output) = run(vec![request(&headers).into_bytes()], 1024, async |conn| {
                conn.map(|_| WsClosed::Stream)
            });
            assert_eq!(result, Err(AcceptError::Refused(refusal)));
            assert_eq!(output, refusal.response());
        }
    }

    #[test]
    fn request_in_tiny_reads_with_frames_pipelined_behind_it() {
        let mut wire = request(&valid_headers()).into_bytes();
        wire.extend(client_frame(0x82, b"first"));
        wire.extend(client_frame(0x89, b"p"));
        wire.extend(client_frame(0x82, b"second"));
        let input = wire.iter().map(|b| vec![*b]).collect();
        let (result, output) = run(input, 1024, async |conn| {
            let mut conn = conn.unwrap();
            assert_eq!(conn.recv().await.unwrap(), b"first");
            assert_eq!(conn.recv().await.unwrap(), b"second");
            conn.recv().await.unwrap_err()
        });
        assert_eq!(result, WsClosed::Stream);
        let mut expected = upgrade_response(KEY).to_vec();
        expected.extend_from_slice(&[0x8A, 1, b'p']);
        assert_eq!(output, expected);
    }

    #[test]
    fn messages_arriving_in_one_read_come_out_one_by_one() {
        let mut burst = Vec::new();
        for i in 0..5u8 {
            burst.extend(client_frame(0x82, &vec![i; usize::from(i) * 10]));
        }
        let input = vec![request(&valid_headers()).into_bytes(), burst];
        let (result, _) = run(input, 1024, async |conn| {
            let mut conn = conn.unwrap();
            for i in 0..5u8 {
                assert_eq!(
                    conn.recv().await.unwrap(),
                    vec![i; usize::from(i) * 10].as_slice()
                );
            }
            conn.recv().await.unwrap_err()
        });
        assert_eq!(result, WsClosed::Stream);
    }

    #[test]
    fn a_message_at_the_cap_fits_with_a_full_ping_between_fragments() {
        const CAP: usize = 64;
        let whole = pattern(CAP);
        let mut burst = request(&valid_headers()).into_bytes();
        burst.extend(client_frame(0x02, &whole[..CAP - 1]));
        burst.extend(client_frame(0x89, &[9; MAX_CONTROL_PAYLOAD]));
        burst.extend(client_frame(0x80, &whole[CAP - 1..]));
        // Everything at once, so the buffer fills to its last byte.
        let (result, output) = run(vec![burst], CAP + RX_OVERHEAD, async |conn| {
            let mut conn = conn.unwrap();
            assert_eq!(conn.max_message(), CAP);
            assert_eq!(conn.recv().await.unwrap(), whole.as_slice());
            conn.recv().await.unwrap_err()
        });
        assert_eq!(result, WsClosed::Stream);
        let pong_at = UPGRADE_LEN;
        assert_eq!(
            &output[pong_at..pong_at + 2],
            &[0x8A, MAX_CONTROL_PAYLOAD as u8]
        );
    }

    #[test]
    fn send_writes_one_unmasked_binary_frame_and_close_ends_it() {
        let input = vec![request(&valid_headers()).into_bytes()];
        let ((), output) = run(input, 1024, async |conn| {
            let mut conn = conn.unwrap();
            conn.send(&[1, 2, 3]).await.unwrap();
            conn.close(CloseCode::NORMAL).await;
        });
        assert_eq!(
            &output[UPGRADE_LEN..],
            &[0x82, 3, 1, 2, 3, 0x88, 2, 0x03, 0xE8]
        );
    }

    // --- helpers ---

    const UPGRADE_LEN: usize = super::super::ws_handshake::UPGRADE_RESPONSE_LEN;

    type Client = tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>;

    /// Drive an immediately-progressing future (tests only: a null-waker
    /// loop over blocking std IO).
    fn block_on<F: Future>(future: F) -> F::Output {
        let mut future = pin!(future);
        let mut cx = Context::from_waker(Waker::noop());
        loop {
            if let Poll::Ready(out) = future.as_mut().poll(&mut cx) {
                return out;
            }
        }
    }

    /// A [`ByteStream`] over a blocking std socket.
    struct TcpByteStream(TcpStream);

    impl ByteStream for TcpByteStream {
        async fn read(&mut self, buf: &mut [u8]) -> Result<usize, StreamClosed> {
            self.0.read(buf).map_err(|_| StreamClosed)
        }

        async fn write_all(&mut self, buf: &[u8]) -> Result<(), StreamClosed> {
            Write::write_all(&mut self.0, buf).map_err(|_| StreamClosed)
        }

        async fn close(&mut self) {
            // Graceful: FIN, then drain what the peer still sends, so unread
            // input cannot turn the close into a reset that eats our close
            // frame.
            let _ = self.0.shutdown(Shutdown::Write);
            let mut sink = [0u8; 4096];
            while matches!(self.0.read(&mut sink), Ok(n) if n > 0) {}
        }
    }

    /// A scripted [`ByteStream`]: each read returns (up to) the next chunk.
    struct MemStream {
        input: VecDeque<Vec<u8>>,
        output: Vec<u8>,
    }

    impl ByteStream for &mut MemStream {
        async fn read(&mut self, buf: &mut [u8]) -> Result<usize, StreamClosed> {
            let Some(mut chunk) = self.input.pop_front() else {
                return Ok(0);
            };
            let n = chunk.len().min(buf.len());
            buf[..n].copy_from_slice(&chunk[..n]);
            if n < chunk.len() {
                self.input.push_front(chunk.split_off(n));
            }
            Ok(n)
        }

        async fn write_all(&mut self, buf: &[u8]) -> Result<(), StreamClosed> {
            self.output.extend_from_slice(buf);
            Ok(())
        }

        async fn close(&mut self) {}
    }

    /// Accept over a [`MemStream`] fed `input` with a receive buffer of
    /// `rx_len`, run `body`; its result and everything the server wrote.
    fn run<T>(
        input: Vec<Vec<u8>>,
        rx_len: usize,
        body: impl AsyncFnOnce(Result<WsConnection<'_, &mut MemStream>, AcceptError>) -> T,
    ) -> (T, Vec<u8>) {
        let mut stream = MemStream {
            input: input.into(),
            output: Vec::new(),
        };
        let mut rx = vec![0u8; rx_len];
        let result = block_on(async {
            let conn = WsConnection::accept(&mut stream, &mut rx).await;
            body(conn).await
        });
        (result, stream.output)
    }

    /// An echo server on a loopback port, one connection; its result is how
    /// the connection ended.
    fn spawn_echo_server(rx_len: usize) -> (u16, JoinHandle<Result<WsClosed, AcceptError>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let (socket, _) = listener.accept().unwrap();
            socket.set_nodelay(true).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut rx = vec![0u8; rx_len];
            block_on(async {
                let mut conn = WsConnection::accept(TcpByteStream(socket), &mut rx).await?;
                loop {
                    let message = match conn.recv().await {
                        Ok(message) => message.to_vec(),
                        Err(why) => return Ok(why),
                    };
                    if let Err(why) = conn.send(&message).await {
                        return Ok(why);
                    }
                }
            })
        });
        (port, server)
    }

    /// Fixed bytes: a client key and masks the tests can predict.
    fn test_entropy(buf: &mut [u8]) {
        buf.fill(0x5a);
    }

    /// A real (tungstenite) server on a loopback port, one connection at
    /// `path`: echoes binary messages, pings before echoing `ping`. Its
    /// result: the request's `Host`, the pongs it got, the close code.
    fn spawn_tungstenite_echo(
        path: &'static str,
    ) -> (u16, JoinHandle<(String, usize, Option<u16>)>) {
        use tungstenite::handshake::server::{Request, Response};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let (socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut host = String::new();
            let mut ws =
                tungstenite::accept_hdr(socket, |request: &Request, response: Response| {
                    assert_eq!(request.uri().path(), path);
                    host = request
                        .headers()
                        .get("host")
                        .and_then(|h| h.to_str().ok())
                        .unwrap_or_default()
                        .into();
                    Ok(response)
                })
                .unwrap();
            let mut pongs = 0;
            loop {
                match ws.read() {
                    Ok(Message::Binary(data)) => {
                        if data.as_ref() == b"ping" {
                            ws.send(Message::Ping(b"hub".to_vec().into())).unwrap();
                        }
                        ws.send(Message::Binary(data)).unwrap();
                    }
                    Ok(Message::Pong(_)) => pongs += 1,
                    Ok(Message::Close(frame)) => {
                        let _ = ws.flush();
                        return (host, pongs, frame.map(|f| u16::from(f.code)));
                    }
                    Ok(_) => {}
                    Err(_) => return (host, pongs, None),
                }
            }
        });
        (port, server)
    }

    fn connect(port: u16, path: &str) -> Result<Client, tungstenite::Error> {
        let (ws, _) = tungstenite::connect(format!("ws://127.0.0.1:{port}{path}"))?;
        if let tungstenite::stream::MaybeTlsStream::Plain(s) = ws.get_ref() {
            s.set_nodelay(true).unwrap();
            s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        }
        Ok(ws)
    }

    /// Read until the client errors; the close codes seen and the error.
    fn read_to_end(ws: &mut Client) -> (Vec<u16>, tungstenite::Error) {
        let mut codes = Vec::new();
        loop {
            match ws.read() {
                Ok(Message::Close(frame)) => {
                    codes.push(frame.map_or(CloseCode::NO_STATUS.0, |f| u16::from(f.code)))
                }
                Ok(other) => panic!("unexpected {other:?}"),
                Err(e) => return (codes, e),
            }
        }
    }

    fn valid_headers() -> Vec<&'static str> {
        vec![
            "Host: board",
            "Upgrade: websocket",
            "Connection: Upgrade",
            "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==",
            "Sec-WebSocket-Version: 13",
        ]
    }

    fn request(headers: &[&str]) -> String {
        let mut req = String::from("GET /link HTTP/1.1\r\n");
        for h in headers {
            req.push_str(h);
            req.push_str("\r\n");
        }
        req.push_str("\r\n");
        req
    }

    fn pattern(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i * 7 + 3) as u8).collect()
    }

    /// A small (≤ 125 bytes) client frame, masked.
    fn client_frame(b0: u8, payload: &[u8]) -> Vec<u8> {
        const MASK: [u8; 4] = [0xa1, 0x5c, 0x03, 0xfe];
        assert!(payload.len() <= 125);
        let mut out = vec![b0, 0x80 | payload.len() as u8];
        out.extend_from_slice(&MASK);
        out.extend(payload.iter().enumerate().map(|(i, b)| b ^ MASK[i & 3]));
        out
    }
}
