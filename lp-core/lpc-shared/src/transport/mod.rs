//! Transport traits for client-server communication
//!
//! This module defines traits for pluggable transport implementations.
//! Messages are consumed (moved) on send, and receive is non-blocking.
//!
//! Transports handle serialization/deserialization internally, working directly
//! with `ClientMessage` and `ServerMessage` types from `lp-model`.

pub mod incoming;
pub mod link;
pub mod link_id;
pub mod link_trust;
pub mod server;

// Re-export TransportError from lp-model for convenience
pub use incoming::Incoming;
pub use link::Link;
pub use link_id::LinkId;
pub use link_trust::LinkTrust;
pub use lpc_wire::TransportError;
pub use server::{
    ProjectReadEventSink, ProjectReadStreamSink, ServerTransport, transport_error_is_signalable,
};
