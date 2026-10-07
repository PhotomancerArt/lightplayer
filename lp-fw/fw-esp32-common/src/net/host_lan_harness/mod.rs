//! A board's LAN path on the host, for tests (feature `host-lan-harness`;
//! Wi-Fi plan P06). **Test-only: no firmware turns this on.**
//!
//! Before an emulated board can join a network, this proves the whole path
//! a LAN client takes, with the board's own code at every step: a std
//! `TcpListener` on `127.0.0.1:0`; each connection upgraded by the P04
//! WebSocket server ([`crate::net::ws::WsConnection`]) over a std socket
//! ([`std_tcp_byte_stream`]); its binary messages carried to a LAN slot of
//! the real [`crate::radio_link::RadioLinkPort`] (made with `leak_locked`,
//! served from another thread under a mutex, as `lp-net` serves it on the
//! C6); the real [`crate::radio_link::LinkMuxTransport`] in front of a real
//! `lpa_server::LpServer` on a memory filesystem, run in `server_loop`'s
//! order ([`harness_server`]). A third concurrent connection is told
//! WebSocket close 1013 ([`harness_edge`]).
//!
//! What is the harness's own (and so not proven here): the TCP socket, the
//! threads and their polling executor ([`harness_block_on`]), the test-grade
//! entropy ([`harness_entropy`]), and a USB cable with nothing on it
//! ([`no_usb`]). Everything between the socket and the server is the
//! board's.
//!
//! std is allowed in this module only, behind its feature: the crate stays
//! `no_std` for every image.

pub mod harness_block_on;
pub mod harness_counters;
mod harness_edge;
pub mod harness_entropy;
mod harness_server;
pub mod lan_harness;
pub mod no_usb;
pub mod std_tcp_byte_stream;

pub use harness_counters::HarnessStats;
pub use lan_harness::{HarnessAccess, LanHarness, LanHarnessOptions};
