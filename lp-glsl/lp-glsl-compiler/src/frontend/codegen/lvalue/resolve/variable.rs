//! Variable expression resolution

use crate::error::{GlslError, extract_span_from_identifier, source_span_to_location};
use crate::frontend::codegen::context::CodegenContext;
use alloc::vec::Vec;

use super::super::types::{LValue, PointerAccessPattern};

/// Resolve a variable expression to an LValue
pub fn resolve_variable_lvalue<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    ident: &glsl::syntax::Identifier,
) -> Result<LValue, GlslError> {
    let span = extract_span_from_identifier(ident);

    // Get variable type first to check if it's an array
    let ty = ctx
        .lookup_variable_type(&ident.name)
        .ok_or_else(|| {
            let error = GlslError::undefined_variable(&ident.name)
                .with_location(source_span_to_location(&span));
            ctx.add_span_to_error(error, &span)
        })?
        .clone();

    // Check if this is an out/inout parameter
    // For non-arrays: has array_ptr but is not an array
    // For arrays: has array_ptr but no stack_slot (out/inout arrays don't have stack_slot)
    let var_info = ctx.lookup_var_info(&ident.name);
    let is_out_inout = var_info
        .map(|info| {
            info.array_ptr.is_some()
                && (!ty.is_array() || (ty.is_array() && info.stack_slot.is_none()))
        })
        .unwrap_or(false);

    // For out/inout parameters, create PointerBased LValue
    if is_out_inout {
        let ptr = var_info
            .and_then(|info| info.array_ptr)
            .expect("out/inout param must have pointer");
        let component_count = if ty.is_vector() {
            ty.component_count().unwrap()
        } else if ty.is_matrix() {
            ty.matrix_element_count().unwrap()
        } else if ty.is_array() {
            // For arrays, calculate total component count
            let element_ty = ty.array_element_type().unwrap();
            let array_size = ty.array_dimensions()[0];
            if element_ty.is_vector() {
                array_size * element_ty.component_count().unwrap()
            } else if element_ty.is_matrix() {
                array_size * element_ty.matrix_element_count().unwrap()
            } else {
                array_size
            }
        } else {
            1
        };
        return Ok(LValue::PointerBased {
            ptr,
            base_ty: ty,
            access_pattern: PointerAccessPattern::Direct { component_count },
        });
    }

    // For arrays (non-out/inout), return LValue::Variable with empty vars (arrays use pointer-based storage)
    if ty.is_array() {
        return Ok(LValue::Variable {
            vars: Vec::new(),
            ty,
        });
    }

    let vars = ctx
        .lookup_variables(&ident.name)
        .ok_or_else(|| {
            let error = GlslError::undefined_variable(&ident.name)
                .with_location(source_span_to_location(&span));
            ctx.add_span_to_error(error, &span)
        })?
        .to_vec();

    Ok(LValue::Variable { vars, ty })
}
