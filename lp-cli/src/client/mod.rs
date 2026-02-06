// Re-export everything from lp-client for backward compatibility
pub use lp_client::*;

// CLI-specific modules
pub mod client_connect;
pub mod local_server;
pub mod serial_port;

// Re-export CLI-specific types
pub use client_connect::client_connect;
