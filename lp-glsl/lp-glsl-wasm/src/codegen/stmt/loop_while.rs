//! While loop code generation.

use wasm_encoder::{BlockType, InstructionSink};

use crate::codegen::context::{LoopContext, WasmCodegenContext};
use crate::options::WasmOptions;
use lp_glsl_frontend::error::GlslDiagnostics;
use lp_glsl_frontend::semantic::types::Type;

/// Emit while(cond) body. Pattern: block { loop { cond; br_if exit; body; br loop } }
pub fn emit_while_loop_to_sink(
    ctx: &mut WasmCodegenContext,
    instr: &mut InstructionSink,
    condition: &glsl::syntax::Condition,
    body: &glsl::syntax::Statement,
    options: &WasmOptions,
    return_type: &Type,
) -> Result<(), GlslDiagnostics> {
    instr.block(BlockType::Empty);
    ctx.block_depth += 1;
    let break_target = ctx.block_depth;
    instr.loop_(BlockType::Empty);
    ctx.block_depth += 1;
    let loop_depth = ctx.block_depth;
    ctx.loop_stack.push(LoopContext {
        break_target_block_depth: break_target,
        loop_block_depth: loop_depth,
        continue_depth: 0,
    });

    super::iteration::emit_condition_to_sink(ctx, instr, condition, options)?;
    instr.i32_eqz();
    instr.br_if(1);

    super::emit_statement_to_sink(ctx, instr, body, options, return_type)?;
    instr.br(0);

    instr.end();
    ctx.block_depth -= 1;
    instr.end();
    ctx.block_depth -= 1;

    ctx.loop_stack.pop();
    Ok(())
}
