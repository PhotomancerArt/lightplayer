//! Engine-side color math: authoring spaces in, canonical LinearSrgb out.
//!
//! `lpc-model`'s [`color`](lpc_model::color) module owns the palette
//! *vocabulary* — [`Gradient`](lpc_model::Gradient),
//! [`Colorspace`](lpc_model::Colorspace),
//! [`InterpMethod`](lpc_model::InterpMethod). This module owns what the engine
//! does with it: interpolate in the authored space, convert the result to
//! canonical, and write the height-one texture a `sampler2D` palette uniform
//! reads (`docs/design/color.md` §6, §7).
//!
//! The split is the one `color.md` §7 draws. Conversion is the *engine's*
//! responsibility, never the shader's: a shader only ever sees canonical F32
//! LinearSrgb, quantized to the texture's Unorm16 storage.

pub mod colorspace;
pub mod gradient_bake;

pub use colorspace::{interpolate_in_space, to_linear_srgb};
pub use gradient_bake::{
    PALETTE_BAKE_BYTES, PALETTE_BAKE_FORMAT, PALETTE_BAKE_WIDTH, bake_gradient_into,
    bake_gradient_mix_into, sample_gradient,
};
