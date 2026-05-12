//! Backend-agnostic LPIR synthesis passes.

pub mod render_samples;
pub mod render_texture;

pub use render_samples::{RENDER_SAMPLES_RGBA16_FN, synthesise_render_samples_rgba16};
pub use render_texture::{SynthError, render_texture_fn_name, synthesise_render_texture};
