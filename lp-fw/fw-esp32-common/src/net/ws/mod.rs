//! The board's minimal WebSocket code, both ends: the server of one route,
//! `ws://<board>/link`, carrying lp-link frames as binary messages (Wi-Fi
//! plan P04), and the client of the relay's device leg,
//! `ws://<relay>/relay/device` (Wi-Fi relay plan P6).
//!
//! Written from RFC 6455 and FIPS 180-4. The codec is sans-IO
//! ([`ws_handshake`], [`ws_client_handshake`], [`ws_frame`]);
//! [`WsConnection`] drives it over any [`ByteStream`] with runtime-neutral
//! futures.

pub mod byte_stream;
pub mod sha1;
pub mod ws_accept_key;
pub mod ws_client_handshake;
pub mod ws_connection;
pub mod ws_frame;
pub mod ws_handshake;

pub use byte_stream::{ByteStream, StreamClosed};
pub use ws_accept_key::accept_key;
pub use ws_client_handshake::{ClientHandshake, ClientKey, client_request, parse_response};
pub use ws_connection::{AcceptError, ConnectError, RX_OVERHEAD, WsClosed, WsConnection, WsEvent};
pub use ws_frame::{
    CloseCode, Decoded, Frame, FrameDecoder, Opcode, apply_mask, client_header, server_header,
};
pub use ws_handshake::{
    Handshake, LINK_PATH, MAX_REQUEST, Refusal, parse_request, upgrade_response,
};
