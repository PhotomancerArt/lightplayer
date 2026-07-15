//! Byte-stream seam under serial-class link providers.
//!
//! The [`DeviceByteStream`] trait is *defined* in `lpa-client` (`lpa-link`
//! already depends on `lpa-client`, so defining it here would invert the
//! dependency graph) and re-exported here as the link-side surface. Serial
//! providers hand an opened stream to
//! `lpa_client::transport_serial::create_hardware_serial_transport_pair_with_options`,
//! which runs the real `M!` framing over it:
//!
//! - `host-serial-esp32` opens a [`SerialPortByteStream`] on a native port.
//! - The `fake-device` feature's `FakeEsp32Device` implements the trait over
//!   a scripted in-memory device (see `providers::fake_device`).

pub use lpa_client::stream::{ByteStreamError, DeviceByteStream};

#[cfg(feature = "host-serial-esp32")]
pub use lpa_client::stream::SerialPortByteStream;
