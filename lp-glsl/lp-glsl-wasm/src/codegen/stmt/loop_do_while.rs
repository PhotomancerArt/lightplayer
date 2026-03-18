//! Do-while loop code generation.

use wasm_encoder::{BlockType, InstructionSink};

use crate::codegen::context::{LoopContext, WasmCodegenContext};
use crate::codegen::expr;
use crate::options::WasmOptions;
use lp_glsl_frontend::error::GlslDiagnostics;
use lp_glsl_frontend::semantic::types::Type;

/// Emit do body while(cond).
/// Pattern: block $break { loop $loop { block $continue { body } cond; eqz; br_if $break; br $loop } }
/// The continue block ensures `continue` jumps to condition (end of block) instead of restarting body.
pub fn emit_do_while_loop_to_sink(
    ctx: &mut WasmCodegenContext,
    instr: &mut InstructionSink,
    body: &glsl::syntax::Statement,
    cond: &glsl::syntax::Expr,
    options: &WasmOptions,
    return_type: &Type,
) -> Result<(), GlslDiagnostics> {
    instr.block(BlockType::Empty);
    ctx.block_depth += 1;
    let break_target = ctx.block_depth;
    instr.loop_(BlockType::Empty);
    ctx.block_depth += 1;
    instr.block(BlockType::Empty);
    ctx.block_depth += 1;
    let continue_target = ctx.block_depth;
    ctx.loop_stack.push(LoopContext {
        break_target_block_depth: break_target,
        loop_block_depth: continue_target,
        continue_depth: 0,
    });

    super::emit_statement_to_sink(ctx, instr, body, options, return_type)?;
    instr.end();
    ctx.block_depth -= 1;
    expr::emit_rvalue(ctx, instr, cond, options)?;
    instr.i32_eqz();
    instr.br_if(1);
    instr.br(0);

    instr.end();
    ctx.block_depth -= 1;
    instr.end();
    ctx.block_depth -= 1;

    ctx.loop_stack.pop();
    Ok(())
}
