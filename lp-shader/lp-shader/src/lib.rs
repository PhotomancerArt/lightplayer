//! High-level shader compilation and texture API.
//!
//! Wraps `lps-frontend` + `lpvm` so consumers do not duplicate the
//! compile → lower → `LpvmEngine::compile` pipeline.

#![no_std]

extern crate alloc;

mod engine;
mod error;
mod frag_shader;
mod texture_buf;

pub use engine::LpsEngine;
pub use error::LpsError;
pub use frag_shader::LpsFragShader;
pub use lpvm::AllocError;
pub use texture_buf::LpsTextureBuf;

pub use lps_shared::{LpsModuleSig, LpsValueF32, TextureBuffer, TextureStorageFormat};

#[cfg(all(test, feature = "cranelift"))]
mod tests;
