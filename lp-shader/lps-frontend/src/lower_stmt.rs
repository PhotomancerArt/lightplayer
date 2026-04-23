//! Naga statements / blocks → LPIR op stream (scalar and multi-value store/return/call).

use alloc::format;
use alloc::string::String;

use alloc::vec::Vec;

use lpir::{LpirOp, VMCTX_VREG};
use naga::{Block, Expression, Statement, SwitchValue, TypeInner};

use crate::lower_access;
use crate::lower_array::aggregate_storage_base_vreg;
use crate::lower_call;
use crate::lower_ctx::{LowerCtx, naga_type_to_ir_types};
use crate::lower_error::LowerError;
use crate::lower_expr::coerce_assignment_vregs;
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
                ctx.fb.push(LpirOp::IeqImm {
                    dst: neg,
                    src: c,
                    imm: 0,
                });
                ctx.fb.push(LpirOp::BrIfNot { cond: neg });
            }
            ctx.fb.end_loop();
            Ok(())
        }
        Statement::Break => {
            ctx.fb.push(LpirOp::Break);
            Ok(())
        }
        Statement::Continue => {
            ctx.fb.push(LpirOp::Continue);
            Ok(())
        }
        Statement::Return { value } => match value {
            Some(expr) => {
                if let Some(sret) = ctx.sret.clone() {
                    crate::lower_call::write_aggregate_return_into_sret(ctx, *expr, &sret)?;
                    ctx.fb.push_return(&[]);
                } else {
                    let mut vs = ctx.ensure_expr_vec(*expr)?;
                    if let Some(res) = &ctx.func.result {
                        let dst_inner = &ctx.module.types[res.ty].inner;
                        vs = coerce_assignment_vregs(ctx, Some(res.ty), dst_inner, *expr, vs)?;
                    }
                    ctx.fb.push_return(&vs);
                }
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
                    if let Some(info) = ctx.aggregate_map.get(&lv).cloned() {
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
                if let Some((root, ops)) =
                    crate::lower_array_multidim::peel_array_subscript_chain(ctx.func, *pointer)
                {
                    use crate::lower_array_multidim::SubscriptOperand;
                    if ops.iter().all(|o| matches!(o, SubscriptOperand::Const(_))) {
                        if let Some(info) = ctx.aggregate_info_for_subscript_root(root)? {
                            let idxs: Vec<u32> = ops
                                .iter()
                                .map(|o| match o {
                                    SubscriptOperand::Const(c) => *c,
                                    SubscriptOperand::Dynamic(_) => 0,
                                })
                                .collect();
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
                                    coerce_assignment_vregs(ctx, None, &scalar_inner, *value, raw)?;
                                if srcs.len() != 1 {
                                    return Err(LowerError::UnsupportedStatement(format!(
                                        "component store expects one scalar, got {} values",
                                        srcs.len()
                                    )));
                                }
                                ctx.fb.push(LpirOp::Copy {
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
                                let srcs =
                                    coerce_assignment_vregs(ctx, None, &col_ty, *value, raw)?;
                                for r in 0..nrows {
                                    let flat_i = col * nrows + r;
                                    ctx.fb.push(LpirOp::Copy {
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
                        let srcs = coerce_assignment_vregs(ctx, None, &dst_inner, *value, raw)?;
                        if srcs.len() != 1 {
                            return Err(LowerError::UnsupportedStatement(format!(
                                "component store expects one scalar, got {} values",
                                srcs.len()
                            )));
                        }
                        ctx.fb.push(LpirOp::Store {
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
                        let srcs = coerce_assignment_vregs(ctx, None, &scalar_inner, *value, raw)?;
                        if srcs.len() != 1 {
                            return Err(LowerError::UnsupportedStatement(format!(
                                "matrix element store expects one scalar, got {} values",
                                srcs.len()
                            )));
                        }
                        ctx.fb.push(LpirOp::Copy {
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
                if let Some(dst_info) = ctx.aggregate_map.get(lv).cloned() {
                    match &ctx.func.expressions[*value] {
                        Expression::Compose { .. } | Expression::ZeroValue(_) => {
                            return crate::lower_array::lower_array_initializer(
                                ctx, &dst_info, *value,
                            );
                        }
                        Expression::FunctionArgument(arg_i) => {
                            let param_ptr = ctx.arg_vregs_for(*arg_i)?[0];
                            let dst = aggregate_storage_base_vreg(ctx, &dst_info.slot)?;
                            ctx.fb.push(LpirOp::Memcpy {
                                dst_addr: dst,
                                src_addr: param_ptr,
                                size: dst_info.total_size,
                            });
                            return Ok(());
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
                                ctx.aggregate_map.get(&src_lv).cloned().ok_or_else(|| {
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
                let lv_ty = ctx.func.local_variables[*lv].ty;
                let dst_inner = &ctx.module.types[lv_ty].inner;
                let raw = ctx.ensure_expr_vec(*value)?;
                let srcs = coerce_assignment_vregs(ctx, Some(lv_ty), dst_inner, *value, raw)?;
                if dsts.len() != srcs.len() {
                    return Err(LowerError::UnsupportedStatement(format!(
                        "Store component mismatch {} vs {}",
                        dsts.len(),
                        srcs.len()
                    )));
                }
                for (d, s) in dsts.iter().zip(srcs.iter()) {
                    ctx.fb.push(LpirOp::Copy { dst: *d, src: *s });
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
                    ctx.fb.push(LpirOp::Store {
                        base: addr,
                        offset: (j * 4) as u32,
                        value: *src,
                    });
                }
                Ok(())
            }
            Expression::GlobalVariable(gv_handle) => {
                // Store to a global variable.
                let info = ctx.global_map.get(gv_handle).cloned().ok_or_else(|| {
                    LowerError::Internal(format!(
                        "GlobalVariable {gv_handle:?} not found in global_map"
                    ))
                })?;

                if info.is_uniform {
                    return Err(LowerError::UnsupportedStatement(String::from(
                        "cannot write to uniform variable",
                    )));
                }

                let srcs = ctx.ensure_expr_vec(*value)?;
                if srcs.len() != info.component_count as usize {
                    return Err(LowerError::UnsupportedStatement(format!(
                        "Store to global: {} vs {} components",
                        srcs.len(),
                        info.component_count
                    )));
                }

                // Store each component to the VMContext buffer
                for (i, src) in srcs.iter().enumerate() {
                    let offset = info.byte_offset + (i as u32 * 4);
                    ctx.fb.push(LpirOp::Store {
                        base: VMCTX_VREG,
                        offset,
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
        } => lower_call::lower_user_call(ctx, *function, arguments, *result),
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

/// `true` if this block does not end with an explicit `return`.
pub(crate) fn void_block_missing_return(block: &Block) -> bool {
    !matches!(block.last(), Some(Statement::Return { .. }))
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
