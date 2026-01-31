use glsl::syntax::SelectionStatement;

use crate::error::{ErrorCode, GlslError};
use crate::frontend::codegen::context::CodegenContext;

use alloc::format;

/// Emit if statement (selection statement)
pub fn emit_if_stmt<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    selection: &SelectionStatement,
) -> Result<(), GlslError> {
    use crate::error::{extract_span_from_expr, source_span_to_location};
    use glsl::syntax::SelectionRestStatement;

    // Translate condition and validate type
    let (cond_vals, cond_ty) = ctx.emit_expr_typed(&selection.cond)?;
    let cond_span = extract_span_from_expr(&selection.cond);
    // Validate that condition is bool type (GLSL spec requirement)
    if cond_ty != crate::frontend::semantic::types::Type::Bool {
        let error = GlslError::new(ErrorCode::E0107, "condition must be bool type")
            .with_location(source_span_to_location(&cond_span))
            .with_note(format!("condition has type `{cond_ty:?}`, expected `Bool`"));
        return Err(ctx.add_span_to_error(error, &cond_span));
    }
    // Condition must be scalar, so we take the first (and only) value
    let condition_value = cond_vals.into_iter().next().ok_or_else(|| {
        let error = GlslError::new(ErrorCode::E0400, "condition expression produced no value")
            .with_location(source_span_to_location(&cond_span));
        ctx.add_span_to_error(error, &cond_span)
    })?;

    // 1. Create blocks
    let then_block = ctx.builder.create_block();
    let merge_block = ctx.builder.create_block();

    match &selection.rest {
        SelectionRestStatement::Statement(then_stmt) => {
            // No else: else_block is same as merge_block
            let else_block = merge_block;

            // 2. Evaluate condition in current block and branch
            ctx.emit_cond_branch(condition_value, then_block, else_block)?;

            // 3. Emit then block
            ctx.emit_block(then_block);
            ctx.enter_scope(); // Enter scope for then block
            ctx.emit_statement(then_stmt)?;
            ctx.exit_scope(); // Exit scope for then block
            ctx.emit_branch(merge_block)?;

            // 4. Emit continuation block
            ctx.emit_block(merge_block);
        }
        SelectionRestStatement::Else(then_stmt, else_stmt) => {
            let else_block = ctx.builder.create_block();

            // 2. Evaluate condition in current block and branch
            ctx.emit_cond_branch(condition_value, then_block, else_block)?;

            // 3. Emit then block
            ctx.emit_block(then_block);
            ctx.enter_scope(); // Enter scope for then block
            ctx.emit_statement(then_stmt)?;
            ctx.exit_scope(); // Exit scope for then block
            ctx.emit_branch(merge_block)?;

            // 4. Emit else block
            ctx.emit_block(else_block);
            ctx.enter_scope(); // Enter scope for else block
            ctx.emit_statement(else_stmt)?;
            ctx.exit_scope(); // Exit scope for else block
            ctx.emit_branch(merge_block)?;

            // 5. Emit continuation block
            ctx.emit_block(merge_block);
        }
    }

    Ok(())
}
