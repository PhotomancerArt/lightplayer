//! Statement code generation.

use alloc::vec::Vec;

mod declaration;
mod expr_stmt;
mod if_stmt;
mod iteration;
mod loop_do_while;
mod loop_for;
mod loop_while;
mod return_;

use wasm_encoder::Function;

use crate::codegen::context::WasmCodegenContext;
use crate::options::WasmOptions;
use hashbrown::HashMap;
use lp_glsl_builtin_ids::BuiltinId;
use lp_glsl_frontend::FloatMode;
use lp_glsl_frontend::error::GlslDiagnostics;
use lp_glsl_frontend::semantic::TypedFunction;
use lp_glsl_frontend::semantic::const_eval::ConstValue;
use lp_glsl_frontend::semantic::functions::Parameter;
use lp_glsl_frontend::semantic::types::Type;

pub use declaration::{allocate_local_from_decl, emit_declaration};
pub use expr_stmt::emit_expr_stmt;
pub use return_::emit_return;

/// Pre-reserved locals for `WasmCodegenContext::alloc_*` (must exist before `Function::new`).
///
/// **Temporary cap:** large fixed pools (e.g. 16k×2) make every function call pay for tens of
/// thousands of WASM locals — catastrophic for per-pixel `main()` on the web demo. Replace with
/// exact high-water sizing (second pass or post-pass trim) soon; see wasm bump-locals plan.
const WASM_SCRATCH_F32_POOL: u32 = 1024;
const WASM_SCRATCH_I32_POOL: u32 = 1024;
const WASM_SCRATCH_I64_POOL: u32 = 32;

/// Emit a complete function body.
pub fn emit_function(
    func: &TypedFunction,
    options: &WasmOptions,
    func_index_map: &hashbrown::HashMap<alloc::string::String, u32>,
    builtin_func_index: &HashMap<BuiltinId, u32>,
    func_return_type: &hashbrown::HashMap<
        alloc::string::String,
        lp_glsl_frontend::semantic::types::Type,
    >,
    all_user_fn_params: &hashbrown::HashMap<alloc::string::String, Vec<Parameter>>,
    global_constants: &hashbrown::HashMap<alloc::string::String, ConstValue>,
) -> Result<Function, GlslDiagnostics> {
    let mut ctx = WasmCodegenContext::new(
        &func.parameters,
        options,
        func_index_map,
        builtin_func_index,
        func_return_type,
        all_user_fn_params,
        global_constants,
    );

    // First pass: allocate locals for declarations
    for stmt in &func.body {
        walk_for_declarations(&mut ctx, stmt);
    }

    // Pre-allocate temps for vector constructors (broadcast, vector conversion)
    ctx.broadcast_temp_f32 = Some(ctx.next_local_idx);
    ctx.local_types.push(wasm_encoder::ValType::F32);
    ctx.next_local_idx += 1;
    ctx.broadcast_temp_i32 = Some(ctx.next_local_idx);
    ctx.local_types.push(wasm_encoder::ValType::I32);
    ctx.next_local_idx += 1;
    ctx.vector_conv_i32_base = Some(ctx.next_local_idx);
    for _ in 0..4 {
        ctx.local_types.push(wasm_encoder::ValType::I32);
        ctx.next_local_idx += 1;
    }
    let mm0 = ctx.next_local_idx;
    ctx.local_types.push(wasm_encoder::ValType::I32);
    ctx.next_local_idx += 1;
    let mm1 = ctx.next_local_idx;
    ctx.local_types.push(wasm_encoder::ValType::I32);
    ctx.next_local_idx += 1;
    ctx.minmax_scratch_i32 = Some((mm0, mm1));

    if options.float_mode == FloatMode::Q32 {
        let ma = ctx.next_local_idx;
        ctx.local_types.push(wasm_encoder::ValType::I32);
        ctx.next_local_idx += 1;
        let mb = ctx.next_local_idx;
        ctx.local_types.push(wasm_encoder::ValType::I32);
        ctx.next_local_idx += 1;
        let mw = ctx.next_local_idx;
        ctx.local_types.push(wasm_encoder::ValType::I64);
        ctx.next_local_idx += 1;
        ctx.q32_mul_scratch = Some((ma, mb, mw));
    }

    ctx.scratch_f32_base = ctx.next_local_idx;
    for _ in 0..WASM_SCRATCH_F32_POOL {
        ctx.local_types.push(wasm_encoder::ValType::F32);
        ctx.next_local_idx += 1;
    }
    ctx.scratch_f32_next = ctx.scratch_f32_base;
    ctx.scratch_f32_end = ctx.next_local_idx;

    ctx.scratch_i32_base = ctx.next_local_idx;
    for _ in 0..WASM_SCRATCH_I32_POOL {
        ctx.local_types.push(wasm_encoder::ValType::I32);
        ctx.next_local_idx += 1;
    }
    ctx.scratch_i32_next = ctx.scratch_i32_base;
    ctx.scratch_i32_end = ctx.next_local_idx;

    ctx.scratch_i64_base = ctx.next_local_idx;
    for _ in 0..WASM_SCRATCH_I64_POOL {
        ctx.local_types.push(wasm_encoder::ValType::I64);
        ctx.next_local_idx += 1;
    }
    ctx.scratch_i64_next = ctx.scratch_i64_base;
    ctx.scratch_i64_end = ctx.next_local_idx;

    let locals: alloc::vec::Vec<(u32, wasm_encoder::ValType)> =
        ctx.local_types.iter().map(|t| (1u32, t.clone())).collect();
    let mut f = Function::new(locals);
    {
        let mut instr = f.instructions();
        for stmt in &func.body {
            emit_statement_to_sink(&mut ctx, &mut instr, stmt, options, &func.return_type)?;
        }
        return_::emit_implicit_tail_return(&ctx, &mut instr, func)?;
        instr.end();
    }

    Ok(f)
}

pub(crate) fn walk_for_declarations(ctx: &mut WasmCodegenContext, stmt: &glsl::syntax::Statement) {
    match stmt {
        glsl::syntax::Statement::Simple(simple) => match &**simple {
            glsl::syntax::SimpleStatement::Declaration(decl) => {
                declaration::allocate_local_from_decl(ctx, decl);
            }
            glsl::syntax::SimpleStatement::Selection(sel) => match &sel.rest {
                glsl::syntax::SelectionRestStatement::Statement(s) => {
                    walk_for_declarations(ctx, s);
                }
                glsl::syntax::SelectionRestStatement::Else(then_s, else_s) => {
                    walk_for_declarations(ctx, then_s);
                    walk_for_declarations(ctx, else_s);
                }
            },
            glsl::syntax::SimpleStatement::Iteration(iter) => match iter {
                glsl::syntax::IterationStatement::While(_, body) => {
                    walk_for_declarations(ctx, body);
                }
                glsl::syntax::IterationStatement::DoWhile(body, _) => {
                    walk_for_declarations(ctx, body);
                }
                glsl::syntax::IterationStatement::For(init, _, body) => {
                    if let glsl::syntax::ForInitStatement::Declaration(decl) = init {
                        declaration::allocate_local_from_decl(ctx, decl);
                    }
                    walk_for_declarations(ctx, body);
                }
            },
            _ => {}
        },
        glsl::syntax::Statement::Compound(compound) => {
            for s in &compound.statement_list {
                walk_for_declarations(ctx, s);
            }
        }
    }
}

fn emit_statement_to_sink(
    ctx: &mut WasmCodegenContext,
    instr: &mut wasm_encoder::InstructionSink,
    stmt: &glsl::syntax::Statement,
    options: &WasmOptions,
    return_type: &Type,
) -> Result<(), GlslDiagnostics> {
    match stmt {
        glsl::syntax::Statement::Simple(simple) => {
            emit_simple_statement_to_sink(ctx, instr, &**simple, options, return_type)?;
        }
        glsl::syntax::Statement::Compound(compound) => {
            for s in &compound.statement_list {
                emit_statement_to_sink(ctx, instr, s, options, return_type)?;
            }
        }
    }
    Ok(())
}

fn emit_simple_statement_to_sink(
    ctx: &mut WasmCodegenContext,
    instr: &mut wasm_encoder::InstructionSink,
    simple: &glsl::syntax::SimpleStatement,
    options: &WasmOptions,
    return_type: &Type,
) -> Result<(), GlslDiagnostics> {
    match simple {
        glsl::syntax::SimpleStatement::Declaration(decl) => {
            declaration::emit_declaration_to_sink(ctx, instr, decl, options)?;
        }
        glsl::syntax::SimpleStatement::Expression(Some(expr)) => {
            expr_stmt::emit_expr_stmt_to_sink(ctx, instr, expr, options)?;
        }
        glsl::syntax::SimpleStatement::Jump(jump) => {
            emit_jump_to_sink(ctx, instr, jump, options, return_type)?;
        }
        glsl::syntax::SimpleStatement::Expression(None) => {}
        glsl::syntax::SimpleStatement::Selection(sel) => {
            if_stmt::emit_if_stmt_to_sink(ctx, instr, &sel.cond, &sel.rest, options, return_type)?;
        }
        glsl::syntax::SimpleStatement::Iteration(iter) => match iter {
            glsl::syntax::IterationStatement::While(cond, body) => {
                loop_while::emit_while_loop_to_sink(ctx, instr, cond, body, options, return_type)?;
            }
            glsl::syntax::IterationStatement::DoWhile(body, cond) => {
                loop_do_while::emit_do_while_loop_to_sink(
                    ctx,
                    instr,
                    body,
                    cond,
                    options,
                    return_type,
                )?;
            }
            glsl::syntax::IterationStatement::For(init, rest, body) => {
                loop_for::emit_for_loop_to_sink(
                    ctx,
                    instr,
                    init,
                    rest,
                    body,
                    options,
                    return_type,
                )?;
            }
        },
        glsl::syntax::SimpleStatement::Switch(_) | glsl::syntax::SimpleStatement::CaseLabel(_) => {
            return Err(lp_glsl_frontend::error::GlslError::new(
                lp_glsl_frontend::error::ErrorCode::E0400,
                alloc::format!("statement {:?} not supported", simple),
            )
            .into());
        }
    }
    Ok(())
}

fn emit_jump_to_sink(
    ctx: &mut WasmCodegenContext,
    instr: &mut wasm_encoder::InstructionSink,
    jump: &glsl::syntax::JumpStatement,
    options: &WasmOptions,
    return_type: &Type,
) -> Result<(), GlslDiagnostics> {
    match jump {
        glsl::syntax::JumpStatement::Return(Some(expr)) => {
            return_::emit_return_to_sink(ctx, instr, expr, options, return_type)?;
        }
        glsl::syntax::JumpStatement::Return(None) => {
            return_::emit_fn_out_writebacks(ctx, instr);
            instr.return_();
        }
        glsl::syntax::JumpStatement::Break => {
            let target = ctx
                .loop_stack
                .last()
                .ok_or_else(|| {
                    GlslDiagnostics::from(lp_glsl_frontend::error::GlslError::new(
                        lp_glsl_frontend::error::ErrorCode::E0400,
                        "break outside loop",
                    ))
                })?
                .break_target_block_depth;
            instr.br(ctx.block_depth.saturating_sub(target));
        }
        glsl::syntax::JumpStatement::Continue => {
            let loop_d = ctx
                .loop_stack
                .last()
                .ok_or_else(|| {
                    GlslDiagnostics::from(lp_glsl_frontend::error::GlslError::new(
                        lp_glsl_frontend::error::ErrorCode::E0400,
                        "continue outside loop",
                    ))
                })?
                .loop_block_depth;
            instr.br(ctx.block_depth.saturating_sub(loop_d));
        }
        glsl::syntax::JumpStatement::Discard => {
            return Err(lp_glsl_frontend::error::GlslError::new(
                lp_glsl_frontend::error::ErrorCode::E0400,
                "discard not supported in WASM backend",
            )
            .into());
        }
    }
    Ok(())
}
