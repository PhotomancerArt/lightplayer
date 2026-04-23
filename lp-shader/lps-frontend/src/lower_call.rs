//! User-function call lowering: vmctx, optional callee sret, aggregate args as pointers, aggregate
//! results via caller-allocated sret slot.
//!
//! LPIR [`LpirOp::Call`] arg order: `[vmctx, sret_dest_addr?, user_arg0, …]`
//! (see `lpir::LpirModule` / callee [`lpir::IrFunction::sret_arg`]).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lpir::{IrType, LpirOp, SlotId, VMCTX_VREG, VReg};
use naga::{Expression, Function, Handle, LocalVariable, TypeInner};
use smallvec::smallvec;

use crate::lower_array::aggregate_storage_base_vreg;
use crate::lower_ctx::{LowerCtx, VRegVec, naga_type_to_ir_types};
use crate::lower_error::LowerError;
use crate::lower_lpfn;
use crate::naga_util::func_return_ir_types_with_sret;

use crate::lower_array_multidim;

/// R-value: local holding an array, or a [`Load`] of that local.
fn call_arg_array_local(
    ctx: &LowerCtx<'_>,
    mut expr: Handle<Expression>,
) -> Result<Handle<LocalVariable>, LowerError> {
    loop {
        match &ctx.func.expressions[expr] {
            Expression::Load { pointer } => expr = *pointer,
            Expression::LocalVariable(lv) => {
                if ctx.aggregate_map.contains_key(lv) {
                    return Ok(*lv);
                }
                return Err(LowerError::UnsupportedExpression(String::from(
                    "array call argument must be a local array",
                )));
            }
            _ => {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "array call argument must be a local array",
                )));
            }
        }
    }
}

/// `inout` / `out` pointer formals: must be a local.
fn call_arg_pointer_local(
    func: &Function,
    expr: Handle<Expression>,
) -> Result<Handle<LocalVariable>, LowerError> {
    match &func.expressions[expr] {
        Expression::LocalVariable(lv) => Ok(*lv),
        _ => Err(LowerError::UnsupportedExpression(String::from(
            "inout/out call argument must be a local variable",
        ))),
    }
}

fn record_call_result_aggregate(
    ctx: &mut LowerCtx<'_>,
    res_h: Handle<Expression>,
    naga_ret_ty: Handle<crate::naga::Type>,
    slot: lpir::SlotId,
) -> Result<(), LowerError> {
    let (dimensions, leaf_ty, leaf_stride) =
        lower_array_multidim::flatten_array_type_shape(ctx.module, naga_ret_ty)?;
    let element_count = dimensions
        .iter()
        .try_fold(1u32, |acc, &d| acc.checked_mul(d))
        .ok_or_else(|| {
            LowerError::Internal(String::from("record_call_result_aggregate: count overflow"))
        })?;
    let (total_size, _align) =
        crate::lower_aggregate_layout::aggregate_size_and_align(ctx.module, naga_ret_ty)?;
    let info = crate::lower_ctx::AggregateInfo {
        slot: crate::lower_ctx::AggregateSlot::Local(slot),
        dimensions,
        leaf_element_ty: leaf_ty,
        leaf_stride,
        element_count,
        total_size,
    };
    ctx.call_result_aggregates.insert(res_h, info);
    let addr = aggregate_storage_base_vreg(ctx, &crate::lower_ctx::AggregateSlot::Local(slot))?;
    if let Some(cache) = ctx.expr_cache.get_mut(res_h.index()) {
        *cache = Some(smallvec![addr]);
    }
    Ok(())
}

/// Copy an aggregate return value into the callee sret buffer.
pub(crate) fn write_aggregate_return_into_sret(
    ctx: &mut LowerCtx<'_>,
    value_expr: Handle<Expression>,
    sret: &crate::lower_ctx::SretCtx,
) -> Result<(), LowerError> {
    use crate::naga::Expression as E;
    match &ctx.func.expressions[value_expr] {
        E::LocalVariable(lv) => {
            if let Some(info) = ctx.aggregate_map.get(lv).cloned() {
                let src = aggregate_storage_base_vreg(ctx, &info.slot)?;
                ctx.fb.push(LpirOp::Memcpy {
                    dst_addr: sret.addr,
                    src_addr: src,
                    size: sret.size,
                });
                return Ok(());
            }
        }
        E::Load { pointer } => {
            if let E::LocalVariable(lv) = &ctx.func.expressions[*pointer] {
                if let Some(info) = ctx.aggregate_map.get(lv).cloned() {
                    let src = aggregate_storage_base_vreg(ctx, &info.slot)?;
                    ctx.fb.push(LpirOp::Memcpy {
                        dst_addr: sret.addr,
                        src_addr: src,
                        size: sret.size,
                    });
                    return Ok(());
                }
            }
        }
        E::CallResult(_) => {
            if let Some(info) = ctx.call_result_aggregates.get(&value_expr).cloned() {
                let src = aggregate_storage_base_vreg(ctx, &info.slot)?;
                ctx.fb.push(LpirOp::Memcpy {
                    dst_addr: sret.addr,
                    src_addr: src,
                    size: sret.size,
                });
                return Ok(());
            }
        }
        E::Compose { .. } | E::ZeroValue(_) => {
            let res_ty = ctx
                .func
                .result
                .as_ref()
                .ok_or_else(|| LowerError::Internal(String::from("sret: missing result type")))?
                .ty;
            let (dimensions, leaf_ty, leaf_stride) =
                lower_array_multidim::flatten_array_type_shape(ctx.module, res_ty)?;
            let element_count = dimensions
                .iter()
                .try_fold(1u32, |acc, &d| acc.checked_mul(d))
                .ok_or_else(|| {
                    LowerError::Internal(String::from("sret literal: count overflow"))
                })?;
            let temp = ctx.fb.alloc_slot(sret.size);
            let taddr = ctx.fb.alloc_vreg(IrType::Pointer);
            ctx.fb.push(LpirOp::SlotAddr {
                dst: taddr,
                slot: temp,
            });
            let info = crate::lower_ctx::AggregateInfo {
                slot: crate::lower_ctx::AggregateSlot::Local(temp),
                dimensions,
                leaf_element_ty: leaf_ty,
                leaf_stride,
                element_count,
                total_size: sret.size,
            };
            if matches!(&ctx.func.expressions[value_expr], E::ZeroValue(_)) {
                crate::lower_array::zero_fill_array(ctx, ctx.module, &info)?;
            } else {
                crate::lower_array::lower_array_initializer(ctx, &info, value_expr)?;
            }
            ctx.fb.push(LpirOp::Memcpy {
                dst_addr: sret.addr,
                src_addr: taddr,
                size: sret.size,
            });
            return Ok(());
        }
        _ => {}
    }
    let vs = ctx.ensure_expr_vec(value_expr)?;
    if !vs.is_empty() {
        return Err(LowerError::Internal(String::from(
            "M1: aggregate return value is in flat vregs; expected slot-backed array (TODO M2: struct literals)",
        )));
    }
    Err(LowerError::Internal(String::from(
        "M1: cannot lower aggregate return expression for sret",
    )))
}

pub(crate) fn lower_user_call(
    ctx: &mut LowerCtx<'_>,
    callee: Handle<crate::naga::Function>,
    arguments: &[Handle<Expression>],
    result: Option<Handle<Expression>>,
) -> Result<(), LowerError> {
    let f = &ctx.module.functions[callee];
    let name = f.name.as_deref().unwrap_or("");
    if name.starts_with("lpfn_") {
        return lower_lpfn::lower_lpfn_call(ctx, callee, arguments, result);
    }
    if f.body.is_empty() {
        if name == "__lp_get_fuel" {
            if let Some(res_h) = result {
                let key = "vm::__lp_get_fuel";
                let callee = ctx
                    .import_map
                    .get(key)
                    .copied()
                    .ok_or_else(|| LowerError::Internal(format!("missing import {key}")))?;
                let r = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push_call(callee, &[VMCTX_VREG], &[r]);
                let mut vregs = VRegVec::new();
                vregs.push(r);
                if let Some(slot) = ctx.expr_cache.get_mut(res_h.index()) {
                    *slot = Some(vregs);
                }
            }
            return Ok(());
        }
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
    // Callee: [`vmctx`, sret dest?, user_arg0, …] — sret (if any) is immediately after vmctx.
    arg_vs.push(VMCTX_VREG);
    let mut result_vs: Vec<VReg> = Vec::new();
    if let Some(res_h) = result {
        let res_ty = f
            .result
            .as_ref()
            .ok_or_else(|| LowerError::Internal(String::from("call result for void function")))?;
        let abi = func_return_ir_types_with_sret(ctx.module, Some(res_ty.ty))?;
        if abi.sret.is_some() {
            let slot = ctx.fb.alloc_slot(abi.sret_size);
            let addr = ctx.fb.alloc_vreg(IrType::Pointer);
            ctx.fb.push(LpirOp::SlotAddr { dst: addr, slot });
            arg_vs.push(addr);
            record_call_result_aggregate(ctx, res_h, res_ty.ty, slot)?;
        } else {
            let inner = &ctx.module.types[res_ty.ty].inner;
            let ir_tys: Vec<IrType> = naga_type_to_ir_types(inner)?.to_vec();
            let mut vregs = VRegVec::new();
            for ty in &ir_tys {
                let v = ctx.fb.alloc_vreg(*ty);
                vregs.push(v);
                result_vs.push(v);
            }
            if let Some(cache) = ctx.expr_cache.get_mut(res_h.index()) {
                *cache = Some(vregs);
            }
        }
    }
    let mut inout_copybacks: Vec<(Handle<LocalVariable>, SlotId)> = Vec::new();
    for (i, &arg_h) in arguments.iter().enumerate() {
        let callee_arg = &f.arguments[i];
        let callee_inner = &ctx.module.types[callee_arg.ty].inner;
        if let TypeInner::Pointer { base, .. } = callee_inner {
            let lv = call_arg_pointer_local(ctx.func, arg_h)?;
            if let Some(info) = ctx.aggregate_map.get(&lv).cloned() {
                let addr = aggregate_storage_base_vreg(ctx, &info.slot)?;
                arg_vs.push(addr);
            } else {
                let local_vregs = ctx.resolve_local(lv)?;
                let base_inner = &ctx.module.types[*base].inner;
                let ir_tys = naga_type_to_ir_types(base_inner)?;
                let slot = ctx.fb.alloc_slot(ir_tys.len() as u32 * 4);
                let addr = ctx.fb.alloc_vreg(IrType::Pointer);
                ctx.fb.push(LpirOp::SlotAddr { dst: addr, slot });
                for (j, &src) in local_vregs.iter().enumerate() {
                    ctx.fb.push(LpirOp::Store {
                        base: addr,
                        offset: (j * 4) as u32,
                        value: src,
                    });
                }
                arg_vs.push(addr);
                inout_copybacks.push((lv, slot));
            }
        } else if matches!(callee_inner, TypeInner::Array { .. }) {
            let lv = call_arg_array_local(ctx, arg_h)?;
            let info = ctx.aggregate_map.get(&lv).cloned().ok_or_else(|| {
                LowerError::UnsupportedExpression(String::from(
                    "aggregate call argument: not a stack-slot aggregate",
                ))
            })?;
            let addr = aggregate_storage_base_vreg(ctx, &info.slot)?;
            arg_vs.push(addr);
        } else {
            let vs = ctx.ensure_expr_vec(arg_h)?;
            arg_vs.extend_from_slice(&vs);
        }
    }
    ctx.fb.push_call(callee_ref, &arg_vs, &result_vs);
    for (lv, slot) in &inout_copybacks {
        let local_vregs = ctx.resolve_local(*lv)?;
        let addr = ctx.fb.alloc_vreg(IrType::Pointer);
        ctx.fb.push(LpirOp::SlotAddr {
            dst: addr,
            slot: *slot,
        });
        for (j, dst_v) in local_vregs.iter().enumerate() {
            ctx.fb.push(LpirOp::Load {
                dst: *dst_v,
                base: addr,
                offset: (j * 4) as u32,
            });
        }
    }
    Ok(())
}
