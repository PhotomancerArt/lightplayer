//! Core GLSL type and function-signature shapes (no parser, no codegen).
//!
//! Used by `lp-glsl-exec` and `lp-glsl-filetests` (shared signatures / types; frontend lives in
//! `lp-glsl-naga` without depending on this crate).

#![no_std]

extern crate alloc;

mod sig;
mod types;

pub use sig::{FnParam, LpsFnSig, ParamQualifier};
pub use types::{LayoutRules, LpsType, StructMember};
