//! Core GLSL type and function-signature shapes (no parser, no codegen).
//!
//! Used by `lp-glsl-exec` and `lp-glsl-filetests` (shared signatures / types; frontend lives in
//! `lp-glsl-naga` without depending on this crate).

#![no_std]

extern crate alloc;

mod functions;
mod types;

pub use functions::{FunctionSignature, ParamQualifier, Parameter};
pub use types::{StructId, Type};
