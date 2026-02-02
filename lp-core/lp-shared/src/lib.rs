//! Shared utilities and abstractions for LightPlayer.
//!
//! This crate provides common functionality used across multiple LightPlayer crates:
//! - File system abstractions (memory, std, view)
//! - Output providers and memory management
//! - Time providers
//! - Texture utilities
//! - Project building utilities
//! - Transport server implementations

#![no_std]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

pub mod error;
pub mod fs;
pub mod util; // Temporarily enabled for Texture

pub mod output;
pub mod project;
pub mod time;
pub mod transport;

pub use error::{FsError, OutputError, TextureError};
// Re-export TransportError from lp-model for convenience
pub use lp_model::TransportError;
pub use project::ProjectBuilder;
pub use util::texture::Texture;
