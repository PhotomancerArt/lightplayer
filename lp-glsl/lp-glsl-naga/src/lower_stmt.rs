//! Naga statements / blocks → LPIR op stream (scalar and multi-value store/return/call).

use alloc::format;
use alloc::string::String;

use alloc::vec::Vec;

use lpir::{IrType, Op, SlotId};
use naga::{Block, Expression, Handle, LocalVariable, Statement, SwitchValue, TypeInner};

use crate::lower_access;
use crate::lower_ctx::{LowerCtx, VRegVec, naga_type_to_ir_types};
use crate::lower_error::LowerError;
use crate::lower_expr::coerce_assignment_vregs;
use crate::lower_lpfx;
use crate::naga_util::expr_type_inner;

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

            // Naga's GLSL frontend for do-while emits `if (!cond) { break; }` as
            // the last body statement with empty continuing and no break_if. Move
            // that trailing if+break into the continuing section so that
            // `continue` (which branches past the inner body block) still reaches
            // the condition check.
            if continuing.is_empty() && break_if.is_none() && is_trailing_break_if(body) {
                let n = body.len();
                for (i, stmt) in body.iter().enumerate() {
                    if i + 1 == n {
                        break;
                    }
                    lower_statement(ctx, stmt)?;
                }
                ctx.fb.push_continuing();
                if let Some(last) = body.last() {
                    lower_statement(ctx, last)?;
                }
            } else {
                lower_block(ctx, body)?;
                ctx.fb.push_continuing();
            }

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
            Expression::Access { .. } => lower_access::store_through_access(ctx, *pointer, *value),
            // `v.x = …`: Naga uses `Store(AccessIndex(…), value)`, not `Store(LocalVariable, …)`.
            Expression::AccessIndex { .. } => {
                if let Some((lv, idxs)) =
                    crate::lower_array_multidim::peel_access_index_chain(ctx.func, *pointer)
                {
                    if let Some(info) = ctx.array_map.get(&lv).cloned() {
                        if idxs.len() == info.dimensions.len() {
                            let flat = crate::lower_array_multidim::flat_index_const_clamped(
                                &info.dimensions,
                                &idxs,
                            )?;
                            return crate::lower_array::store_array_element_const(
                                ctx, &info, flat, *value,
                            );
                        }
                    }
                }
                let Expression::AccessIndex { base, index } = &ctx.func.expressions[*pointer]
                else {
                    return Err(LowerError::Internal(String::from(
                        "AccessIndex store shape",
                    )));
                };
                match &ctx.func.expressions[*base] {
                    Expression::LocalVariable(lv) => {
                        let dsts = ctx.resolve_local(*lv)?;
                        let lv_ty = &ctx.module.types[ctx.func.local_variables[*lv].ty].inner;
                        match lv_ty {
                            TypeInner::Vector { scalar, .. } => {
                                let comp = *index as usize;
                                if comp >= dsts.len() {
                                    return Err(LowerError::UnsupportedStatement(format!(
                                        "AccessIndex {comp} out of range (len {})",
                                        dsts.len()
                                    )));
                                }
                                let scalar_inner = TypeInner::Scalar(*scalar);
                                let raw = ctx.ensure_expr_vec(*value)?;
                                let srcs =
                                    coerce_assignment_vregs(ctx, &scalar_inner, *value, raw)?;
                                if srcs.len() != 1 {
                                    return Err(LowerError::UnsupportedStatement(format!(
                                        "component store expects one scalar, got {} values",
                                        srcs.len()
                                    )));
                                }
                                ctx.fb.push(Op::Copy {
                                    dst: dsts[comp],
                                    src: srcs[0],
                                });
                                Ok(())
                            }
                            TypeInner::Matrix {
                                columns,
                                rows,
                                scalar,
                            } => {
                                let ncols = crate::lower_ctx::vector_size_usize(*columns);
                                let nrows = crate::lower_ctx::vector_size_usize(*rows);
                                let col = *index as usize;
                                if col >= ncols {
                                    return Err(LowerError::UnsupportedStatement(format!(
                                        "matrix column AccessIndex {col} out of range (cols {ncols})"
                                    )));
                                }
                                let col_ty = TypeInner::Vector {
                                    size: *rows,
                                    scalar: *scalar,
                                };
                                let raw = ctx.ensure_expr_vec(*value)?;
                                let srcs = coerce_assignment_vregs(ctx, &col_ty, *value, raw)?;
                                for r in 0..nrows {
                                    let flat_i = col * nrows + r;
                                    ctx.fb.push(Op::Copy {
                                        dst: dsts[flat_i],
                                        src: srcs[r],
                                    });
                                }
                                Ok(())
                            }
                            _ => Err(LowerError::UnsupportedStatement(String::from(
                                "AccessIndex store on non-vector non-matrix local",
                            ))),
                        }
                    }
                    Expression::FunctionArgument(arg_i) if ctx.pointer_args.contains_key(arg_i) => {
                        let store_ty = expr_type_inner(ctx.module, ctx.func, *pointer)?;
                        let dst_inner = match store_ty {
                            TypeInner::ValuePointer {
                                size: None, scalar, ..
                            } => TypeInner::Scalar(scalar),
                            TypeInner::Scalar(s) => TypeInner::Scalar(s),
                            other => {
                                return Err(LowerError::UnsupportedStatement(format!(
                                    "component store through parameter pointer: expected scalar target, got {other:?}"
                                )));
                            }
                        };
                        let addr = ctx.arg_vregs_for(*arg_i)?[0];
                        let raw = ctx.ensure_expr_vec(*value)?;
                        let srcs = coerce_assignment_vregs(ctx, &dst_inner, *value, raw)?;
                        if srcs.len() != 1 {
                            return Err(LowerError::UnsupportedStatement(format!(
                                "component store expects one scalar, got {} values",
                                srcs.len()
                            )));
                        }
                        ctx.fb.push(Op::Store {
                            base: addr,
                            offset: *index * 4,
                            value: srcs[0],
                        });
                        Ok(())
                    }
                    Expression::AccessIndex {
                        base: col_base,
                        index: col_idx,
                    } => {
                        let Expression::LocalVariable(lv) = &ctx.func.expressions[*col_base] else {
                            return Err(LowerError::UnsupportedStatement(String::from(
                                "matrix element store: expected local matrix",
                            )));
                        };
                        let lv_ty = &ctx.module.types[ctx.func.local_variables[*lv].ty].inner;
                        let TypeInner::Matrix {
                            columns,
                            rows,
                            scalar,
                        } = lv_ty
                        else {
                            return Err(LowerError::UnsupportedStatement(String::from(
                                "nested AccessIndex store base is not a matrix local",
                            )));
                        };
                        let nrows = crate::lower_ctx::vector_size_usize(*rows);
                        let ncols = crate::lower_ctx::vector_size_usize(*columns);
                        let col = *col_idx as usize;
                        let row = *index as usize;
                        if col >= ncols || row >= nrows {
                            return Err(LowerError::UnsupportedStatement(format!(
                                "matrix store index out of range col {col} row {row} (mat {ncols}x{nrows})"
                            )));
                        }
                        let flat_i = col * nrows + row;
                        let dsts = ctx.resolve_local(*lv)?;
                        if flat_i >= dsts.len() {
                            return Err(LowerError::UnsupportedStatement(format!(
                                "matrix flat index {flat_i} out of range (len {})",
                                dsts.len()
                            )));
                        }
                        let scalar_inner = TypeInner::Scalar(*scalar);
                        let raw = ctx.ensure_expr_vec(*value)?;
                        let srcs = coerce_assignment_vregs(ctx, &scalar_inner, *value, raw)?;
                        if srcs.len() != 1 {
                            return Err(LowerError::UnsupportedStatement(format!(
                                "matrix element store expects one scalar, got {} values",
                                srcs.len()
                            )));
                        }
                        ctx.fb.push(Op::Copy {
                            dst: dsts[flat_i],
                            src: srcs[0],
                        });
                        Ok(())
                    }
                    _ => Err(LowerError::UnsupportedStatement(String::from(
                        "store to non-local pointer",
                    ))),
                }
            }
            Expression::LocalVariable(lv) => {
                if let Some(dst_info) = ctx.array_map.get(lv).cloned() {
                    match &ctx.func.expressions[*value] {
                        Expression::Compose { .. } | Expression::ZeroValue(_) => {
                            return crate::lower_array::lower_array_initializer(
                                ctx, &dst_info, *value,
                            );
                        }
                        _ => {
                            let src_lv = crate::lower_array::peel_array_local_value(
                                ctx.func,
                                *value,
                            )
                            .ok_or_else(|| {
                                LowerError::UnsupportedStatement(String::from(
                                    "array assignment: rhs must be another stack array or constructor",
                                ))
                            })?;
                            let src_info =
                                ctx.array_map.get(&src_lv).cloned().ok_or_else(|| {
                                    LowerError::UnsupportedStatement(String::from(
                                        "array assignment: rhs not a stack array",
                                    ))
                                })?;
                            return crate::lower_array::copy_stack_array_slots(
                                ctx, &dst_info, &src_info,
                            );
                        }
                    }
                }
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

/// `true` when `body` ends with `if (…) { break; }` — the do-while exit pattern
/// emitted by Naga's GLSL frontend.
fn is_trailing_break_if(body: &Block) -> bool {
    matches!(
        body.last(),
        Some(Statement::If {
            accept,
            reject,
            ..
        }) if accept.len() == 1
            && matches!(accept.last(), Some(Statement::Break))
            && reject.is_empty()
    )
}
