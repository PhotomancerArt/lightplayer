//! Core GLSL type and function-signature shapes (no parser, no codegen).
//!
//! Used by `lp-glsl-naga`, filetests, and `lp-glsl-exec`.

#![no_std]

extern crate alloc;

mod functions;
mod types;

pub use functions::{FunctionSignature, ParamQualifier, Parameter};
pub use types::{StructId, Type};
