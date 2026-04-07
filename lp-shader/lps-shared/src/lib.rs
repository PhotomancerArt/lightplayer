//! Core GLSL type and function-signature shapes (no parser, no codegen).
//!
//! Includes [`LpsType`] / [`LpsValueF32`], std430 [`layout`], string path parsing ([`path`]),
//! and path projection on types and values ([`path_resolve`], [`value_path`]).
//!
//! Used by `lps-exec`, `lpvm`, and `lps-filetests`.

#![no_std]

extern crate alloc;

pub mod layout;
pub mod lps_value_f32;
pub mod lps_value_f64;
pub mod lps_value_f64_convert;
mod lps_value_q32;
pub mod path;
pub mod path_resolve;
mod sig;
mod types;
pub mod value_path;

pub use layout::{array_stride, round_up, type_alignment, type_size};
pub use lps_value_f32::LpsValueF32;
pub use lps_value_q32::LpsValueQ32;
pub use sig::{FnParam, LpsFnSig, LpsModuleSig, ParamQualifier};
pub use types::{LayoutRules, LpsType, StructMember};
