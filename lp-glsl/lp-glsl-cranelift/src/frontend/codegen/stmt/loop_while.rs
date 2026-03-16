use glsl::syntax::{Condition, Statement};

use crate::error::GlslError;
use crate::frontend::codegen::context::CodegenContext;
use crate::frontend::codegen::stmt::loops::emit_condition;

/// Emit while loop statement
pub fn emit_loop_while_stmt<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    condition: &Condition,
    body: &Statement,
) -> Result<(), GlslError> {
    let header_block = ctx.builder.create_block();
    let body_block = ctx.builder.create_block();
    let exit_block = ctx.builder.create_block();

    // Push loop context for break/continue
    ctx.loop_stack
        .push(crate::frontend::codegen::context::LoopContext {
            continue_target: header_block,
            exit_block,
        });

    // Jump to header
    ctx.emit_branch(header_block)?;

    // Enter scope for loop body BEFORE evaluating condition
    // This ensures variables declared in the condition are in the same scope as the body
    // According to GLSL spec: "Variables declared in the condition-expression are only
    // in scope until the end of the sub-statement of the while loop"
    ctx.enter_scope();

    // Header: evaluate condition
    // Don't seal header yet - it will receive a back edge from body
    ctx.switch_to_block(header_block);
    let condition_value = emit_condition(ctx, condition)?;
    ctx.emit_cond_branch(condition_value, body_block, exit_block)?;

    // Body
    ctx.emit_block(body_block);
    ctx.emit_statement(body)?;
    ctx.emit_branch(header_block)?; // Loop back

    // Exit scope for loop body (condition variables go out of scope here)
    ctx.exit_scope();

    // Now seal header block - all predecessors (initial jump + back edge) are known
    ctx.seal_block(header_block);

    // Exit - seal immediately since all predecessors are known
    ctx.emit_block(exit_block);

    // Pop loop context
    ctx.loop_stack.pop();

    Ok(())
}
