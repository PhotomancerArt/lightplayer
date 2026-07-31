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
// `backtrace`'s Xtensa arm reads `a0`/`a1` and forces a register-window spill,
// both of which need inline asm; Xtensa asm is still unstable upstream. Scoped
// to the Xtensa target so every other build stays on stable features.
#![cfg_attr(target_arch = "xtensa", feature(asm_experimental_arch))]
#![cfg_attr(
    target_arch = "xtensa",
    allow(
        unstable_features,
        reason = "asm_experimental_arch is required to read Xtensa's windowed \
                  a0/a1 in backtrace::capture_frames_arch; Xtensa builds only"
    )
)]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

pub mod backtrace;
pub mod error;
pub mod fps;
pub mod stats;
pub mod util; // Temporarily enabled for Texture

pub mod display_pipeline;
pub mod output;
pub mod project;
pub mod time;
pub mod transport;

pub use display_pipeline::{DisplayPipeline, DisplayPipelineOptions};
pub use error::{DisplayPipelineError, TextureError};
pub use lpc_hardware::OutputError;
// Re-export TransportError from lp-model for convenience
pub use lpc_wire::TransportError;
pub use project::ProjectBuilder;
#[allow(
    deprecated,
    reason = "deprecated Texture retained until callers migrate to LpsTextureBuf"
)]
pub use util::texture::Texture;
