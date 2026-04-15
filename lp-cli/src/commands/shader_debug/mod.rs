//! `shader-debug` — unified debug output for all backends.
//!
//! Replaces the old `shader-rv32c` and `shader-rv32n` commands with a
//! single interface that uses unified debug data structures for consistent output.

mod args;
mod collect;
mod comparison_table;
mod display;
mod handler;
mod types;

pub use args::Args;
pub use handler::handle_shader_debug;
pub use types::{BackendDebugData, BackendTarget, DebugReport, FunctionDebugData, SectionFilter};
