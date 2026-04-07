//! Core GLSL type and function-signature shapes (no parser, no codegen).
//!
//! Includes [`LpsType`] / [`LpsValue`], std430 [`layout`], string path parsing ([`path`]),
//! and path projection on types and values ([`path_resolve`], [`value_path`]).
//!
//! Used by `lps-exec`, `lpvm`, and `lps-filetests`.

#![no_std]

extern crate alloc;

pub mod layout;
pub mod lps_value;
pub mod lps_value_f64;
pub mod lps_value_f64_convert;
pub mod path;
pub mod path_resolve;
mod sig;
mod types;
pub mod value_path;

pub use layout::{array_stride, round_up, type_alignment, type_size};
pub use lps_value::LpsValue;
pub use sig::{FnParam, LpsFnSig, LpsModuleSig, ParamQualifier};
pub use types::{LayoutRules, LpsType, StructMember};
