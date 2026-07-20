//! LightPlayer graphics abstraction (`no_std + alloc`).
//!
//! This crate is the seam between the engine (`lpc-engine`) and shader
//! execution backends: the object-safe traits [`LpGraphics`] / [`LpShader`] /
//! [`LpComputeShader`], the opaque RAII resource handles
//! ([`TextureHandle`], [`SamplePointsHandle`], [`SampleOutHandle`]), the
//! backend error type [`GfxError`], and [`ShaderCompileOptions`] with its
//! explicit [`ShaderSemantics`] tier.
//!
//! # Backend doctrine
//!
//! - **One guaranteed CPU backend per target.** Every compile target gets
//!   exactly one cfg-selected CPU implementation
//!   (`lp-gfx-lpvm::LpvmGraphics`); it is always present and always able to
//!   compile and run shaders. This is the product path on embedded targets.
//! - **Optional accelerated backends.** A GPU backend (`lp-gfx-wgpu`) may
//!   additionally be constructed on capable targets. Which backend serves a
//!   given runtime is decided at runtime creation, by the host.
//! - **Selection is explicit, never silent.** A backend must *error* on
//!   options it cannot honor — most importantly the
//!   [`ShaderSemantics`] tier in [`ShaderCompileOptions`] — instead of
//!   silently substituting different semantics. Which tier/backend is active
//!   is user-visible state; see
//!   `docs/adr/2026-07-09-preview-fidelity-tiers.md`.
//!
//! # Handle lifetime rules
//!
//! Handles own their backend allocation: dropping a handle returns the
//! allocation to the backend that created it (each handle carries an
//! `Arc<dyn HandleAllocator>`). There are no manual free calls and no
//! backend pointers in the public API — texel access goes through
//! [`LpGraphics::read_back`] and the texel-upload/write methods, which move
//! owned bytes across the seam. A handle must only be used with the
//! [`LpGraphics`] that created it; backends reject foreign handles with
//! [`GfxError::Backend`].

#![no_std]

extern crate alloc;

pub mod compute_shader;
pub mod gfx_error;
pub mod graphics;
pub mod handle_allocator;
pub mod sample_out_handle;
pub mod sample_points_handle;
pub mod shader;
pub mod shader_compile_options;
pub mod shader_semantics;
pub mod texture_data;
pub mod texture_handle;

pub use compute_shader::LpComputeShader;
pub use gfx_error::GfxError;
pub use lp_shader::{ShaderFuelTrap, ShaderFuelTrapEntry};
pub use graphics::LpGraphics;
pub use handle_allocator::{HandleAllocator, HandleBacking};
pub use sample_out_handle::SampleOutHandle;
pub use sample_points_handle::SamplePointsHandle;
pub use shader::{LpShader, ShaderCompileStats};
pub use shader_compile_options::ShaderCompileOptions;
pub use shader_semantics::ShaderSemantics;
pub use texture_data::TextureData;
pub use texture_handle::TextureHandle;
