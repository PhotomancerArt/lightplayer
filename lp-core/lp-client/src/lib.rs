//! LightPlayer client library for communicating with LightPlayer servers.
//!
//! This crate provides a no_std-compatible client for sending requests and receiving
//! responses from LightPlayer servers over various transport mechanisms.

#![no_std]

extern crate alloc;

pub mod channel;
pub mod client;
pub mod error;
pub mod transport;

pub use client::LpClient;
pub use error::ClientError;

pub use transport::client::ClientTransport;

#[cfg(feature = "std")]
pub use transport::local::LocalTransport;
