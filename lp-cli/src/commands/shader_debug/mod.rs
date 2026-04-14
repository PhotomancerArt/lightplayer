//! `shader-debug` — unified debug output for all backends.
//!
//! Replaces the old `shader-rv32` and `shader-rv32fa` commands with a
//! single interface that uses `ModuleDebugInfo` for consistent output.

mod args;
mod handler;

pub use args::Args;
pub use handler::handle_shader_debug;
