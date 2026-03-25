//! Core GLSL type and function-signature shapes (no parser, no codegen).
//!
//! Copied from `lp-glsl-frontend` for the **new** stack. The frontend crate keeps
//! its own definitions until it is retired or switched to depend on this crate.

#![no_std]

extern crate alloc;

mod functions;
mod types;

pub use functions::{FunctionSignature, ParamQualifier, Parameter};
pub use types::{StructId, Type};
