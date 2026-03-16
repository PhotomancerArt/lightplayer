//! Statement code generation.

mod declaration;
mod expr_stmt;
mod return_;

use wasm_encoder::Function;

use crate::codegen::context::WasmCodegenContext;
use crate::options::WasmOptions;
use lp_glsl_frontend::error::GlslDiagnostics;
use lp_glsl_frontend::semantic::TypedFunction;
use lp_glsl_frontend::semantic::types::Type;

pub use declaration::{allocate_local_from_decl, emit_declaration};
pub use expr_stmt::emit_expr_stmt;
pub use return_::emit_return;

/// Emit a complete function body.
pub fn emit_function(
    func: &TypedFunction,
    options: &WasmOptions,
) -> Result<Function, GlslDiagnostics> {
    let mut ctx = WasmCodegenContext::new(&func.parameters, options);

    // First pass: allocate locals for declarations
    for stmt in &func.body {
        walk_for_declarations(&mut ctx, stmt);
    }

    let locals: alloc::vec::Vec<(u32, wasm_encoder::ValType)> =
        ctx.local_types.iter().map(|t| (1u32, t.clone())).collect();
    let mut f = Function::new(locals);
    {
        let mut instr = f.instructions();
        for stmt in &func.body {
            emit_statement_to_sink(&mut ctx, &mut instr, stmt, options, &func.return_type)?;
        }
        instr.end();
    }

    Ok(f)
}

fn walk_for_declarations(ctx: &mut WasmCodegenContext, stmt: &glsl::syntax::Statement) {
    match stmt {
        glsl::syntax::Statement::Simple(simple) => {
            if let glsl::syntax::SimpleStatement::Declaration(decl) = &**simple {
                declaration::allocate_local_from_decl(ctx, decl);
            }
        }
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
        glsl::syntax::SimpleStatement::Expression(None)
        | glsl::syntax::SimpleStatement::Selection(_)
        | glsl::syntax::SimpleStatement::Iteration(_)
        | glsl::syntax::SimpleStatement::Switch(_)
        | glsl::syntax::SimpleStatement::CaseLabel(_) => {
            // Phase ii: not implemented
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
            instr.return_();
        }
        glsl::syntax::JumpStatement::Break
        | glsl::syntax::JumpStatement::Continue
        | glsl::syntax::JumpStatement::Discard => {
            // Phase ii: not implemented
        }
    }
    Ok(())
}
