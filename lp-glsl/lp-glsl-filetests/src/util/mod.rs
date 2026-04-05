//! Shared utilities.

pub mod file_update;
mod value_parse;

// Re-exports
pub use file_update::{FileUpdate, format_glsl_value};
pub use value_parse::parse_lps_value_literal;
