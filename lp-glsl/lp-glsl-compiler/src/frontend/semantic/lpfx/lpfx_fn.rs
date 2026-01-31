//! LPFX function definitions
//!
//! Core data structures for representing LPFX functions and their implementations.

use crate::backend::builtins::BuiltinId;
use crate::semantic::functions::FunctionSignature;

/// LPFX function definition
///
/// Contains the GLSL signature and all available implementations for different decimal formats.
pub struct LpfxFn {
    /// GLSL signature of the function (name, parameters, return type)
    pub glsl_sig: FunctionSignature,

    /// Available implementations for different decimal formats
    pub impls: LpfxFnImpl,
}

pub enum LpfxFnImpl {
    NonDecimal(BuiltinId),
    Decimal {
        float_impl: BuiltinId,
        q32_impl: BuiltinId,
    },
}
