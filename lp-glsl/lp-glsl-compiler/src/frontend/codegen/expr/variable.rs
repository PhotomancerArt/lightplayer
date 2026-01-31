use crate::error::{GlslError, extract_span_from_identifier, source_span_to_location};
use crate::frontend::codegen::context::CodegenContext;
use crate::frontend::codegen::lvalue::emit_lvalue_as_rvalue;
use crate::frontend::codegen::rvalue::RValue;
use crate::semantic::types::Type as GlslType;
use cranelift_codegen::ir::Value;
use glsl::syntax::Expr;

use alloc::vec::Vec;

pub fn emit_variable<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    expr: &Expr,
) -> Result<(Vec<Value>, GlslType), GlslError> {
    let Expr::Variable(ident, _span) = expr else {
        unreachable!("translate_variable called on non-variable");
    };

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

    // For arrays, we can't evaluate them as RValues directly (they need indexing)
    // Return empty vec but correct type so type checking works
    if ty.is_array() {
        return Ok((Vec::new(), ty));
    }

    let vars = ctx
        .lookup_variables(&ident.name)
        .ok_or_else(|| {
            let error = GlslError::undefined_variable(&ident.name)
                .with_location(source_span_to_location(&span));
            ctx.add_span_to_error(error, &span)
        })?
        .to_vec(); // Clone to avoid borrow issues

    // Ensure we're in the correct block before reading variables
    // This is important when reading variables in merge blocks after control flow
    ctx.ensure_block()?;
    ctx.builder.ensure_inserted_block();

    // Read all component values fresh in the current block context
    // This ensures we get the correct SSA values for the current block
    let vals: Vec<Value> = vars
        .iter()
        .map(|&v| {
            // Force a fresh read of the variable in the current block
            ctx.builder.use_var(v)
        })
        .collect();

    Ok((vals, ty))
}

/// Emit variable expression as RValue
///
/// Reads a variable by resolving it as an LValue, then loading its value.
pub fn emit_variable_rvalue<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    expr: &Expr,
) -> Result<RValue, GlslError> {
    emit_lvalue_as_rvalue(ctx, expr)
}
