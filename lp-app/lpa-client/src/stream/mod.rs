//! Byte-stream seam under the serial-class transports.
//!
//! [`DeviceByteStream`] is the narrow trait between "a serial-class device
//! attachment" (a raw byte pipe with DTR/RTS control) and the framing
//! transport built on top of it (`transport_serial::hardware`). Implementors:
//!
//! - [`SerialPortByteStream`] — a native `serialport` port (feature `serial`)
//! - [`TcpByteStream`] — an emulator's UART over TCP
//! - [`WsByteStream`] — an emulator's byte link over a WebSocket
//!   (feature `ws-stream`, which `serial` turns on): what
//!   `lp-cli emu serve` serves and what a browser can also reach
//! - `lpa-link`'s `FakeEsp32Device` — a scripted in-memory device
//!   (feature `fake-device` on `lpa-link`)
//! - later: a PTY or browser byte shuttle
//!
//! The trait lives in `lpa-client` (not `lpa-link`) because `lpa-link`
//! already depends on `lpa-client`; placing it here keeps the dependency
//! graph acyclic. `lpa-link` re-exports it from `lpa_link::stream`.

pub mod device_byte_stream;
#[cfg(feature = "serial")]
pub mod serialport_stream;
pub mod tcp_stream;
#[cfg(feature = "ws-stream")]
pub mod ws_stream;

pub use device_byte_stream::{ByteStreamError, DeviceByteStream};
#[cfg(feature = "serial")]
pub use serialport_stream::SerialPortByteStream;
pub use tcp_stream::TcpByteStream;
#[cfg(feature = "ws-stream")]
pub use ws_stream::WsByteStream;
