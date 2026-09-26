//! Async serial transport implementations
//!
//! Provides generic async serial transport that can work with emulator or hardware serial.
//! Factory functions create the appropriate transport for each use case.
//!
//! # Wire encoding
//!
//! Every serial transport reads the board's stream through
//! [`lpc_wire::WireStream`], so console lines, `M!{json}` lines and packed
//! frames (`\n 0x00 'P' COBS 0x00`) all arrive, whichever the board is
//! writing. Each one also **asks for packed** on connect: when the board's
//! hello names this build's dictionary it writes
//! `ClientRequest::SetEncoding` itself — after the hello, before the traffic
//! that follows it — swallows the answer, and asks again (at most once per
//! few seconds) if the board falls back to JSON mid-session
//! ([`lpc_wire::PackOptIn`]). `LP_WIRE_ENCODING=json` turns the asking off
//! ([`crate::wire_encoding_env`]).

mod client;
#[cfg(feature = "serial")]
mod emulator;
#[cfg(feature = "serial")]
mod hardware;

pub use client::AsyncSerialClientTransport;
#[cfg(feature = "serial")]
pub use emulator::{BacktraceInfo, create_emulator_serial_transport_pair};
#[cfg(feature = "serial")]
pub use hardware::{
    HardwareSerialOptions, SerialLineObserver, create_hardware_serial_transport_pair,
    create_hardware_serial_transport_pair_with_options,
};
