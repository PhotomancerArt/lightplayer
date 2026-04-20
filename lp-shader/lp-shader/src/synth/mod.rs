//! Backend-agnostic LPIR synthesis passes.

pub mod render_texture;

pub use render_texture::{SynthError, render_texture_fn_name, synthesise_render_texture};
