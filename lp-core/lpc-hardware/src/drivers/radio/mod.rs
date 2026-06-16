//! Small packet-radio abstractions for peer/device messages.
//!
//! Radio drivers expose openable devices that can subscribe to logical channels,
//! send [`RadioMessage`](radio_message::RadioMessage)s, and drain received
//! messages with overflow reporting.

pub mod radio_channel;
pub mod radio_driver;
pub mod radio_message;
pub mod virtual_radio_driver;
