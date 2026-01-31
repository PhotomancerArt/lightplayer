//! Array indexing resolution

use crate::error::{ErrorCode, GlslError, extract_span_from_identifier, source_span_to_location};
use crate::frontend::codegen::context::CodegenContext;
use crate::semantic::types::Type as GlslType;
use glsl::syntax::{ArraySpecifier, ArraySpecifierDimension, SourceSpan};

use alloc::format;

use super::super::super::super::expr::component;
use super::super::super::types::LValue;
use super::nested::resolve_nested_array_indexing;

/// Resolve array variable indexing
pub fn resolve_array_indexing<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    ident: &glsl::syntax::Identifier,
    array_spec: &ArraySpecifier,
    span: &SourceSpan,
) -> Result<LValue, GlslError> {
    let var_info = ctx.lookup_var_info(&ident.name).ok_or_else(|| {
        let span = extract_span_from_identifier(ident);
        let error = GlslError::undefined_variable(&ident.name)
            .with_location(source_span_to_location(&span));
        ctx.add_span_to_error(error, &span)
    })?;

    let base_ty = var_info.glsl_type.clone();

    let array_ptr = var_info.array_ptr.ok_or_else(|| {
        GlslError::new(
            ErrorCode::E0400,
            format!("variable '{}' is not an array", ident.name),
        )
        .with_location(source_span_to_location(span))
    })?;

    if array_spec.dimensions.0.is_empty() {
        return Err(
            GlslError::new(ErrorCode::E0400, "indexing requires explicit index")
                .with_location(source_span_to_location(span)),
        );
    }

    let index_expr = match &array_spec.dimensions.0[0] {
        ArraySpecifierDimension::ExplicitlySized(expr) => expr,
        ArraySpecifierDimension::Unsized => {
            return Err(
                GlslError::new(ErrorCode::E0400, "indexing requires explicit index")
                    .with_location(source_span_to_location(span)),
            );
        }
    };

    // Evaluate index (must be int)
    let (index_vals, index_ty) = ctx.emit_expr_typed(index_expr)?;
    if index_ty != GlslType::Int {
        return Err(GlslError::new(ErrorCode::E0106, "index must be int")
            .with_location(source_span_to_location(span)));
    }

    let index_val = index_vals[0];
    let element_ty = base_ty.array_element_type().unwrap();
    let array_size = base_ty.array_dimensions()[0];

    // Calculate element size in bytes (handles vectors/matrices)
    let element_size_bytes = ctx.calculate_array_element_size_bytes(&element_ty)?;

    // Extract compile-time constant index if available
    let compile_time_index = if let glsl::syntax::Expr::IntConst(n, _) = index_expr.as_ref() {
        let idx = *n as usize;
        if idx >= array_size {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!("array index {} out of bounds (max {})", idx, array_size - 1),
            )
            .with_location(source_span_to_location(span)));
        }
        Some(idx)
    } else {
        // Runtime index - emit bounds check
        component::emit_bounds_check(ctx, index_val, array_size, span)?;
        None
    };

    // If there are more dimensions and the element is a matrix/vector, continue processing
    if array_spec.dimensions.0.len() > 1 && (element_ty.is_matrix() || element_ty.is_vector()) {
        return resolve_nested_array_indexing(
            ctx,
            array_ptr,
            base_ty,
            compile_time_index,
            if compile_time_index.is_none() {
                Some(index_val)
            } else {
                None
            },
            element_ty,
            element_size_bytes,
            array_spec,
            span,
        );
    } else {
        // No more dimensions or element is scalar - return ArrayElement
        Ok(LValue::ArrayElement {
            array_ptr,
            base_ty,
            index: compile_time_index,
            index_val: if compile_time_index.is_none() {
                Some(index_val)
            } else {
                None
            },
            element_ty,
            element_size_bytes,
            component_indices: None,
        })
    }
}
