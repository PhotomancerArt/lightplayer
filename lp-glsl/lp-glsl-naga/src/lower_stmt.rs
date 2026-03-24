//! Naga statements / blocks → LPIR op stream.

use alloc::format;
use alloc::string::String;

use alloc::vec::Vec;

use lpir::Op;
use naga::{Block, Expression, Handle, LocalVariable, Statement, SwitchValue};

use crate::lower_ctx::{LowerCtx, naga_type_to_ir_type};
use crate::lower_error::LowerError;

pub(crate) fn lower_block(ctx: &mut LowerCtx<'_>, block: &Block) -> Result<(), LowerError> {
    for stmt in block.iter() {
        lower_statement(ctx, stmt)?;
    }
    Ok(())
}

fn lower_statement(ctx: &mut LowerCtx<'_>, stmt: &Statement) -> Result<(), LowerError> {
    match stmt {
        Statement::Emit(_) => Ok(()),
        Statement::Block(inner) => lower_block(ctx, inner),
        Statement::If {
            condition,
            accept,
            reject,
        } => {
            let cond = ctx.ensure_expr(*condition)?;
            ctx.fb.push_if(cond);
            lower_block(ctx, accept)?;
            if !reject.is_empty() {
                ctx.fb.push_else();
                lower_block(ctx, reject)?;
            }
            ctx.fb.end_if();
            Ok(())
        }
        Statement::Loop {
            body,
            continuing,
            break_if,
        } => {
            ctx.fb.push_loop();
            lower_block(ctx, body)?;
            // Naga runs `body` then `continuing` each iteration; LPIR `Continue` jumps here.
            ctx.fb.push_continuing();
            lower_block(ctx, continuing)?;
            if let Some(cond) = break_if {
                let c = ctx.ensure_expr(*cond)?;
                let neg = ctx.fb.alloc_vreg(lpir::IrType::I32);
                ctx.fb.push(Op::IeqImm {
                    dst: neg,
                    src: c,
                    imm: 0,
                });
                ctx.fb.push(Op::BrIfNot { cond: neg });
            }
            ctx.fb.end_loop();
            Ok(())
        }
        Statement::Break => {
            ctx.fb.push(Op::Break);
            Ok(())
        }
        Statement::Continue => {
            ctx.fb.push(Op::Continue);
            Ok(())
        }
        Statement::Return { value } => match value {
            Some(expr) => {
                let v = ctx.ensure_expr(*expr)?;
                ctx.fb.push_return(&[v]);
                Ok(())
            }
            None => {
                ctx.fb.push_return(&[]);
                Ok(())
            }
        },
        Statement::Store { pointer, value } => {
            let lv = store_pointer_local(ctx.func, *pointer)?;
            let dst = ctx.resolve_local(lv)?;
            let src = ctx.ensure_expr(*value)?;
            ctx.fb.push(Op::Copy { dst, src });
            Ok(())
        }
        Statement::Switch { selector, cases } => {
            let sel = ctx.ensure_expr(*selector)?;
            ctx.fb.push_switch(sel);
            for case in cases {
                match case.value {
                    SwitchValue::Default => ctx.fb.push_default(),
                    SwitchValue::I32(v) => ctx.fb.push_case(v),
                    SwitchValue::U32(v) => ctx.fb.push_case(v as i32),
                }
                lower_block(ctx, &case.body)?;
                if !case.fall_through {
                    ctx.fb.end_switch_arm();
                }
            }
            ctx.fb.end_switch();
            Ok(())
        }
        Statement::Call {
            function,
            arguments,
            result,
        } => lower_user_call(ctx, *function, arguments, *result),
        Statement::Kill
        | Statement::ControlBarrier(_)
        | Statement::MemoryBarrier(_)
        | Statement::ImageStore { .. }
        | Statement::Atomic { .. }
        | Statement::ImageAtomic { .. }
        | Statement::WorkGroupUniformLoad { .. }
        | Statement::RayQuery { .. }
        | Statement::RayPipelineFunction(_)
        | Statement::SubgroupBallot { .. }
        | Statement::SubgroupGather { .. }
        | Statement::SubgroupCollectiveOperation { .. }
        | Statement::CooperativeStore { .. } => {
            Err(LowerError::UnsupportedStatement(format!("{stmt:?}")))
        }
    }
}

fn store_pointer_local(
    func: &naga::Function,
    expr: Handle<Expression>,
) -> Result<Handle<LocalVariable>, LowerError> {
    match &func.expressions[expr] {
        Expression::LocalVariable(lv) => Ok(*lv),
        _ => Err(LowerError::UnsupportedStatement(String::from(
            "store to non-local pointer",
        ))),
    }
}

/// `true` if this block does not end with an explicit `return`.
pub(crate) fn void_block_missing_return(block: &Block) -> bool {
    !matches!(block.last(), Some(Statement::Return { .. }))
}

fn lower_user_call(
    ctx: &mut LowerCtx<'_>,
    callee: Handle<naga::Function>,
    arguments: &[Handle<Expression>],
    result: Option<Handle<Expression>>,
) -> Result<(), LowerError> {
    let f = &ctx.module.functions[callee];
    let name = f.name.as_deref().unwrap_or("");
    if name.starts_with("lpfx_") {
        return Err(LowerError::UnsupportedStatement(String::from(
            "LPFX call (phase 6)",
        )));
    }
    if f.body.is_empty() {
        if result.is_some() {
            return Err(LowerError::Internal(String::from(
                "call to empty-bodied function with result",
            )));
        }
        return Ok(());
    }
    let callee_ref = ctx
        .func_map
        .get(&callee)
        .copied()
        .ok_or_else(|| LowerError::Internal(format!("callee not in export map: {name:?}")))?;
    let mut arg_vs = Vec::new();
    for a in arguments {
        arg_vs.push(ctx.ensure_expr(*a)?);
    }
    let mut result_vs = Vec::new();
    if let Some(res_h) = result {
        let res_ty = f
            .result
            .as_ref()
            .ok_or_else(|| LowerError::Internal(String::from("call result for void function")))?;
        let inner = &ctx.module.types[res_ty.ty].inner;
        let ir_ty = naga_type_to_ir_type(inner)?;
        let v = ctx.fb.alloc_vreg(ir_ty);
        if let Some(slot) = ctx.expr_cache.get_mut(res_h.index()) {
            *slot = Some(v);
        }
        result_vs.push(v);
    }
    ctx.fb.push_call(callee_ref, &arg_vs, &result_vs);
    Ok(())
}
