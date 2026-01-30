//! LightPlayer CLI library
//!
//! This library exposes the CLI functionality for use in tests and as a library.

pub mod client;
pub mod commands;
pub mod config;
pub mod debug_ui;
pub mod error;
pub mod messages;
pub mod server;

// Re-export commonly used types for tests
pub use commands::dev;
