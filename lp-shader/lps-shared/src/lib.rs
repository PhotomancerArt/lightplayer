//! Core GLSL type and function-signature shapes (no parser, no codegen).
//!
//! Includes [`LpsType`] / [`LpsValueF32`], std430 [`layout`], string path parsing ([`path`]),
//! texture layout ([`TextureStorageFormat`], [`TextureBindingSpec`], [`TextureBuffer`]),
//! path projection on types and values ([`path_resolve`], [`value_path`]), and the
//! shared `uniform sampler2D` declaration recognizer ([`sampler2d_decl`]) both
//! compiler tiers rewrite from.
//!
//! Used by `lps-exec`, `lpvm`, and `lps-filetests`.

#![no_std]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

pub mod layout;
pub mod lps_buffer;
pub mod lps_value_f32;
pub mod lps_value_q32;
pub mod path;
pub mod path_resolve;
pub mod sampler2d_decl;
mod sig;
pub mod texture_binding_validate;
pub mod texture_buffer;
pub mod texture_format;
mod types;
pub mod value_path;

pub use layout::{VMCTX_HEADER_SIZE, array_stride, round_up, type_alignment, type_size};
pub use lps_buffer::{LpsBuffer, LpsBufferElem};
pub use lps_value_f32::LpsValueF32;
pub use lps_value_q32::{
    FloatLaneAbi, LpsValueQ32, LpsValueQ32Error, lanes_to_lps_value_f32, lps_value_f32_to_lanes,
    lps_value_f32_to_q32, q32_to_lps_value_f32,
};
pub use sampler2d_decl::{
    Sampler2DDecl, Sampler2DScan, Sampler2DSite, scan_uniform_sampler2d_decls,
    strip_comments_and_directives,
};
pub use sig::{FnParam, LpsFnKind, LpsFnSig, LpsModuleSig, ParamQualifier};
pub use texture_binding_validate::validate_texture_binding_specs_against_module;
pub use texture_buffer::TextureBuffer;
pub use texture_format::{
    LpsTexture2DDescriptor, LpsTexture2DValue, TextureBindingSpec, TextureFilter, TextureShapeHint,
    TextureStorageFormat, TextureWrap,
};
pub use types::{LayoutRules, LpsType, StructMember, glsl_type_name};
