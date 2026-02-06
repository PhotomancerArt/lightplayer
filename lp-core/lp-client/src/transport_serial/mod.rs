//! Async serial transport implementations
//!
//! Provides generic async serial transport that can work with emulator or hardware serial.
//! Factory functions create the appropriate transport for each use case.

mod client;
#[cfg(feature = "serial")]
mod emulator;
#[cfg(feature = "serial")]
mod hardware;

pub use client::AsyncSerialClientTransport;
#[cfg(feature = "serial")]
pub use emulator::create_emulator_serial_transport_pair;
#[cfg(feature = "serial")]
pub use hardware::create_hardware_serial_transport_pair;
