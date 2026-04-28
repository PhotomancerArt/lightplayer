//! Stack-slot arrays: zero-fill, initializer lists, indexed load/store with bounds clamping.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use smallvec::smallvec;

use lpir::{IrType, LpirOp, VMCTX_VREG, VReg};
use lps_shared::LpsType;
use naga::{
    BinaryOperator, Expression, Function, Handle, Literal, LocalVariable, Module, ScalarKind, Type,
    TypeInner,
};

use crate::lower_ctx::{
    AggregateInfo, AggregateSlot, LowerCtx, VRegVec,
    debug_assert_not_param_readonly_aggregate_store, naga_type_to_ir_types, vector_size_usize,
};
use crate::lower_error::LowerError;
use crate::lower_expr::coerce_assignment_vregs;

/// Clamp dynamic index to `[0, element_count-1]` (v1 safety; see `docs/design/arrays.md`).
/// Build flat element index from mixed const/dynamic subscripts (row-major).
pub(crate) fn emit_row_major_flat_from_operands(
    ctx: &mut LowerCtx<'_>,
    dimensions: &[u32],
    ops: &[crate::lower_array_multidim::SubscriptOperand],
) -> Result<VReg, LowerError> {
    if dimensions.len() != ops.len() {
        return Err(LowerError::Internal(format!(
            "emit_row_major_flat_from_operands: dim {} vs ops {}",
            dimensions.len(),
            ops.len()
        )));
    }
    let mut vregs = Vec::new();
    for (d, op) in dimensions.iter().zip(ops.iter()) {
        let v = match op {
            crate::lower_array_multidim::SubscriptOperand::Const(c) => {
                let cc = (*c).min(*d - 1);
                let vreg = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(LpirOp::IconstI32 {
                    dst: vreg,
                    value: cc as i32,
                });
                vreg
            }
            crate::lower_array_multidim::SubscriptOperand::Dynamic(h) => {
                let raw = ctx.ensure_expr(*h)?;
                clamp_array_index(ctx, raw, *d)?
            }
        };
        vregs.push(v);
    }
    emit_row_major_flat_index_vregs(ctx, dimensions, &vregs)
}

/// Row-major linear element index from per-axis dynamic indices (each axis clamped).
pub(crate) fn emit_row_major_flat_index_vregs(
    ctx: &mut LowerCtx<'_>,
    dimensions: &[u32],
    index_v: &[VReg],
) -> Result<VReg, LowerError> {
    if dimensions.is_empty() || dimensions.len() != index_v.len() {
        return Err(LowerError::Internal(format!(
            "emit_row_major_flat_index_vregs: dim {} vs idx {}",
            dimensions.len(),
            index_v.len()
        )));
    }
    let mut acc = clamp_array_index(ctx, index_v[0], dimensions[0])?;
    for k in 1..dimensions.len() {
        let dk = dimensions[k];
        let dim_v = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(LpirOp::IconstI32 {
            dst: dim_v,
            value: dk as i32,
        });
        let prod = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(LpirOp::Imul {
            dst: prod,
            lhs: acc,
            rhs: dim_v,
        });
        let ik = clamp_array_index(ctx, index_v[k], dk)?;
        let sum = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(LpirOp::Iadd {
            dst: sum,
            lhs: prod,
            rhs: ik,
        });
        acc = sum;
    }
    Ok(acc)
}

pub(crate) fn clamp_array_index(
    ctx: &mut LowerCtx<'_>,
    index_v: VReg,
    element_count: u32,
) -> Result<VReg, LowerError> {
    if element_count == 0 {
        return Err(LowerError::Internal(String::from(
            "clamp_array_index: empty array",
        )));
    }
    let zero = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::IconstI32 {
        dst: zero,
        value: 0,
    });

    let len = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::IconstI32 {
        dst: len,
        value: element_count as i32,
    });

    let max_idx = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::IconstI32 {
        dst: max_idx,
        value: (element_count - 1) as i32,
    });

    let lt0 = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::IltS {
        dst: lt0,
        lhs: index_v,
        rhs: zero,
    });
    let after_low = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Select {
        dst: after_low,
        cond: lt0,
        if_true: zero,
        if_false: index_v,
    });

    let ge_len = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::IgeU {
        dst: ge_len,
        lhs: after_low,
        rhs: len,
    });
    let out = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Select {
        dst: out,
        cond: ge_len,
        if_true: max_idx,
        if_false: after_low,
    });
    Ok(out)
}

/// Element index for [`array_element_address`]: literal subscript or dynamic vreg (clamped for
/// [`ElementIndex::Dynamic`]).
pub(crate) enum ElementIndex {
    Const(u32),
    Dynamic(VReg),
}

/// Byte address of array element `index` within the aggregate slot (`base + index * leaf_stride`),
/// with dynamic indices clamped like [`clamp_array_index`]. Constant indices use the same OOB clamp
/// as dynamic loads (`index.min(element_count - 1)`).
pub(crate) fn array_element_address(
    ctx: &mut LowerCtx<'_>,
    info: &AggregateInfo,
    index: ElementIndex,
) -> Result<VReg, LowerError> {
    array_element_address_with_field_offset(ctx, info, index, 0)
}

/// Like [`array_element_address`], but the array's bytes start at `field_offset` from the aggregate
/// slot base (e.g. `Point ps[N]` field inside a stack [`AggregateInfo`] struct local).
pub(crate) fn array_element_address_with_field_offset(
    ctx: &mut LowerCtx<'_>,
    info: &AggregateInfo,
    index: ElementIndex,
    field_offset: u32,
) -> Result<VReg, LowerError> {
    if info.element_count() == 0 {
        return Err(LowerError::Internal(String::from(
            "array_element_address: empty array",
        )));
    }
    let base = aggregate_storage_base_vreg(ctx, &info.slot)?;
    let stride = info.leaf_stride();
    match index {
        ElementIndex::Const(i) => {
            let i = i.min(info.element_count() - 1);
            let array_off = i.checked_mul(stride).ok_or_else(|| {
                LowerError::Internal(String::from("array_element_address: const offset overflow"))
            })?;
            let total_off = field_offset
                .checked_add(array_off)
                .ok_or_else(|| LowerError::Internal(String::from("array_element_address: off")))?;
            if total_off == 0 {
                Ok(base)
            } else {
                let off_v = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(LpirOp::IconstI32 {
                    dst: off_v,
                    value: total_off as i32,
                });
                let addr = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(LpirOp::Iadd {
                    dst: addr,
                    lhs: base,
                    rhs: off_v,
                });
                Ok(addr)
            }
        }
        ElementIndex::Dynamic(index_v) => {
            let clamped = clamp_array_index(ctx, index_v, info.element_count())?;
            let stride_v = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::IconstI32 {
                dst: stride_v,
                value: stride as i32,
            });
            let byte_off = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::Imul {
                dst: byte_off,
                lhs: clamped,
                rhs: stride_v,
            });
            let addr = ctx.fb.alloc_vreg(IrType::I32);
            if field_offset == 0 {
                ctx.fb.push(LpirOp::Iadd {
                    dst: addr,
                    lhs: base,
                    rhs: byte_off,
                });
            } else {
                let off_extra = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(LpirOp::IconstI32 {
                    dst: off_extra,
                    value: field_offset as i32,
                });
                let sum = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(LpirOp::Iadd {
                    dst: sum,
                    lhs: byte_off,
                    rhs: off_extra,
                });
                ctx.fb.push(LpirOp::Iadd {
                    dst: addr,
                    lhs: base,
                    rhs: sum,
                });
            }
            Ok(addr)
        }
    }
}

pub(crate) fn array_slot_base(ctx: &mut LowerCtx<'_>, slot: lpir::SlotId) -> VReg {
    array_slot_base_fb(&mut ctx.fb, slot)
}

fn array_slot_base_fb(fb: &mut lpir::FunctionBuilder, slot: lpir::SlotId) -> VReg {
    let base = fb.alloc_vreg(IrType::Pointer);
    fb.push(LpirOp::SlotAddr { dst: base, slot });
    base
}

/// Base address for aggregate (array) storage: local [`SlotAddr`] or `Pointer` param in
/// [`LowerCtx::arg_vregs`][0].
pub(crate) fn aggregate_storage_base_vreg(
    ctx: &mut LowerCtx<'_>,
    slot: &AggregateSlot,
) -> Result<VReg, LowerError> {
    match slot {
        AggregateSlot::Local(s) => Ok(array_slot_base(ctx, *s)),
        AggregateSlot::Param(arg_i) | AggregateSlot::ParamReadOnly(arg_i) => {
            ctx.arg_vregs_for(*arg_i)?.first().copied().ok_or_else(|| {
                LowerError::Internal(String::from("array param: missing address vreg"))
            })
        }
        AggregateSlot::Global(gv) => {
            let ginfo = ctx.global_map.get(gv).ok_or_else(|| {
                LowerError::Internal(String::from(
                    "array global: GlobalVariable not in global_map",
                ))
            })?;
            let off = ginfo.byte_offset;
            if off == 0 {
                Ok(VMCTX_VREG)
            } else {
                let off_v = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(LpirOp::IconstI32 {
                    dst: off_v,
                    value: i32::try_from(off).map_err(|_| {
                        LowerError::Internal(String::from("array global: byte_offset overflow"))
                    })?,
                });
                let addr = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(LpirOp::Iadd {
                    dst: addr,
                    lhs: VMCTX_VREG,
                    rhs: off_v,
                });
                Ok(addr)
            }
        }
    }
}

/// Zero-initialize every element (no `LowerCtx`; used from [`crate::lower_ctx::LowerCtx::new`]).
pub(crate) fn zero_fill_array_slot(
    fb: &mut lpir::FunctionBuilder,
    module: &Module,
    info: &AggregateInfo,
) -> Result<(), LowerError> {
    // `Param` / `ParamReadOnly` never use stack zero-fill in prologue; only `Local` slots.
    let AggregateSlot::Local(slot) = info.slot else {
        return Err(LowerError::Internal(String::from(
            "zero_fill_array_slot: not a local stack array",
        )));
    };
    let leaf_naga = info.leaf_element_ty();
    let elem_inner = &module.types[leaf_naga].inner;
    let base = array_slot_base_fb(fb, slot);

    if matches!(elem_inner, TypeInner::Struct { .. }) {
        for i in 0..info.element_count() {
            let byte_off = i.checked_mul(info.leaf_stride()).ok_or_else(|| {
                LowerError::Internal(String::from("zero_fill_array_slot: stride overflow"))
            })?;
            crate::lower_struct::zero_struct_at_offset_fb(fb, module, base, byte_off, leaf_naga)?;
        }
        return Ok(());
    }

    let ir_tys = naga_type_to_ir_types(module, elem_inner)?;
    for i in 0..info.element_count() {
        let byte_off = i.checked_mul(info.leaf_stride()).ok_or_else(|| {
            LowerError::Internal(String::from("zero_fill_array_slot: stride overflow"))
        })?;
        for (j, ty) in ir_tys.iter().enumerate() {
            let z = fb.alloc_vreg(*ty);
            push_zero_for_ir_type(fb, z, *ty);
            fb.push(LpirOp::Store {
                base,
                offset: byte_off + (j as u32) * 4,
                value: z,
            });
        }
    }
    Ok(())
}

pub(crate) fn zero_fill_array(
    ctx: &mut LowerCtx<'_>,
    module: &Module,
    info: &AggregateInfo,
) -> Result<(), LowerError> {
    zero_fill_array_slot(&mut ctx.fb, module, info)
}

/// Zero a [`TypeInner::Array]` region at `base + region_off` (e.g. array field inside a struct).
pub(crate) fn zero_array_region_in_slot(
    fb: &mut lpir::FunctionBuilder,
    module: &Module,
    base: VReg,
    region_off: u32,
    naga_array_ty: Handle<Type>,
) -> Result<(), LowerError> {
    let Some(layout) = crate::naga_util::aggregate_layout(module, naga_array_ty)? else {
        return Err(LowerError::Internal(String::from(
            "zero_array_region: not aggregate",
        )));
    };
    let crate::naga_util::AggregateKind::Array {
        leaf_element_ty,
        leaf_stride,
        element_count,
        ..
    } = &layout.kind
    else {
        return Err(LowerError::Internal(String::from(
            "zero_array_region: not array",
        )));
    };
    let leaf_naga = *leaf_element_ty;
    let leaf_stride = *leaf_stride;
    let element_count = *element_count;
    let elem_inner = &module.types[leaf_naga].inner;
    if matches!(elem_inner, TypeInner::Struct { .. }) {
        for i in 0..element_count {
            let bo = i
                .checked_mul(leaf_stride)
                .and_then(|b| region_off.checked_add(b))
                .ok_or_else(|| {
                    LowerError::Internal(String::from("zero_array_region: struct leaf off"))
                })?;
            crate::lower_struct::zero_struct_at_offset_fb(fb, module, base, bo, leaf_naga)?;
        }
        return Ok(());
    }
    let ir_tys = naga_type_to_ir_types(module, elem_inner)?;
    for i in 0..element_count {
        let elem_base = i
            .checked_mul(leaf_stride)
            .and_then(|b| region_off.checked_add(b))
            .ok_or_else(|| LowerError::Internal(String::from("zero_array_region: elem off")))?;
        for (j, ty) in ir_tys.iter().enumerate() {
            let z = fb.alloc_vreg(*ty);
            push_zero_for_ir_type(fb, z, *ty);
            fb.push(LpirOp::Store {
                base,
                offset: elem_base + (j as u32) * 4,
                value: z,
            });
        }
    }
    Ok(())
}

fn push_zero_for_ir_type(fb: &mut lpir::FunctionBuilder, dst: VReg, ty: IrType) {
    match ty {
        IrType::F32 => fb.push(LpirOp::FconstF32 { dst, value: 0.0 }),
        IrType::I32 | IrType::Pointer => fb.push(LpirOp::IconstI32 { dst, value: 0 }),
    }
}

pub(crate) fn load_array_element_const(
    ctx: &mut LowerCtx<'_>,
    info: &AggregateInfo,
    index: u32,
) -> Result<VRegVec, LowerError> {
    if info.element_count() == 0 {
        return Err(LowerError::Internal(String::from(
            "load_array_element_const: empty array",
        )));
    }
    let elem_inner = &ctx.module.types[info.leaf_element_ty()].inner;
    let ir_tys = naga_type_to_ir_types(ctx.module, elem_inner)?;
    let addr = array_element_address(ctx, info, ElementIndex::Const(index))?;
    let mut out = VRegVec::new();
    for (j, ty) in ir_tys.iter().enumerate() {
        let dst = ctx.fb.alloc_vreg(*ty);
        ctx.fb.push(LpirOp::Load {
            dst,
            base: addr,
            offset: (j as u32) * 4,
        });
        out.push(dst);
    }
    Ok(out)
}

pub(crate) fn load_array_element_dynamic(
    ctx: &mut LowerCtx<'_>,
    info: &AggregateInfo,
    index_v: VReg,
) -> Result<VRegVec, LowerError> {
    let addr = array_element_address(ctx, info, ElementIndex::Dynamic(index_v))?;
    let elem_inner = &ctx.module.types[info.leaf_element_ty()].inner;
    let ir_tys = naga_type_to_ir_types(ctx.module, elem_inner)?;
    let mut out = VRegVec::new();
    for (j, ty) in ir_tys.iter().enumerate() {
        let dst = ctx.fb.alloc_vreg(*ty);
        ctx.fb.push(LpirOp::Load {
            dst,
            base: addr,
            offset: (j as u32) * 4,
        });
        out.push(dst);
    }
    Ok(out)
}

pub(crate) fn store_array_element_const_vregs(
    ctx: &mut LowerCtx<'_>,
    info: &AggregateInfo,
    index: u32,
    srcs: &[VReg],
) -> Result<(), LowerError> {
    debug_assert_not_param_readonly_aggregate_store(info, "store_array_element_const_vregs");
    if info.element_count() == 0 {
        return Err(LowerError::Internal(String::from(
            "store_array_element_const_vregs: empty array",
        )));
    }
    let elem_inner = &ctx.module.types[info.leaf_element_ty()].inner;
    let ir_tys = naga_type_to_ir_types(ctx.module, elem_inner)?;
    if srcs.len() != ir_tys.len() {
        return Err(LowerError::UnsupportedStatement(format!(
            "store_array_element_const_vregs: {} vs {} components",
            srcs.len(),
            ir_tys.len()
        )));
    }
    let addr = array_element_address(ctx, info, ElementIndex::Const(index))?;
    for (j, &src) in srcs.iter().enumerate() {
        ctx.fb.push(LpirOp::Store {
            base: addr,
            offset: (j as u32) * 4,
            value: src,
        });
    }
    Ok(())
}

pub(crate) fn store_array_element_dynamic_vregs(
    ctx: &mut LowerCtx<'_>,
    info: &AggregateInfo,
    index_v: VReg,
    srcs: &[VReg],
) -> Result<(), LowerError> {
    debug_assert_not_param_readonly_aggregate_store(info, "store_array_element_dynamic_vregs");
    let addr = array_element_address(ctx, info, ElementIndex::Dynamic(index_v))?;
    let elem_inner = &ctx.module.types[info.leaf_element_ty()].inner;
    let ir_tys = naga_type_to_ir_types(ctx.module, elem_inner)?;
    if srcs.len() != ir_tys.len() {
        return Err(LowerError::UnsupportedStatement(format!(
            "store_array_element_dynamic_vregs: {} vs {} components",
            srcs.len(),
            ir_tys.len()
        )));
    }
    for (j, &src) in srcs.iter().enumerate() {
        ctx.fb.push(LpirOp::Store {
            base: addr,
            offset: (j as u32) * 4,
            value: src,
        });
    }
    Ok(())
}

pub(crate) fn store_array_element_const(
    ctx: &mut LowerCtx<'_>,
    info: &AggregateInfo,
    index: u32,
    value_expr: Handle<Expression>,
) -> Result<(), LowerError> {
    debug_assert_not_param_readonly_aggregate_store(info, "store_array_element_const");
    if info.element_count() == 0 {
        return Err(LowerError::Internal(String::from(
            "store_array_element_const: empty array",
        )));
    }
    let elem_inner = &ctx.module.types[info.leaf_element_ty()].inner;
    let raw = ctx.ensure_expr_vec(value_expr)?;
    let srcs = coerce_assignment_vregs(ctx, None, elem_inner, value_expr, raw)?;
    let ir_tys = naga_type_to_ir_types(ctx.module, elem_inner)?;
    if srcs.len() != ir_tys.len() {
        return Err(LowerError::UnsupportedStatement(format!(
            "array element store: {} vs {} components",
            srcs.len(),
            ir_tys.len()
        )));
    }
    let addr = array_element_address(ctx, info, ElementIndex::Const(index))?;
    for (j, &src) in srcs.iter().enumerate() {
        ctx.fb.push(LpirOp::Store {
            base: addr,
            offset: (j as u32) * 4,
            value: src,
        });
    }
    Ok(())
}

pub(crate) fn store_array_element_dynamic(
    ctx: &mut LowerCtx<'_>,
    info: &AggregateInfo,
    index_v: VReg,
    value_expr: Handle<Expression>,
) -> Result<(), LowerError> {
    debug_assert_not_param_readonly_aggregate_store(info, "store_array_element_dynamic");
    let addr = array_element_address(ctx, info, ElementIndex::Dynamic(index_v))?;
    let elem_inner = &ctx.module.types[info.leaf_element_ty()].inner;
    let raw = ctx.ensure_expr_vec(value_expr)?;
    let srcs = coerce_assignment_vregs(ctx, None, elem_inner, value_expr, raw)?;
    let ir_tys = naga_type_to_ir_types(ctx.module, elem_inner)?;
    if srcs.len() != ir_tys.len() {
        return Err(LowerError::UnsupportedStatement(format!(
            "array element store: {} vs {} components",
            srcs.len(),
            ir_tys.len()
        )));
    }
    for (j, &src) in srcs.iter().enumerate() {
        ctx.fb.push(LpirOp::Store {
            base: addr,
            offset: (j as u32) * 4,
            value: src,
        });
    }
    Ok(())
}

pub(crate) fn lower_array_initializer(
    ctx: &mut LowerCtx<'_>,
    info: &AggregateInfo,
    init_h: Handle<Expression>,
) -> Result<(), LowerError> {
    if matches!(&ctx.func.expressions[init_h], Expression::ZeroValue(_)) {
        if matches!(info.slot, AggregateSlot::ParamReadOnly(_)) {
            // No stack copy: zero-fill would store into the caller’s `in` buffer — not elided.
            return Ok(());
        }
        return zero_fill_array(ctx, ctx.module, info);
    }
    debug_assert_not_param_readonly_aggregate_store(
        info,
        "lower_array_initializer: non-ZeroValue init",
    );
    let base = aggregate_storage_base_vreg(ctx, &info.slot)?;
    if crate::lower_aggregate_write::try_memcpy_aggregate_expr(ctx, base, 0, info, init_h)? {
        return Ok(());
    }
    let leaf_naga = info.leaf_element_ty();
    let leaf_lps = crate::lower_aggregate_layout::naga_to_lps_type(ctx.module, leaf_naga)?;
    let leaf_struct_layout = match &leaf_lps {
        LpsType::Struct { .. } => Some(
            crate::naga_util::aggregate_layout(ctx.module, leaf_naga)?.ok_or_else(|| {
                LowerError::Internal(String::from(
                    "array initializer: missing struct leaf aggregate layout",
                ))
            })?,
        ),
        _ => None,
    };
    match &ctx.func.expressions[init_h] {
        Expression::Compose { .. } => {
            // Flatten to leaf components (row-major). Naga `LpsType` nesting for multi-dim arrays
            // can differ from the initializer's `Compose` tree (see M2 notes); do not recurse `Array`
            // in `store_lps_value_into_slot` for this path.
            let depth = info.dimensions().len().saturating_sub(1);
            let flat = collect_flat_compose_components(ctx.func, init_h, depth)?;
            if flat.len() as u32 > info.element_count() {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "array initializer: too many elements",
                )));
            }
            for (i, &comp) in flat.iter().enumerate() {
                let byte_off = (i as u32)
                    .checked_mul(info.leaf_stride())
                    .ok_or_else(|| LowerError::Internal(String::from("init: byte_off overflow")))?;
                crate::lower_aggregate_write::store_lps_value_into_slot(
                    ctx,
                    base,
                    byte_off,
                    leaf_naga,
                    &leaf_lps,
                    comp,
                    leaf_struct_layout.as_ref(),
                )?;
            }
            for i in (flat.len() as u32)..info.element_count() {
                let byte_off = i.checked_mul(info.leaf_stride()).ok_or_else(|| {
                    LowerError::Internal(String::from("init: tail byte_off overflow"))
                })?;
                crate::lower_aggregate_write::zero_leaf_lps_in_slot(
                    ctx, base, byte_off, leaf_naga, &leaf_lps,
                )?;
            }
            Ok(())
        }
        _ => Err(LowerError::UnsupportedExpression(format!(
            "unsupported array initializer: {:?}",
            ctx.func.expressions[init_h]
        ))),
    }
}

/// Flatten `{a,b}` or nested `{{a,b},{c,d}}` into leaf initializer expressions (row-major).
/// Only flattens for multi-dimensional arrays where inner components represent sub-arrays.
/// For 1D arrays of vectors/matrices, preserves components as leaf elements.
fn collect_flat_compose_components(
    func: &Function,
    init_h: Handle<Expression>,
    depth: usize,
) -> Result<Vec<Handle<Expression>>, LowerError> {
    match &func.expressions[init_h] {
        Expression::Compose { components, .. } => {
            if components.is_empty() {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "empty array initializer list",
                )));
            }
            // If any component is not Compose, we're at leaf level - don't flatten.
            let any_non_compose = components
                .iter()
                .any(|&c| !matches!(&func.expressions[c], Expression::Compose { .. }));
            if any_non_compose || depth == 0 {
                // Leaf level: components are the final elements (scalars or composite types)
                Ok(components.iter().copied().collect())
            } else {
                // Inner level of multi-dimensional array: recursively flatten.
                let mut flat = Vec::new();
                for &c in components.iter() {
                    flat.extend(collect_flat_compose_components(func, c, depth - 1)?);
                }
                Ok(flat)
            }
        }
        _ => Err(LowerError::UnsupportedExpression(String::from(
            "expected `{ ... }` array initializer",
        ))),
    }
}

/// Naga's GLSL front-end emits `.length()` on multi-dim arrays as `dimensions[0]` (type-tree outer);
/// GLSL uses the leftmost `[]` size, which matches `dimensions[last]` in our shape walk.
pub(crate) fn scan_naga_multidim_array_length_literals(
    func: &Function,
    aggregate_map: &BTreeMap<Handle<LocalVariable>, AggregateInfo>,
) -> BTreeMap<Handle<Expression>, i32> {
    let mut fixes = BTreeMap::new();
    let entries: Vec<(usize, Handle<Expression>, &Expression)> = func
        .expressions
        .iter()
        .map(|(h, e)| (h.index(), h, e))
        .collect();
    for w in entries.windows(2) {
        let (i0, _h0, e0) = w[0];
        let (i1, h1, e1) = w[1];
        if i0 + 1 != i1 {
            continue;
        }
        let Expression::Load { pointer } = e0 else {
            continue;
        };
        let Expression::LocalVariable(lv) = &func.expressions[*pointer] else {
            continue;
        };
        let Some(info) = aggregate_map.get(lv) else {
            continue;
        };
        if !matches!(
            &info.layout.kind,
            crate::naga_util::AggregateKind::Array { .. }
        ) {
            continue;
        }
        if info.dimensions().len() < 2 {
            continue;
        }
        // With dimensions in GLSL order (outer first):
        // - Naga emits the inner dimension (what used to be dimensions[0] in Naga's type tree)
        // - GLSL wants the outer dimension (dimensions[0] in our representation)
        let glsl_outer = info.dimensions()[0];
        let naga_emitted = *info.dimensions().last().expect("dims");
        if glsl_outer == naga_emitted {
            continue;
        }
        let wrong = naga_emitted;
        let correct = glsl_outer;
        let Expression::Literal(Literal::U32(n)) = e1 else {
            continue;
        };
        if *n != wrong {
            continue;
        }
        fixes.insert(h1, correct as i32);
        let mut next_idx = i1 + 1;
        while next_idx < func.expressions.len() {
            let mut hit = None;
            for (h, e) in func.expressions.iter() {
                if h.index() == next_idx {
                    hit = Some((h, e));
                    break;
                }
            }
            let Some((h, e)) = hit else {
                break;
            };
            match e {
                Expression::Literal(Literal::I32(v)) if *v == wrong as i32 => {
                    fixes.insert(h, correct as i32);
                    next_idx += 1;
                }
                _ => break,
            }
        }
    }
    fixes
}

pub(crate) fn peel_array_local_value(
    func: &Function,
    expr: Handle<Expression>,
) -> Option<Handle<LocalVariable>> {
    match &func.expressions[expr] {
        Expression::LocalVariable(lv) => Some(*lv),
        Expression::Load { pointer } => match &func.expressions[*pointer] {
            Expression::LocalVariable(lv) => Some(*lv),
            _ => None,
        },
        _ => None,
    }
}

/// GLSL `.length()` / [`Expression::ArrayLength`]: size of the leftmost `[]` for the array value.
/// Copy one stack-slot array to another (same shape); used for whole-array assignment.
pub(crate) fn copy_stack_array_slots(
    ctx: &mut LowerCtx<'_>,
    dst: &AggregateInfo,
    src: &AggregateInfo,
) -> Result<(), LowerError> {
    if dst.element_count() != src.element_count()
        || dst.leaf_stride() != src.leaf_stride()
        || dst.leaf_element_ty() != src.leaf_element_ty()
    {
        return Err(LowerError::UnsupportedStatement(String::from(
            "array copy: shape mismatch",
        )));
    }
    debug_assert_eq!(
        dst.align(),
        src.align(),
        "array copy: std430 alignment must match for identical array shapes"
    );
    let (AggregateSlot::Local(dst_slot), AggregateSlot::Local(src_slot)) = (dst.slot, src.slot)
    else {
        return Err(LowerError::Internal(format!(
            "array copy: expected two local stack slots (stack↔stack memcpy); \
             got dst={:?} src={:?} (Param/ParamReadOnly cannot participate)",
            dst.slot, src.slot
        )));
    };
    let sz = dst.total_size();
    let dst_addr = array_slot_base(ctx, dst_slot);
    let src_addr = array_slot_base(ctx, src_slot);
    ctx.fb.push(LpirOp::Memcpy {
        dst_addr,
        src_addr,
        size: sz,
    });
    Ok(())
}

pub(crate) fn lower_array_length(
    ctx: &mut LowerCtx<'_>,
    array_expr: Handle<Expression>,
) -> Result<VRegVec, LowerError> {
    let lv = peel_array_local_value(ctx.func, array_expr).ok_or_else(|| {
        LowerError::UnsupportedExpression(String::from(
            "ArrayLength: expected local array (pointer)",
        ))
    })?;
    let info = ctx.aggregate_map.get(&lv).ok_or_else(|| {
        LowerError::UnsupportedExpression(String::from("ArrayLength: not a stack-slot array local"))
    })?;
    let len = *info.dimensions().first().expect("array dimensions") as i32;
    let dst = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::IconstI32 { dst, value: len });
    Ok(smallvec![dst])
}

pub(crate) fn lower_array_equality_vec(
    ctx: &mut LowerCtx<'_>,
    op: BinaryOperator,
    left: Handle<Expression>,
    right: Handle<Expression>,
) -> Result<VRegVec, LowerError> {
    if !matches!(op, BinaryOperator::Equal | BinaryOperator::NotEqual) {
        return Err(LowerError::Internal(String::from(
            "lower_array_equality_vec: not equality",
        )));
    }
    let ll = peel_array_local_value(ctx.func, left).ok_or_else(|| {
        LowerError::UnsupportedExpression(String::from("array ==: expected local array value"))
    })?;
    let rr = peel_array_local_value(ctx.func, right).ok_or_else(|| {
        LowerError::UnsupportedExpression(String::from("array ==: expected local array value"))
    })?;
    let il = ctx.aggregate_map.get(&ll).cloned().ok_or_else(|| {
        LowerError::UnsupportedExpression(String::from("array ==: left not a stack array"))
    })?;
    let ir = ctx.aggregate_map.get(&rr).cloned().ok_or_else(|| {
        LowerError::UnsupportedExpression(String::from("array ==: right not a stack array"))
    })?;
    if il.element_count() != ir.element_count() || il.leaf_element_ty() != ir.leaf_element_ty() {
        return Err(LowerError::UnsupportedExpression(String::from(
            "array ==: shape mismatch",
        )));
    }
    let leaf_inner = &ctx.module.types[il.leaf_element_ty()].inner;
    let n = il.element_count();
    if n == 0 {
        let v = ctx.fb.alloc_vreg(IrType::I32);
        let val = match op {
            BinaryOperator::Equal => 1,
            BinaryOperator::NotEqual => 0,
            _ => 0,
        };
        ctx.fb.push(LpirOp::IconstI32 { dst: v, value: val });
        return Ok(smallvec![v]);
    }
    let mut acc: Option<VReg> = None;
    for i in 0..n {
        let lvs = load_array_element_const(ctx, &il, i)?;
        let rvs = load_array_element_const(ctx, &ir, i)?;
        let cmp = compare_leaf_elements(ctx, op, &lvs, &rvs, leaf_inner)?;
        acc = Some(match acc {
            None => cmp,
            Some(a) => match op {
                BinaryOperator::Equal => emit_i32_and(ctx, a, cmp)?,
                BinaryOperator::NotEqual => emit_i32_or(ctx, a, cmp)?,
                _ => return Err(LowerError::Internal(String::from("array eq op"))),
            },
        });
    }
    Ok(smallvec![acc.expect("non-empty fold")])
}

fn emit_i32_and(ctx: &mut LowerCtx<'_>, a: VReg, b: VReg) -> Result<VReg, LowerError> {
    let dst = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Iand {
        dst,
        lhs: a,
        rhs: b,
    });
    Ok(dst)
}

fn emit_i32_or(ctx: &mut LowerCtx<'_>, a: VReg, b: VReg) -> Result<VReg, LowerError> {
    let dst = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Ior {
        dst,
        lhs: a,
        rhs: b,
    });
    Ok(dst)
}

fn compare_leaf_elements(
    ctx: &mut LowerCtx<'_>,
    array_op: BinaryOperator,
    left: &VRegVec,
    right: &VRegVec,
    leaf_inner: &TypeInner,
) -> Result<VReg, LowerError> {
    let elem_op = array_op;
    match *leaf_inner {
        TypeInner::Scalar(scalar) => {
            if left.len() != 1 || right.len() != 1 {
                return Err(LowerError::Internal(String::from(
                    "scalar leaf width mismatch",
                )));
            }
            scalar_cmp_vreg(ctx, elem_op, left[0], right[0], scalar.kind)
        }
        TypeInner::Vector { size, scalar } => {
            let n = vector_size_usize(size);
            if left.len() != n || right.len() != n {
                return Err(LowerError::Internal(String::from(
                    "vector leaf width mismatch",
                )));
            }
            let mut acc: Option<VReg> = None;
            for j in 0..n {
                let c = scalar_cmp_vreg(ctx, elem_op, left[j], right[j], scalar.kind)?;
                acc = Some(match acc {
                    None => c,
                    Some(a) => match array_op {
                        BinaryOperator::Equal => emit_i32_and(ctx, a, c)?,
                        BinaryOperator::NotEqual => emit_i32_or(ctx, a, c)?,
                        _ => {
                            return Err(LowerError::Internal(String::from(
                                "compare_leaf vector fold",
                            )));
                        }
                    },
                });
            }
            Ok(acc.expect("vector compare"))
        }
        TypeInner::Matrix {
            columns,
            rows,
            scalar,
        } => {
            let n = vector_size_usize(columns) * vector_size_usize(rows);
            if left.len() != n || right.len() != n {
                return Err(LowerError::Internal(String::from(
                    "matrix leaf width mismatch",
                )));
            }
            let mut acc: Option<VReg> = None;
            for j in 0..n {
                let c = scalar_cmp_vreg(ctx, elem_op, left[j], right[j], scalar.kind)?;
                acc = Some(match acc {
                    None => c,
                    Some(a) => match array_op {
                        BinaryOperator::Equal => emit_i32_and(ctx, a, c)?,
                        BinaryOperator::NotEqual => emit_i32_or(ctx, a, c)?,
                        _ => {
                            return Err(LowerError::Internal(String::from(
                                "compare_leaf matrix fold",
                            )));
                        }
                    },
                });
            }
            Ok(acc.expect("matrix compare"))
        }
        _ => Err(LowerError::UnsupportedExpression(format!(
            "array ==: unsupported leaf {leaf_inner:?}"
        ))),
    }
}

fn scalar_cmp_vreg(
    ctx: &mut LowerCtx<'_>,
    op: BinaryOperator,
    lhs: VReg,
    rhs: VReg,
    kind: ScalarKind,
) -> Result<VReg, LowerError> {
    let dst = ctx.fb.alloc_vreg(IrType::I32);
    match kind {
        ScalarKind::Float => match op {
            BinaryOperator::Equal => ctx.fb.push(LpirOp::Feq { dst, lhs, rhs }),
            BinaryOperator::NotEqual => ctx.fb.push(LpirOp::Fne { dst, lhs, rhs }),
            _ => {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "scalar_cmp float op",
                )));
            }
        },
        ScalarKind::Sint | ScalarKind::Uint | ScalarKind::Bool => match op {
            BinaryOperator::Equal => ctx.fb.push(LpirOp::Ieq { dst, lhs, rhs }),
            BinaryOperator::NotEqual => ctx.fb.push(LpirOp::Ine { dst, lhs, rhs }),
            _ => {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "scalar_cmp int/bool op",
                )));
            }
        },
        ScalarKind::AbstractInt | ScalarKind::AbstractFloat => {
            return Err(LowerError::UnsupportedType(String::from(
                "abstract in array compare",
            )));
        }
    }
    Ok(dst)
}
