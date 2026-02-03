//! LightPlayer client library.
//!
//! Provides client-side functionality for communicating with LpServer.
//! Includes transport implementations and the main LpClient struct.

pub mod client;
pub mod local;
pub mod specifier;
pub mod transport;
#[cfg(feature = "ws")]
pub mod transport_ws;

// Re-export main types
pub use client::{serializable_response_to_project_response, LpClient};
pub use local::{
    create_local_transport_pair, AsyncLocalClientTransport, AsyncLocalServerTransport,
};
pub use specifier::HostSpecifier;
pub use transport::ClientTransport;
#[cfg(feature = "ws")]
pub use transport_ws::WebSocketClientTransport;
