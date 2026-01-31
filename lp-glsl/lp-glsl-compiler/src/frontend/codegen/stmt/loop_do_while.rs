use glsl::syntax::{Expr, Statement};

use crate::error::GlslError;
use crate::frontend::codegen::context::CodegenContext;

/// Emit do-while loop statement
pub fn emit_loop_do_while_stmt<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    body: &Statement,
    condition: &Expr,
) -> Result<(), GlslError> {
    // Create blocks (don't seal yet)
    let body_block = ctx.builder.create_block();
    let cond_block = ctx.builder.create_block();
    let exit_block = ctx.builder.create_block();

    ctx.loop_stack
        .push(crate::frontend::codegen::context::LoopContext {
            continue_target: cond_block,
            exit_block,
        });

    // Branch directly to body (do-while always executes once)
    ctx.emit_branch(body_block)?;

    // Body: switch to but don't seal yet - will receive back edge from cond_block
    ctx.switch_to_block(body_block);
    ctx.enter_scope(); // Enter scope for body variables
    ctx.emit_statement(body)?;
    ctx.exit_scope(); // Exit scope for body variables

    // Create jump to cond_block. The jump instruction internally declares the predecessor
    // via FunctionBuilder's declare_successor method, matching the test case pattern.
    // We capture the instruction to ensure it's created before switching blocks.
    // Note: This declares the normal-flow predecessor. Continue statements processed
    // in the body above may have already declared additional predecessors to cond_block.
    let _jump_to_cond = ctx.emit_branch(cond_block)?;

    // Condition: switch to cond_block and seal it AFTER all predecessors are declared.
    // The body statement above may have contained continue statements that branch to
    // cond_block, so all predecessors should now be declared.
    // Sealing before using variables is critical for correct SSA construction.
    // - For single-predecessor blocks, Cranelift can optimize by using values directly
    // - For multiple-predecessor blocks (e.g., with continue statements), Cranelift
    //   will create block parameters to merge values from all predecessors
    ctx.switch_to_block(cond_block);
    ctx.seal_block(cond_block);

    // Now translate the condition expression, which may use variables from body_block
    // or from continue statements. Since cond_block is sealed with all predecessors
    // declared, Cranelift will correctly handle SSA construction whether there's
    // one or multiple predecessors.
    let condition_value = ctx.emit_expr(condition)?;
    // This brif creates the back edge to body_block
    ctx.emit_cond_branch(condition_value, body_block, exit_block)?;

    // Seal body_block after cond_block is sealed and has appended arguments
    // to body_block's jump. body_block can now be sealed because:
    // - The back edge from cond_block has been declared (via emit_cond_branch above)
    // - cond_block has been sealed and appended arguments to body_block's jump
    ctx.seal_block(body_block);

    // Exit - seal immediately since all predecessors are known
    ctx.emit_block(exit_block);

    ctx.loop_stack.pop();

    Ok(())
}
