//! Firmware core library.
//!
//! This crate provides the core functionality shared between firmware implementations,
//! including serial I/O and transport abstractions for embedded LightPlayer servers.

#![no_std]

#[cfg(any(feature = "emu", feature = "esp32"))]
pub mod log;

pub mod message_router;
pub mod serial;
pub mod test_messages;
pub mod transport;

pub use message_router::MessageRouter;
pub use test_messages::{
    TestCommand, TestResponse, deserialize_command, parse_message_line, serialize_command,
    serialize_response,
};
