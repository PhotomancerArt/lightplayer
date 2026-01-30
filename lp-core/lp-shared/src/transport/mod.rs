//! Transport traits for client-server communication
//!
//! This module defines traits for pluggable transport implementations.
//! Messages are consumed (moved) on send, and receive is non-blocking.
//!
//! Transports handle serialization/deserialization internally, working directly
//! with `ClientMessage` and `ServerMessage` types from `lp-model`.

pub mod server;

// Re-export TransportError from lp-model for convenience
pub use lp_model::TransportError;
pub use server::ServerTransport;
