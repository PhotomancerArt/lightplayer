//! Legacy authored node configuration and source-side types; see `lpc_source::legacy`.

#![allow(
    dead_code,
    reason = "legacy source parsers are retained as references while the slot/value model replaces them"
)]

pub use lpc_model::nodes::shader::glsl_opts;
pub mod toml_color;
mod toml_parse;
