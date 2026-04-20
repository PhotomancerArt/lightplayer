//! Core GLSL type and function-signature shapes (no parser, no codegen).
//!
//! Includes [`LpsType`] / [`LpsValueF32`], std430 [`layout`], string path parsing ([`path`]),
//! texture layout ([`TextureStorageFormat`], [`TextureBuffer`]),
//! and path projection on types and values ([`path_resolve`], [`value_path`]).
//!
//! Used by `lps-exec`, `lpvm`, and `lps-filetests`.

#![no_std]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

pub mod layout;
pub mod lps_value_f32;
pub mod lps_value_q32;
pub mod path;
pub mod path_resolve;
mod sig;
pub mod texture_buffer;
pub mod texture_format;
mod types;
pub mod value_path;

pub use layout::{VMCTX_HEADER_SIZE, array_stride, round_up, type_alignment, type_size};
pub use lps_value_f32::LpsValueF32;
pub use lps_value_q32::{
    LpsValueQ32, LpsValueQ32Error, lps_value_f32_to_q32, q32_to_lps_value_f32,
};
pub use sig::{FnParam, LpsFnKind, LpsFnSig, LpsModuleSig, ParamQualifier};
pub use texture_buffer::TextureBuffer;
pub use texture_format::TextureStorageFormat;
pub use types::{LayoutRules, LpsType, StructMember};
