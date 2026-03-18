//! If/else statement code generation.

use glsl::syntax::SelectionRestStatement;

use crate::codegen::context::WasmCodegenContext;
use crate::codegen::expr;
use crate::options::WasmOptions;
use lp_glsl_frontend::error::GlslDiagnostics;
use lp_glsl_frontend::semantic::types::Type;
use wasm_encoder::{BlockType, InstructionSink};

/// Emit if/else statement. Condition must evaluate to bool (i32: 0 = false, else = true).
pub fn emit_if_stmt_to_sink(
    ctx: &mut WasmCodegenContext,
    instr: &mut InstructionSink,
    cond: &glsl::syntax::Expr,
    rest: &SelectionRestStatement,
    options: &WasmOptions,
    return_type: &Type,
) -> Result<(), GlslDiagnostics> {
    expr::emit_rvalue(ctx, instr, cond, options)?;
    instr.if_(BlockType::Empty);
    ctx.block_depth += 1;
    let (then_returns, else_returns) = match rest {
        SelectionRestStatement::Statement(then_stmt) => {
            super::emit_statement_to_sink(ctx, instr, then_stmt, options, return_type)?;
            instr.else_();
            (stmt_always_returns(then_stmt), false)
        }
        SelectionRestStatement::Else(then_stmt, else_stmt) => {
            super::emit_statement_to_sink(ctx, instr, then_stmt, options, return_type)?;
            instr.else_();
            let else_ret = stmt_always_returns(else_stmt);
            super::emit_statement_to_sink(ctx, instr, else_stmt, options, return_type)?;
            (stmt_always_returns(then_stmt), else_ret)
        }
    };
    instr.end();
    ctx.block_depth -= 1;
    if !matches!(return_type, Type::Void) && then_returns && else_returns {
        instr.unreachable();
    }
    Ok(())
}

fn stmt_always_returns(stmt: &glsl::syntax::Statement) -> bool {
    match stmt {
        glsl::syntax::Statement::Simple(s) => matches!(
            &**s,
            glsl::syntax::SimpleStatement::Jump(glsl::syntax::JumpStatement::Return(_))
        ),
        glsl::syntax::Statement::Compound(c) => {
            c.statement_list.last().map_or(false, stmt_always_returns)
        }
    }
}
