//! Component access on ArrayElement LValue

use crate::semantic::types::Type as GlslType;
use alloc::vec::Vec;
use cranelift_codegen::ir::Value;

use super::super::super::types::LValue;

/// Resolve component access on an ArrayElement LValue
pub fn resolve_component_on_array_element(
    array_ptr: Value,
    base_ty: GlslType,
    index: Option<usize>,
    index_val: Option<Value>,
    element_ty: GlslType,
    element_size_bytes: usize,
    indices: Vec<usize>,
) -> LValue {
    // Component access on array element: arr[i].x
    // Store component indices in the ArrayElement
    log::trace!(
        "resolve_lvalue: Component access on ArrayElement: element_ty={element_ty:?}, indices={indices:?}, index={index:?}, index_val={index_val:?}"
    );
    LValue::ArrayElement {
        array_ptr,
        base_ty,
        index,
        index_val,
        element_ty,
        element_size_bytes,
        component_indices: Some(indices),
    }
}
