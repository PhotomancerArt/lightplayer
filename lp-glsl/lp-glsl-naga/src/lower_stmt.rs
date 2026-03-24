//! Naga statements / blocks → LPIR op stream (scalar and multi-value store/return/call).

use alloc::format;
use alloc::string::String;

use alloc::vec::Vec;

use lpir::{IrType, Op, SlotId};
use naga::{Block, Expression, Handle, LocalVariable, Statement, SwitchValue, TypeInner};

use crate::lower_ctx::{LowerCtx, VRegVec, naga_type_to_ir_types};
use crate::lower_error::LowerError;
use crate::lower_expr::coerce_assignment_vregs;
use crate::lower_lpfx;

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
                let mut vs = ctx.ensure_expr_vec(*expr)?;
                if let Some(res) = &ctx.func.result {
                    let dst_inner = &ctx.module.types[res.ty].inner;
                    vs = coerce_assignment_vregs(ctx, dst_inner, *expr, vs)?;
                }
                ctx.fb.push_return(&vs);
                Ok(())
            }
            None => {
                ctx.fb.push_return(&[]);
                Ok(())
            }
        },
        Statement::Store { pointer, value } => match &ctx.func.expressions[*pointer] {
            Expression::LocalVariable(lv) => {
                let dsts = ctx.resolve_local(*lv)?;
                let dst_inner = &ctx.module.types[ctx.func.local_variables[*lv].ty].inner;
                let raw = ctx.ensure_expr_vec(*value)?;
                let srcs = coerce_assignment_vregs(ctx, dst_inner, *value, raw)?;
                if dsts.len() != srcs.len() {
                    return Err(LowerError::UnsupportedStatement(format!(
                        "Store component mismatch {} vs {}",
                        dsts.len(),
                        srcs.len()
                    )));
                }
                for (d, s) in dsts.iter().zip(srcs.iter()) {
                    ctx.fb.push(Op::Copy { dst: *d, src: *s });
                }
                Ok(())
            }
            Expression::FunctionArgument(i) if ctx.pointer_args.contains_key(i) => {
                let base_ty_h = ctx.pointer_args[i];
                let base_inner = &ctx.module.types[base_ty_h].inner;
                let ir_tys = naga_type_to_ir_types(base_inner)?;
                let addr = ctx.arg_vregs_for(*i)?[0];
                let srcs = ctx.ensure_expr_vec(*value)?;
                if ir_tys.len() != srcs.len() {
                    return Err(LowerError::UnsupportedStatement(format!(
                        "Store to inout pointer: {} vs {} components",
                        ir_tys.len(),
                        srcs.len()
                    )));
                }
                for (j, src) in srcs.iter().enumerate() {
                    ctx.fb.push(Op::Store {
                        base: addr,
                        offset: (j * 4) as u32,
                        value: *src,
                    });
                }
                Ok(())
            }
            _ => Err(LowerError::UnsupportedStatement(String::from(
                "store to non-local pointer",
            ))),
        },
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

fn call_arg_pointer_local(
    func: &naga::Function,
    expr: Handle<Expression>,
) -> Result<Handle<LocalVariable>, LowerError> {
    match &func.expressions[expr] {
        Expression::LocalVariable(lv) => Ok(*lv),
        _ => Err(LowerError::UnsupportedExpression(String::from(
            "inout/out call argument must be a local variable",
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
        return lower_lpfx::lower_lpfx_call(ctx, callee, arguments, result);
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
    let mut inout_copybacks: Vec<(Handle<LocalVariable>, SlotId)> = Vec::new();
    for (i, &arg_h) in arguments.iter().enumerate() {
        let callee_arg = &f.arguments[i];
        let callee_inner = &ctx.module.types[callee_arg.ty].inner;
        if let TypeInner::Pointer { base, .. } = callee_inner {
            let lv = call_arg_pointer_local(ctx.func, arg_h)?;
            let local_vregs = ctx.resolve_local(lv)?;
            let base_inner = &ctx.module.types[*base].inner;
            let ir_tys = naga_type_to_ir_types(base_inner)?;
            let slot = ctx.fb.alloc_slot(ir_tys.len() as u32 * 4);
            let addr = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(Op::SlotAddr { dst: addr, slot });
            for (j, &src) in local_vregs.iter().enumerate() {
                ctx.fb.push(Op::Store {
                    base: addr,
                    offset: (j * 4) as u32,
                    value: src,
                });
            }
            arg_vs.push(addr);
            inout_copybacks.push((lv, slot));
        } else {
            let vs = ctx.ensure_expr_vec(arg_h)?;
            arg_vs.extend_from_slice(&vs);
        }
    }
    let mut result_vs = Vec::new();
    if let Some(res_h) = result {
        let res_ty = f
            .result
            .as_ref()
            .ok_or_else(|| LowerError::Internal(String::from("call result for void function")))?;
        let inner = &ctx.module.types[res_ty.ty].inner;
        let ir_tys = naga_type_to_ir_types(inner)?;
        let mut vregs = VRegVec::new();
        for ty in &ir_tys {
            let v = ctx.fb.alloc_vreg(*ty);
            vregs.push(v);
            result_vs.push(v);
        }
        if let Some(slot) = ctx.expr_cache.get_mut(res_h.index()) {
            *slot = Some(vregs);
        }
    }
    ctx.fb.push_call(callee_ref, &arg_vs, &result_vs);
    for (lv, slot) in &inout_copybacks {
        let local_vregs = ctx.resolve_local(*lv)?;
        let addr = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(Op::SlotAddr {
            dst: addr,
            slot: *slot,
        });
        for (j, dst_v) in local_vregs.iter().enumerate() {
            ctx.fb.push(Op::Load {
                dst: *dst_v,
                base: addr,
                offset: (j * 4) as u32,
            });
        }
    }
    Ok(())
}
