//! User-function call lowering: vmctx, optional callee sret, aggregate args as pointers, aggregate
//! results via caller-allocated sret slot.
//!
//! LPIR [`LpirOp::Call`] arg order: `[vmctx, sret_dest_addr?, user_arg0, …]`
//! (see `lpir::LpirModule` / callee [`lpir::IrFunction::sret_arg`]).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lpir::{IrType, LpirOp, VMCTX_VREG, VReg};
use naga::{Expression, Handle, TypeInner};
use smallvec::smallvec;

use crate::lower_array::aggregate_storage_base_vreg;
use crate::lower_ctx::{LowerCtx, VRegVec, naga_type_to_ir_types};
use crate::lower_error::LowerError;
use crate::lower_lpfn;
use crate::naga_util::{aggregate_layout, func_return_ir_types_with_sret};

/// Peel `Load*` chains to a stack-slot aggregate [`LocalVariable`], if any.
fn peel_aggregate_call_arg_local(
    ctx: &LowerCtx<'_>,
    mut expr: Handle<Expression>,
) -> Option<crate::lower_ctx::AggregateInfo> {
    loop {
        match &ctx.func.expressions[expr] {
            Expression::Load { pointer } => expr = *pointer,
            Expression::LocalVariable(lv) => return ctx.aggregate_map.get(lv).cloned(),
            _ => return None,
        }
    }
}

/// Pointer to callee `in` aggregate argument: slot-backed local/load, [`CallResult`] slot, or temp
/// materialisation for rvalues (`Compose`, nested calls, etc.).
fn aggregate_arg_pointer(
    ctx: &mut LowerCtx<'_>,
    arg_h: Handle<Expression>,
    callee_arg_ty: Handle<crate::naga::Type>,
) -> Result<VReg, LowerError> {
    if let Some(info) = peel_aggregate_call_arg_local(ctx, arg_h) {
        return aggregate_storage_base_vreg(ctx, &info.slot);
    }
    if matches!(&ctx.func.expressions[arg_h], Expression::CallResult(_)) {
        if let Some(info) = ctx.call_result_aggregates.get(&arg_h).cloned() {
            return aggregate_storage_base_vreg(ctx, &info.slot);
        }
    }
    let layout = aggregate_layout(ctx.module, callee_arg_ty)?.ok_or_else(|| {
        LowerError::Internal(String::from(
            "aggregate_arg_pointer: expected aggregate layout",
        ))
    })?;
    let info = crate::lower_aggregate_write::materialise_aggregate_rvalue_to_temp_slot(
        ctx,
        arg_h,
        layout,
        callee_arg_ty,
    )?;
    aggregate_storage_base_vreg(ctx, &info.slot)
}

fn record_call_result_aggregate(
    ctx: &mut LowerCtx<'_>,
    res_h: Handle<Expression>,
    naga_ret_ty: Handle<crate::naga::Type>,
    slot: lpir::SlotId,
) -> Result<(), LowerError> {
    let layout = aggregate_layout(ctx.module, naga_ret_ty)?.ok_or_else(|| {
        LowerError::Internal(String::from(
            "record_call_result_aggregate: expected aggregate layout",
        ))
    })?;
    let info = crate::lower_ctx::AggregateInfo {
        slot: crate::lower_ctx::AggregateSlot::Local(slot),
        layout,
        naga_ty: naga_ret_ty,
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
            let layout = aggregate_layout(ctx.module, res_ty)?.ok_or_else(|| {
                LowerError::Internal(String::from("sret literal: expected aggregate layout"))
            })?;
            let temp = ctx.fb.alloc_slot(sret.size);
            let taddr = ctx.fb.alloc_vreg(IrType::Pointer);
            ctx.fb.push(LpirOp::SlotAddr {
                dst: taddr,
                slot: temp,
            });
            let info = crate::lower_ctx::AggregateInfo {
                slot: crate::lower_ctx::AggregateSlot::Local(temp),
                layout,
                naga_ty: res_ty,
            };
            match &info.layout.kind {
                crate::naga_util::AggregateKind::Struct { .. } => {
                    let lps_ty =
                        crate::lower_aggregate_layout::naga_to_lps_type(ctx.module, res_ty)?;
                    crate::lower_aggregate_write::store_lps_value_into_slot(
                        ctx,
                        taddr,
                        0,
                        res_ty,
                        &lps_ty,
                        value_expr,
                        Some(&info.layout),
                    )?;
                }
                crate::naga_util::AggregateKind::Array { .. } => {
                    if matches!(&ctx.func.expressions[value_expr], E::ZeroValue(_)) {
                        crate::lower_array::zero_fill_array(ctx, ctx.module, &info)?;
                    } else {
                        crate::lower_array::lower_array_initializer(ctx, &info, value_expr)?;
                    }
                }
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
            "aggregate return value is in flat vregs; expected slot-backed aggregate",
        )));
    }
    Err(LowerError::Internal(String::from(
        "cannot lower aggregate return expression for sret",
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
            let ir_tys: Vec<IrType> = naga_type_to_ir_types(ctx.module, inner)?.to_vec();
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
    let mut inout_writebacks: Vec<crate::lower_lvalue::WritableWriteback> = Vec::new();
    for (i, &arg_h) in arguments.iter().enumerate() {
        let callee_arg = &f.arguments[i];
        let callee_inner = &ctx.module.types[callee_arg.ty].inner;
        if let TypeInner::Pointer { base, .. } = callee_inner {
            let wa = crate::lower_lvalue::resolve_writable_actual(ctx, arg_h, *base)?;
            arg_vs.push(wa.addr);
            if let Some(wb) = wa.writeback {
                inout_writebacks.push(wb);
            }
        } else if aggregate_layout(ctx.module, callee_arg.ty)?.is_some() {
            let addr = aggregate_arg_pointer(ctx, arg_h, callee_arg.ty)?;
            arg_vs.push(addr);
        } else {
            let vs = ctx.ensure_expr_vec(arg_h)?;
            arg_vs.extend_from_slice(&vs);
        }
    }
    ctx.fb.push_call(callee_ref, &arg_vs, &result_vs);
    for wb in inout_writebacks {
        crate::lower_lvalue::apply_writable_writeback(ctx, wb)?;
    }
    Ok(())
}
