//! LValue resolution

use crate::error::{ErrorCode, GlslError, extract_span_from_expr, source_span_to_location};
use crate::frontend::codegen::context::CodegenContext;
use glsl::syntax::Expr;

use super::types::LValue;

mod component;
mod indexing;
mod variable;

use component::resolve_component_lvalue;
use indexing::resolve_indexing_lvalue;
use variable::resolve_variable_lvalue;

/// Resolve an expression to an LValue
///
/// Recursively analyzes the expression to determine the modifiable location.
/// Handles nested expressions like `m[0].x` by first resolving `m[0]` then extracting the component.
pub fn resolve_lvalue<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    expr: &Expr,
) -> Result<LValue, GlslError> {
    match expr {
        Expr::Variable(ident, _span) => resolve_variable_lvalue(ctx, ident),
        Expr::Dot(base_expr, field, _dot_span) => resolve_component_lvalue(ctx, base_expr, field),
        Expr::Bracket(array_expr, array_spec, span) => {
            resolve_indexing_lvalue(ctx, array_expr, array_spec, span)
        }
        _ => {
            let span = extract_span_from_expr(expr);
            Err(GlslError::new(
                ErrorCode::E0115,
                "expression is not a valid LValue (must be variable, component access, or matrix element)",
            )
            .with_location(source_span_to_location(&span)))
        }
    }
}
