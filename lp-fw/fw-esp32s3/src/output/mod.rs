//! Output for the ESP32-S3 app layer.
//!
//! The provider itself is chip-agnostic and comes from `fw-esp32-common`
//! unchanged; only the *driver* below it is chip-side. This milestone does not
//! port the RMT/ws281x driver (the WS2811 session owns it), so the driver
//! registered here writes to the serial log instead of to LEDs — see
//! [`readout_driver`].

pub mod readout_driver;

pub use fw_esp32_common::output::provider::Esp32OutputProvider;
pub use readout_driver::SerialReadoutWs281xDriver;
