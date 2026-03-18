//! For loop code generation.

use glsl::syntax::{ForInitStatement, ForRestStatement};

use crate::codegen::context::{LoopContext, WasmCodegenContext};
use crate::codegen::expr;
use crate::options::WasmOptions;
use lp_glsl_frontend::error::GlslDiagnostics;
use lp_glsl_frontend::semantic::types::Type;
use wasm_encoder::{BlockType, InstructionSink};

/// Emit for(init; cond; update) body.
///
/// Pattern uses a continue block so `continue` skips to update instead of
/// restarting the loop (which would skip the update and infinite-loop):
///
///   init
///   block $break {
///     loop $loop {
///       cond; br_if $break
///       block $continue {
///         body            ← continue branches to $continue end, falls through to update
///       }
///       update
///       br $loop
///     }
///   }
pub fn emit_for_loop_to_sink(
    ctx: &mut WasmCodegenContext,
    instr: &mut InstructionSink,
    init: &ForInitStatement,
    rest: &ForRestStatement,
    body: &glsl::syntax::Statement,
    options: &WasmOptions,
    return_type: &Type,
) -> Result<(), GlslDiagnostics> {
    match init {
        ForInitStatement::Expression(Some(expr)) => {
            expr::emit_rvalue(ctx, instr, expr, options)?;
            instr.drop();
        }
        ForInitStatement::Declaration(decl) => {
            super::declaration::emit_declaration_to_sink(ctx, instr, decl, options)?;
        }
        ForInitStatement::Expression(None) => {}
    }

    // block $break {
    instr.block(BlockType::Empty);
    ctx.block_depth += 1;
    let break_target = ctx.block_depth;

    //   loop $loop {
    instr.loop_(BlockType::Empty);
    ctx.block_depth += 1;

    if let Some(condition) = &rest.condition {
        super::iteration::emit_condition_to_sink(ctx, instr, condition, options)?;
        instr.i32_eqz();
        instr.br_if(1);
    }

    //     block $continue {
    instr.block(BlockType::Empty);
    ctx.block_depth += 1;
    let continue_target = ctx.block_depth;

    ctx.loop_stack.push(LoopContext {
        break_target_block_depth: break_target,
        loop_block_depth: continue_target,
        continue_depth: 0,
    });

    super::emit_statement_to_sink(ctx, instr, body, options, return_type)?;

    //     } end $continue
    instr.end();
    ctx.block_depth -= 1;

    ctx.loop_stack.pop();

    if let Some(update) = &rest.post_expr {
        expr::emit_rvalue(ctx, instr, update, options)?;
        instr.drop();
    }

    instr.br(0);

    //   } end $loop
    instr.end();
    ctx.block_depth -= 1;
    // } end $break
    instr.end();
    ctx.block_depth -= 1;

    Ok(())
}
