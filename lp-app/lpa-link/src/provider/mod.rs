//! Provider-facing link model and controller trait.
//!
//! This module defines the provider-neutral vocabulary used by every concrete
//! link implementation: endpoints that can be discovered, sessions that are
//! opened from endpoints, protocol connections handed to higher layers, logs,
//! diagnostics, capabilities, and the `LinkProvider` controller trait.
//!
//! The types here are lightweight records and ids. Concrete resources such as
//! serial ports, browser workers, spawned host runtimes, and protocol handles
//! stay owned by the provider implementation that created them.

pub mod connection;
pub mod diagnostic;
pub mod endpoint;
pub mod error;
pub mod log;
pub mod management_progress;
pub mod management_request;
pub mod management_result;
pub mod operation;
pub mod provider;
pub mod session;
