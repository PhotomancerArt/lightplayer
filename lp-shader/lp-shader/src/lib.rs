//! High-level shader compilation and texture API.
//!
//! Wraps `lps-frontend` + `lpvm` so consumers do not duplicate the
//! compile → lower → `LpvmEngine::compile` pipeline.
//!
//! Helpers [`texture_binding`], [`CompilePxDesc::with_texture_spec`], and
//! [`LpsTextureBuf::to_named_texture_uniform`] pair compile-time [`TextureBindingSpec`] maps with
//! runtime uniform structs for [`LpsPxShader::render_frame`]. Higher layers (lpfx/domain and similar)
//! own baking palette or gradient texels into a buffer (typically height `== 1`) and must pass a
//! matching spec—for example [`texture_binding::height_one`] when sampling ignores the vertical axis.

#![no_std]

extern crate alloc;

mod compile_px_desc;
mod engine;
mod error;
mod px_shader;
mod runtime_texture_validation;
pub mod synth;
mod texture_buf;
mod texture_interface;

pub use compile_px_desc::{CompilePxDesc, TextureBindingSpecs, texture_binding};
pub use engine::LpsEngine;
pub use error::LpsError;
pub use lps_shared::{LpsTexture2DDescriptor, LpsTexture2DValue};
pub use lpvm::AllocError;
pub use px_shader::LpsPxShader;
pub use texture_buf::LpsTextureBuf;

pub use lps_shared::{
    LpsModuleSig, LpsValueF32, TextureBindingSpec, TextureBuffer, TextureStorageFormat,
};

#[cfg(test)]
mod tests;
