//! Function signature shapes (no registry / overload resolution).

use alloc::{string::String, vec::Vec};

use crate::LpsType;

/// Signature for LightPlayer Shader functions
#[derive(Debug, Clone)]
pub struct LpsFnSig {
    pub name: String,
    pub return_type: LpsType,
    pub parameters: Vec<FnParam>,
}

#[derive(Debug, Clone)]
pub struct FnParam {
    pub name: String,
    pub ty: LpsType,
    pub qualifier: ParamQualifier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamQualifier {
    In,
    Out,
    InOut,
}
