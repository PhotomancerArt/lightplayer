//! Component access resolution

use crate::error::{
    ErrorCode, GlslError, extract_span_from_expr, extract_span_from_identifier,
    source_span_to_location,
};
use crate::frontend::codegen::context::CodegenContext;
use crate::semantic::types::Type as GlslType;
use glsl::syntax::Expr;

use alloc::format;

use super::super::super::expr::component;
use super::super::types::LValue;

mod array_element;
mod matrix_column;
mod nested;
mod variable;

use array_element::resolve_component_on_array_element;
use matrix_column::resolve_component_on_matrix_column;
use nested::resolve_component_on_component;
use variable::{resolve_component_on_pointer_based, resolve_component_on_variable};

/// Resolve component access (Dot expression) to an LValue
pub fn resolve_component_lvalue<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    base_expr: &Expr,
    field: &glsl::syntax::Identifier,
) -> Result<LValue, GlslError> {
    // Recursively resolve the base expression to an LValue
    use super::super::resolve_lvalue;
    let base_lvalue = resolve_lvalue(ctx, base_expr)?;

    // Extract component indices from the field name
    let base_ty = match &base_lvalue {
        LValue::Variable { ty, .. } => ty.clone(),
        LValue::Component { result_ty, .. } => result_ty.clone(),
        LValue::MatrixColumn { result_ty, .. } => result_ty.clone(),
        LValue::ArrayElement { element_ty, .. } => element_ty.clone(),
        LValue::PointerBased { base_ty, .. } => base_ty.clone(),
        LValue::MatrixElement { .. } | LValue::VectorElement { .. } => {
            // Can't access components of a scalar
            let span = extract_span_from_expr(base_expr);
            return Err(
                GlslError::new(ErrorCode::E0112, "component access on scalar value")
                    .with_location(source_span_to_location(&span)),
            );
        }
    };

    if !base_ty.is_vector() {
        let span = extract_span_from_expr(base_expr);
        return Err(GlslError::new(
            ErrorCode::E0112,
            format!("component access on non-vector type: {base_ty:?}"),
        )
        .with_location(source_span_to_location(&span)));
    }

    let field_span = extract_span_from_identifier(field);
    let indices = component::parse_vector_swizzle(&field.name, &base_ty, Some(field_span.clone()))?;
    let base_component_ty = base_ty.vector_base_type().unwrap();

    let result_ty = if indices.len() == 1 {
        base_component_ty.clone()
    } else {
        GlslType::vector_type(&base_component_ty, indices.len()).ok_or_else(|| {
            GlslError::new(
                ErrorCode::E0400,
                format!("cannot create vector of size {}", indices.len()),
            )
            .with_location(source_span_to_location(&field_span))
        })?
    };

    // Get the base variables and compute indices based on the base LValue type
    match base_lvalue {
        LValue::Variable {
            vars, ty: base_ty, ..
        } => {
            // Regular variable: use SSA vars
            // Out/inout parameters now use PointerBased variant directly
            Ok(resolve_component_on_variable(
                vars, base_ty, indices, result_ty,
            ))
        }
        LValue::Component {
            base_vars, base_ty, ..
        } => Ok(resolve_component_on_component(
            base_vars, base_ty, indices, result_ty,
        )),
        LValue::MatrixColumn {
            base_vars,
            base_ty: matrix_ty,
            col,
            ..
        } => Ok(resolve_component_on_matrix_column(
            base_vars, matrix_ty, col, indices, result_ty,
        )),
        LValue::ArrayElement {
            array_ptr,
            base_ty,
            index,
            index_val,
            element_ty,
            element_size_bytes,
            ..
        } => Ok(resolve_component_on_array_element(
            array_ptr,
            base_ty,
            index,
            index_val,
            element_ty,
            element_size_bytes,
            indices,
        )),
        LValue::PointerBased { ptr, base_ty, .. } => {
            // Component access on PointerBased (out/inout parameter)
            Ok(resolve_component_on_pointer_based(
                ptr,
                base_ty.clone(),
                indices,
                result_ty,
            ))
        }
        LValue::MatrixElement { .. } | LValue::VectorElement { .. } => unreachable!(), // Already handled above
    }
}
