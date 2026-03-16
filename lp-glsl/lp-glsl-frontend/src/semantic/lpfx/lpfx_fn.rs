//! LPFX function definitions
//!
//! Core data structures for representing LPFX functions and their implementations.

use crate::semantic::functions::ParamQualifier;
use crate::semantic::types::Type;
use lp_glsl_builtin_ids::BuiltinId;

/// Parameter with static references (used by LPFX registry)
#[derive(Debug)]
pub struct ParameterRef {
    pub name: &'static str,
    pub ty: Type,
    pub qualifier: ParamQualifier,
}

/// Function signature with static references (used by LPFX registry)
#[derive(Debug)]
pub struct FunctionSignatureRef {
    pub name: &'static str,
    pub return_type: Type,
    pub parameters: &'static [ParameterRef],
}

/// LPFX function definition
///
/// Contains the GLSL signature and all available implementations for different decimal formats.
pub struct LpfxFn {
    /// GLSL signature of the function (name, parameters, return type)
    pub glsl_sig: FunctionSignatureRef,

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
