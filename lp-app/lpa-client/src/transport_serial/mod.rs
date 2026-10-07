//! Async serial transport implementations
//!
//! Provides generic async serial transport that can work with emulator or hardware serial.
//! Factory functions create the appropriate transport for each use case.
//!
//! # The link
//!
//! A board's USB serial link is an lp-link since `WIRE_PROTO_VERSION` 30
//! (plan `lp2025/2026-09-27-0215-lp-link-usb-cutover`): the hardware
//! transport ([`create_hardware_serial_transport_pair_with_options`]) runs one
//! [`lpc_wire::WireLinkPort`] per stream — native ports, emulated boards'
//! `serial:tcp://` and `serial:ws://` doors, and the fake board alike. Lost or
//! damaged bytes are resent under the messages, a link reset fails the
//! requests it lost at once ([`crate::link_reset`]), and on each link session
//! the transport **asks for packed** replies once the board's hello names
//! this build's pack format. `LP_WIRE_ENCODING=json` turns the asking off
//! ([`crate::wire_encoding_env`]).
//!
//! The fw-emu transport ([`create_emulator_serial_transport_pair`]) is not a
//! USB link: fw-emu speaks `M!{json}` lines over a lossless syscall pipe
//! (plan D3), read through [`lpc_wire::WireStream`].

mod client;
#[cfg(feature = "serial")]
mod emulator;
#[cfg(feature = "serial")]
mod hardware;
#[cfg(feature = "serial")]
mod link_nonce;
#[cfg(feature = "serial")]
mod link_pump;

pub use client::AsyncSerialClientTransport;
// The LAN transport (`crate::transport_lan`) rides the same client half.
#[cfg(feature = "lan")]
pub(crate) use client::SerialInbound;
#[cfg(feature = "serial")]
pub use emulator::{BacktraceInfo, create_emulator_serial_transport_pair};
#[cfg(feature = "serial")]
pub use hardware::{
    HardwareSerialOptions, SerialLineObserver, create_hardware_serial_transport_pair,
    create_hardware_serial_transport_pair_with_options, link_config_for_port,
};
#[cfg(feature = "serial")]
pub use link_nonce::fresh_link_nonce;
