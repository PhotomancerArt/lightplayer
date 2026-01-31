//! Component access on Component LValue (nested)

use crate::semantic::types::Type as GlslType;
use alloc::vec::Vec;
use cranelift_frontend::Variable;

use super::super::super::types::LValue;

/// Resolve component access on a Component LValue (nested component access)
pub fn resolve_component_on_component(
    base_vars: Vec<Variable>,
    base_ty: GlslType,
    indices: Vec<usize>,
    result_ty: GlslType,
) -> LValue {
    LValue::Component {
        base_vars,
        base_ty,
        indices,
        result_ty,
    }
}
