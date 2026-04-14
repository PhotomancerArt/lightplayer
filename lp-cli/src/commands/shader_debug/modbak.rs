//! `shader-debug` — unified debug output for all backends.
//!
//! Replaces the old `shader-rv32` and `shader-rv32fa` commands with a
//! single interface that uses unified debug data structures for consistent output.

mod args;
mod collect;
mod display;
mod handler;
mod types;

pub use args::Args;
pub use handler::handle_shader_debug;
pub use types::{BackendTarget, DebugReport, FunctionDebugData, BackendDebugData, SectionFilter};
