//! `lp-cli pattern` — host tooling for catalog patterns.
//!
//! `pattern preview` renders patterns on standard lamp shapes and records
//! the frames as JSON for the pattern review page
//! (`scripts/pattern-review/`). This is an edge tool: it runs the host
//! engine (`lpvm-wasm` under wasmtime, Q32 unless a shader pins
//! `float_mode`), never the device, and says nothing about device fps.

pub mod args;
pub mod base64;
pub mod handler;
pub mod knob_override;
pub mod pattern_source;
pub mod preview_record;
pub mod preview_render;
pub mod swatch;
pub mod swatch_rig;

pub use args::PatternCli;
pub use handler::handle_pattern;
