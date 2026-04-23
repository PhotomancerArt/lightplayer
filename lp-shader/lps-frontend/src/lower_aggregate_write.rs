//! Store typed [`lps_shared::LpsType`] **leaf** values (scalar/vector/matrix) into a stack slot, plus
//! a `Memcpy` fast path for whole slot-backed aggregates. Array **initializer lists** with nested
//! `Compose` are lowered in [`crate::lower_array::lower_array_initializer`] (flat leaf components) so
//! GLSL row-major order matches Naga’s compose shape even when `LpsType` nesting order differs.
//! `LpsType::Struct` in [`store_lps_value_into_slot`] is phase 04.

use alloc::format;
use alloc::string::String;

use lpir::{IrType, LpirOp, VReg};
use lps_shared::layout::type_size;
use lps_shared::{LayoutRules, LpsType};
use naga::{Expression, Handle, Module, Type};

use crate::lower_array::aggregate_storage_base_vreg;
use crate::lower_ctx::{AggregateInfo, AggregateSlot, LowerCtx};
use crate::lower_error::LowerError;
use crate::lower_expr::coerce_assignment_vregs;
use crate::naga_util::AggregateLayout;
use crate::naga_util::naga_type_to_ir_types;

/// LpsType of a stack aggregate (from its Naga type handle).
#[allow(dead_code, reason = "used by M2 phase 04 struct compose and tooling")]
pub(crate) fn lps_type_of_aggregate_info(
    module: &Module,
    info: &AggregateInfo,
) -> Result<LpsType, LowerError> {
    crate::lower_aggregate_layout::naga_to_lps_type(module, info.naga_ty)
}

/// Write the value of `expr_h` (typed as `lps_ty` / `naga_dest`) into
/// `[base + offset, base + offset + sizeof(lps_ty))` for a **scalar, vector, or matrix** `lps_ty`.
/// For whole-array/aggregate copies from another slot, use the memcpy fast path first.
/// For `LpsType::Struct`, pass `agg_layout: Some` with the matching [`AggregateLayout`].
/// Padding implied by std430 is left undefined.
pub(crate) fn store_lps_value_into_slot(
    ctx: &mut LowerCtx<'_>,
    base: VReg,
    offset: u32,
    naga_dest: Handle<Type>,
    lps_ty: &LpsType,
    expr_h: Handle<Expression>,
    agg_layout: Option<&AggregateLayout>,
) -> Result<(), LowerError> {
    if try_memcpy_slot_backed_aggregate(ctx, base, offset, naga_dest, lps_ty, expr_h)? {
        return Ok(());
    }

    match lps_ty {
        LpsType::Struct { .. } => {
            let members = agg_layout.and_then(|l| l.struct_members()).ok_or_else(|| {
                LowerError::Internal(String::from(
                    "store_lps_value_into_slot: struct needs AggregateLayout with members",
                ))
            })?;
            match &ctx.func.expressions[expr_h] {
                Expression::Compose { components, .. } => {
                    if components.len() != members.len() {
                        return Err(LowerError::UnsupportedExpression(format!(
                            "struct compose: {} components vs {} members",
                            components.len(),
                            members.len()
                        )));
                    }
                    for (i, &comp) in components.iter().enumerate() {
                        let m = &members[i];
                        if matches!(&m.lps_ty, LpsType::Struct { .. }) {
                            let sub = crate::naga_util::aggregate_layout(ctx.module, m.naga_ty)?
                                .ok_or_else(|| {
                                    LowerError::Internal(String::from("nested struct layout"))
                                })?;
                            store_lps_value_into_slot(
                                ctx,
                                base,
                                offset + m.byte_offset,
                                m.naga_ty,
                                &m.lps_ty,
                                comp,
                                Some(&sub),
                            )?;
                        } else {
                            store_lps_value_into_slot(
                                ctx,
                                base,
                                offset + m.byte_offset,
                                m.naga_ty,
                                &m.lps_ty,
                                comp,
                                None,
                            )?;
                        }
                    }
                    Ok(())
                }
                Expression::ZeroValue(_) => {
                    crate::lower_struct::zero_struct_at_offset(ctx, base, offset, naga_dest)
                }
                Expression::FunctionArgument(arg_i) => {
                    let arg = ctx.func.arguments.get(*arg_i as usize).ok_or_else(|| {
                        LowerError::Internal(String::from("struct field: bad arg"))
                    })?;
                    if arg.ty != naga_dest {
                        return Err(LowerError::Internal(String::from(
                            "struct field init: by-value argument type mismatch",
                        )));
                    }
                    let field_layout = crate::naga_util::aggregate_layout(ctx.module, naga_dest)?
                        .ok_or_else(|| {
                        LowerError::Internal(String::from(
                            "store_lps_value_into_slot: struct field layout",
                        ))
                    })?;
                    let src = ctx.arg_vregs_for(*arg_i)?[0];
                    let dst_addr = vreg_ptr_plus_bytes(ctx, base, offset)?;
                    ctx.fb.push(LpirOp::Memcpy {
                        dst_addr,
                        src_addr: src,
                        size: field_layout.total_size,
                    });
                    Ok(())
                }
                Expression::LocalVariable(_)
                | Expression::Load { .. }
                | Expression::CallResult(_) => Err(LowerError::Internal(String::from(
                    "store_lps_value_into_slot: struct rvalue not slot-backed (use memcpy path)",
                ))),
                _ => Err(LowerError::UnsupportedExpression(format!(
                    "struct source: {:?}",
                    ctx.func.expressions[expr_h]
                ))),
            }
        }
        LpsType::Array { .. } => Err(LowerError::Internal(String::from(
            "store_lps_value_into_slot: LpsType::Array (use array init flatten or memcpy)",
        ))),
        LpsType::Float
        | LpsType::Int
        | LpsType::UInt
        | LpsType::Bool
        | LpsType::Vec2
        | LpsType::Vec3
        | LpsType::Vec4
        | LpsType::IVec2
        | LpsType::IVec3
        | LpsType::IVec4
        | LpsType::UVec2
        | LpsType::UVec3
        | LpsType::UVec4
        | LpsType::BVec2
        | LpsType::BVec3
        | LpsType::BVec4
        | LpsType::Mat2
        | LpsType::Mat3
        | LpsType::Mat4 => {
            let _ = agg_layout;
            let inner = &ctx.module.types[naga_dest].inner;
            let raw = ctx.ensure_expr_vec(expr_h)?;
            let srcs = coerce_assignment_vregs(ctx, Some(naga_dest), inner, expr_h, raw)?;
            let ir_tys = naga_type_to_ir_types(ctx.module, inner)?;
            if srcs.len() != ir_tys.len() {
                return Err(LowerError::UnsupportedStatement(format!(
                    "slot store: {} vs {} components",
                    srcs.len(),
                    ir_tys.len()
                )));
            }
            for (j, &src) in srcs.iter().enumerate() {
                ctx.fb.push(LpirOp::Store {
                    base,
                    offset: offset + (j as u32) * 4,
                    value: src,
                });
            }
            Ok(())
        }
        LpsType::Void => Err(LowerError::Internal(String::from(
            "store_lps_value_into_slot: void",
        ))),
    }
}

/// `Memcpy` from a slot-backed aggregate with the same Naga type as `info` (inferred-size arrays
/// included: no `naga_to_lps` on the array handle).
/// Returns `true` if the memcpy was emitted.
pub(crate) fn try_memcpy_aggregate_expr(
    ctx: &mut LowerCtx<'_>,
    base: VReg,
    offset: u32,
    info: &AggregateInfo,
    expr_h: Handle<Expression>,
) -> Result<bool, LowerError> {
    let Some(src_info) = peel_slot_backed_aggregate_info(ctx, expr_h) else {
        return Ok(false);
    };
    if src_info.naga_ty != info.naga_ty {
        return Ok(false);
    }
    if src_info.total_size() != info.total_size() {
        return Err(LowerError::Internal(String::from(
            "memcpy: matching naga_ty but total_size mismatch",
        )));
    }
    emit_memcpy_to_dest(ctx, base, offset, &src_info, info.total_size())
}

/// Returns `true` if a matching [`LpirOp::Memcpy`] was emitted.
fn try_memcpy_slot_backed_aggregate(
    ctx: &mut LowerCtx<'_>,
    dst_base: VReg,
    dst_offset: u32,
    naga_dest: Handle<Type>,
    lps_ty: &LpsType,
    expr_h: Handle<Expression>,
) -> Result<bool, LowerError> {
    let Some(src_info) = peel_slot_backed_aggregate_info(ctx, expr_h) else {
        return Ok(false);
    };
    if src_info.naga_ty != naga_dest {
        return Ok(false);
    }
    let want_bytes = type_size(lps_ty, LayoutRules::Std430) as u32;
    if src_info.total_size() != want_bytes {
        return Ok(false);
    }
    emit_memcpy_to_dest(ctx, dst_base, dst_offset, &src_info, want_bytes)
}

/// Emits a [`LpirOp::Memcpy`] from `src_info`'s storage to `(dst_base + dst_offset)`.
fn emit_memcpy_to_dest(
    ctx: &mut LowerCtx<'_>,
    dst_base: VReg,
    dst_offset: u32,
    src_info: &AggregateInfo,
    size: u32,
) -> Result<bool, LowerError> {
    let src_base = aggregate_storage_base_vreg(ctx, &src_info.slot)?;
    let dst_addr = vreg_ptr_plus_bytes(ctx, dst_base, dst_offset)?;
    ctx.fb.push(LpirOp::Memcpy {
        dst_addr,
        src_addr: src_base,
        size,
    });
    Ok(true)
}

pub(crate) fn vreg_ptr_plus_bytes(
    ctx: &mut LowerCtx<'_>,
    base: VReg,
    add_bytes: u32,
) -> Result<VReg, LowerError> {
    if add_bytes == 0 {
        return Ok(base);
    }
    let off = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::IconstI32 {
        dst: off,
        value: add_bytes as i32,
    });
    let out = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Iadd {
        dst: out,
        lhs: base,
        rhs: off,
    });
    Ok(out)
}

fn peel_slot_backed_aggregate_info(
    ctx: &LowerCtx<'_>,
    expr_h: Handle<Expression>,
) -> Option<AggregateInfo> {
    match &ctx.func.expressions[expr_h] {
        Expression::LocalVariable(lv) => ctx.aggregate_map.get(lv).cloned(),
        Expression::Load { pointer } => {
            if let Expression::LocalVariable(lv) = &ctx.func.expressions[*pointer] {
                ctx.aggregate_map.get(lv).cloned()
            } else {
                None
            }
        }
        Expression::CallResult(_) => ctx.call_result_aggregates.get(&expr_h).cloned(),
        _ => None,
    }
}

/// Zero a **scalar, vector, or matrix** at `offset` (array init tail elements).
pub(crate) fn zero_leaf_lps_in_slot(
    ctx: &mut LowerCtx<'_>,
    base: VReg,
    offset: u32,
    naga_ty: Handle<Type>,
    lps_ty: &LpsType,
) -> Result<(), LowerError> {
    match lps_ty {
        LpsType::Struct { .. } => {
            crate::lower_struct::zero_struct_at_offset(ctx, base, offset, naga_ty)
        }
        LpsType::Array { .. } => Err(LowerError::Internal(String::from(
            "zero_leaf_lps_in_slot: nested array (expected leaf only)",
        ))),
        LpsType::Float
        | LpsType::Int
        | LpsType::UInt
        | LpsType::Bool
        | LpsType::Vec2
        | LpsType::Vec3
        | LpsType::Vec4
        | LpsType::IVec2
        | LpsType::IVec3
        | LpsType::IVec4
        | LpsType::UVec2
        | LpsType::UVec3
        | LpsType::UVec4
        | LpsType::BVec2
        | LpsType::BVec3
        | LpsType::BVec4
        | LpsType::Mat2
        | LpsType::Mat3
        | LpsType::Mat4 => {
            let inner = &ctx.module.types[naga_ty].inner;
            let ir_tys = naga_type_to_ir_types(ctx.module, inner)?;
            for (j, ty) in ir_tys.iter().enumerate() {
                let z = ctx.fb.alloc_vreg(*ty);
                push_zero_for_ir_type(&mut ctx.fb, z, *ty);
                ctx.fb.push(LpirOp::Store {
                    base,
                    offset: offset + (j as u32) * 4,
                    value: z,
                });
            }
            Ok(())
        }
        LpsType::Void => Err(LowerError::Internal(String::from("zero: void"))),
    }
}

fn push_zero_for_ir_type(fb: &mut lpir::FunctionBuilder, dst: VReg, ty: IrType) {
    match ty {
        IrType::F32 => fb.push(LpirOp::FconstF32 { dst, value: 0.0 }),
        IrType::I32 | IrType::Pointer => fb.push(LpirOp::IconstI32 { dst, value: 0 }),
    }
}

/// Allocate a temp stack slot and lower `expr_h` into it (struct or array rvalue).
pub(crate) fn materialise_aggregate_rvalue_to_temp_slot(
    ctx: &mut LowerCtx<'_>,
    expr_h: Handle<Expression>,
    layout: AggregateLayout,
    naga_ty: Handle<Type>,
) -> Result<AggregateInfo, LowerError> {
    let slot = ctx.fb.alloc_slot(layout.total_size);
    let lps_ty = crate::lower_aggregate_layout::naga_to_lps_type(ctx.module, naga_ty)?;
    let base = ctx.fb.alloc_vreg(IrType::Pointer);
    ctx.fb.push(LpirOp::SlotAddr { dst: base, slot });
    let info = AggregateInfo {
        slot: AggregateSlot::Local(slot),
        layout,
        naga_ty,
    };
    match &info.layout.kind {
        crate::naga_util::AggregateKind::Struct { .. } => {
            store_lps_value_into_slot(ctx, base, 0, naga_ty, &lps_ty, expr_h, Some(&info.layout))?;
        }
        crate::naga_util::AggregateKind::Array { .. } => {
            use naga::Expression as E;
            if matches!(&ctx.func.expressions[expr_h], E::ZeroValue(_)) {
                crate::lower_array::zero_fill_array(ctx, ctx.module, &info)?;
            } else {
                crate::lower_array::lower_array_initializer(ctx, &info, expr_h)?;
            }
        }
    }
    Ok(info)
}
