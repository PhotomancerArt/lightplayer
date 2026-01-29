//! Component access on Variable LValue

use crate::semantic::types::Type as GlslType;
use alloc::vec::Vec;
use cranelift_codegen::ir::Value;
use cranelift_frontend::Variable;

use super::super::super::types::{LValue, PointerAccessPattern};

/// Resolve component access on a Variable LValue
pub fn resolve_component_on_variable(
    vars: Vec<Variable>,
    base_ty: GlslType,
    indices: Vec<usize>,
    result_ty: GlslType,
) -> LValue {
    LValue::Component {
        base_vars: vars,
        base_ty,
        indices,
        result_ty,
    }
}

/// Resolve component access on a PointerBased LValue (out/inout parameter)
pub fn resolve_component_on_pointer_based(
    ptr: Value,
    base_ty: GlslType,
    indices: Vec<usize>,
    result_ty: GlslType,
) -> LValue {
    LValue::PointerBased {
        ptr,
        base_ty,
        access_pattern: PointerAccessPattern::Component { indices, result_ty },
    }
}
