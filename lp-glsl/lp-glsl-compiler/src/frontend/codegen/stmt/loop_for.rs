use glsl::syntax::{ForInitStatement, ForRestStatement, Statement};

use crate::error::GlslError;
use crate::frontend::codegen::context::CodegenContext;
use crate::frontend::codegen::stmt::loops::emit_condition;
use cranelift_codegen::ir::InstBuilder;

/// Emit for loop statement
pub fn emit_loop_for_stmt<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    init: &ForInitStatement,
    rest: &ForRestStatement,
    body: &Statement,
) -> Result<(), GlslError> {
    // Create blocks: header, body, update (for continue), exit
    let header_block = ctx.builder.create_block();
    let body_block = ctx.builder.create_block();
    let update_block = ctx.builder.create_block();
    let exit_block = ctx.builder.create_block();

    // For loops: continue should jump to update block, not header
    ctx.loop_stack
        .push(crate::frontend::codegen::context::LoopContext {
            continue_target: update_block,
            exit_block,
        });

    // Enter scope for loop variables
    ctx.enter_scope();

    // Translate init (now inside loop scope)
    match init {
        glsl::syntax::ForInitStatement::Expression(Some(expr)) => {
            ctx.emit_expr(expr)?;
        }
        glsl::syntax::ForInitStatement::Declaration(decl) => {
            ctx.emit_declaration(decl)?;
        }
        glsl::syntax::ForInitStatement::Expression(None) => {
            // Empty init
        }
    }

    ctx.emit_branch(header_block)?;

    // Header: evaluate condition
    // Don't seal header yet - it will receive a back edge from update block
    ctx.switch_to_block(header_block);
    let condition_value = if let Some(condition) = &rest.condition {
        emit_condition(ctx, condition)?
    } else {
        // No condition means infinite loop (while(true))
        ctx.builder
            .ins()
            .iconst(cranelift_codegen::ir::types::I8, 1)
    };
    ctx.emit_cond_branch(condition_value, body_block, exit_block)?;

    // Body
    ctx.emit_block(body_block);
    ctx.emit_statement(body)?;
    ctx.emit_branch(update_block)?;

    // Update block
    ctx.emit_block(update_block);
    if let Some(update_expr) = &rest.post_expr {
        ctx.emit_expr(update_expr)?;
    }
    ctx.emit_branch(header_block)?;

    // Now seal header block - all predecessors (initial jump + back edge from update) are known
    ctx.seal_block(header_block);

    // Exit - seal immediately since all predecessors are known
    ctx.emit_block(exit_block);

    ctx.loop_stack.pop();

    // Exit scope for loop variables
    ctx.exit_scope();

    Ok(())
}
