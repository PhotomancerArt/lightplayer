// Re-export everything from lp-client for backward compatibility
pub use lp_client::*;

// CLI-specific modules
pub mod client_connect;
pub mod local_server;

// Re-export CLI-specific types
pub use client_connect::client_connect;
