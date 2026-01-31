//! Indexing resolution (Bracket expressions)

use crate::error::{GlslError, source_span_to_location};
use crate::frontend::codegen::context::CodegenContext;
use glsl::syntax::{Expr, SourceSpan};

use super::super::types::LValue;

mod array;
mod helpers;
mod matrix_vector;
mod nested;

use array::resolve_array_indexing;
use matrix_vector::resolve_matrix_vector_indexing;

/// Resolve indexing (Bracket expression) to an LValue
pub fn resolve_indexing_lvalue<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    array_expr: &Expr,
    array_spec: &glsl::syntax::ArraySpecifier,
    span: &SourceSpan,
) -> Result<LValue, GlslError> {
    // Check if base is an array variable first
    if let Expr::Variable(ident, _) = array_expr {
        use crate::error::extract_span_from_identifier;
        let var_info = ctx.lookup_var_info(&ident.name).ok_or_else(|| {
            let span = extract_span_from_identifier(ident);
            let error = crate::error::GlslError::undefined_variable(&ident.name)
                .with_location(source_span_to_location(&span));
            ctx.add_span_to_error(error, &span)
        })?;

        let base_ty = var_info.glsl_type.clone();

        // Check if this is an array
        if base_ty.is_array() {
            return resolve_array_indexing(ctx, ident, array_spec, span);
        }
        // If not an array, fall through to matrix/vector handling
    }

    // Handle matrix/vector indexing
    resolve_matrix_vector_indexing(ctx, array_expr, array_spec, span)
}
