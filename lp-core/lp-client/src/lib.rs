//! LightPlayer client library.
//!
//! Provides client-side functionality for communicating with LpServer.
//! Includes transport implementations and the main LpClient struct.

pub mod client;
pub mod local;
pub mod specifier;
pub mod transport;
pub mod transport_ws;

// Re-export main types
pub use client::{LpClient, serializable_response_to_project_response};
pub use local::{
    AsyncLocalClientTransport, AsyncLocalServerTransport, create_local_transport_pair,
};
pub use specifier::HostSpecifier;
pub use transport::ClientTransport;
pub use transport_ws::WebSocketClientTransport;
