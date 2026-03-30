//! Stack-slot arrays: zero-fill, initializer lists, indexed load/store with bounds clamping.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lpir::{IrType, Op, VReg};
use naga::{Expression, Function, Handle, Module};

use crate::lower_ctx::{ArrayInfo, LowerCtx, VRegVec, naga_type_to_ir_types};
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
                ctx.fb.push(Op::IconstI32 {
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
        ctx.fb.push(Op::IconstI32 {
            dst: dim_v,
            value: dk as i32,
        });
        let prod = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(Op::Imul {
            dst: prod,
            lhs: acc,
            rhs: dim_v,
        });
        let ik = clamp_array_index(ctx, index_v[k], dk)?;
        let sum = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(Op::Iadd {
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
    ctx.fb.push(Op::IconstI32 {
        dst: zero,
        value: 0,
    });

    let len = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::IconstI32 {
        dst: len,
        value: element_count as i32,
    });

    let max_idx = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::IconstI32 {
        dst: max_idx,
        value: (element_count - 1) as i32,
    });

    let lt0 = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::IltS {
        dst: lt0,
        lhs: index_v,
        rhs: zero,
    });
    let after_low = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::Select {
        dst: after_low,
        cond: lt0,
        if_true: zero,
        if_false: index_v,
    });

    let ge_len = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::IgeU {
        dst: ge_len,
        lhs: after_low,
        rhs: len,
    });
    let out = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::Select {
        dst: out,
        cond: ge_len,
        if_true: max_idx,
        if_false: after_low,
    });
    Ok(out)
}

pub(crate) fn array_slot_base(ctx: &mut LowerCtx<'_>, slot: lpir::SlotId) -> VReg {
    array_slot_base_fb(&mut ctx.fb, slot)
}

fn array_slot_base_fb(fb: &mut lpir::FunctionBuilder, slot: lpir::SlotId) -> VReg {
    let base = fb.alloc_vreg(IrType::I32);
    fb.push(Op::SlotAddr { dst: base, slot });
    base
}

/// Zero-initialize every element (no `LowerCtx`; used from [`crate::lower_ctx::LowerCtx::new`]).
pub(crate) fn zero_fill_array_slot(
    fb: &mut lpir::FunctionBuilder,
    module: &Module,
    info: &ArrayInfo,
) -> Result<(), LowerError> {
    let elem_inner = &module.types[info.leaf_element_ty].inner;
    let ir_tys = naga_type_to_ir_types(elem_inner)?;
    let base = array_slot_base_fb(fb, info.slot);

    for i in 0..info.element_count {
        let byte_off = i.checked_mul(info.leaf_stride).ok_or_else(|| {
            LowerError::Internal(String::from("zero_fill_array_slot: stride overflow"))
        })?;
        for (j, ty) in ir_tys.iter().enumerate() {
            let z = fb.alloc_vreg(*ty);
            push_zero_for_ir_type(fb, z, *ty);
            fb.push(Op::Store {
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
    info: &ArrayInfo,
) -> Result<(), LowerError> {
    zero_fill_array_slot(&mut ctx.fb, module, info)
}

fn push_zero_for_ir_type(fb: &mut lpir::FunctionBuilder, dst: VReg, ty: IrType) {
    match ty {
        IrType::F32 => fb.push(Op::FconstF32 { dst, value: 0.0 }),
        IrType::I32 => fb.push(Op::IconstI32 { dst, value: 0 }),
    }
}

pub(crate) fn load_array_element_const(
    ctx: &mut LowerCtx<'_>,
    info: &ArrayInfo,
    index: u32,
) -> Result<VRegVec, LowerError> {
    if info.element_count == 0 {
        return Err(LowerError::Internal(String::from(
            "load_array_element_const: empty array",
        )));
    }
    // Match dynamic clamp: OOB constant indices clamp to the last element (see `clamp_array_index`).
    let index = index.min(info.element_count - 1);
    let elem_inner = &ctx.module.types[info.leaf_element_ty].inner;
    let ir_tys = naga_type_to_ir_types(elem_inner)?;
    let base = array_slot_base(ctx, info.slot);
    let byte_off = index
        .checked_mul(info.leaf_stride)
        .ok_or_else(|| LowerError::Internal(String::from("load_array_element_const: overflow")))?;
    let mut out = VRegVec::new();
    for (j, ty) in ir_tys.iter().enumerate() {
        let dst = ctx.fb.alloc_vreg(*ty);
        ctx.fb.push(Op::Load {
            dst,
            base,
            offset: byte_off + (j as u32) * 4,
        });
        out.push(dst);
    }
    Ok(out)
}

pub(crate) fn load_array_element_dynamic(
    ctx: &mut LowerCtx<'_>,
    info: &ArrayInfo,
    index_v: VReg,
) -> Result<VRegVec, LowerError> {
    let clamped = clamp_array_index(ctx, index_v, info.element_count)?;
    let stride_v = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::IconstI32 {
        dst: stride_v,
        value: info.leaf_stride as i32,
    });
    let byte_off = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::Imul {
        dst: byte_off,
        lhs: clamped,
        rhs: stride_v,
    });
    let base = array_slot_base(ctx, info.slot);
    let addr = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::Iadd {
        dst: addr,
        lhs: base,
        rhs: byte_off,
    });

    let elem_inner = &ctx.module.types[info.leaf_element_ty].inner;
    let ir_tys = naga_type_to_ir_types(elem_inner)?;
    let mut out = VRegVec::new();
    for (j, ty) in ir_tys.iter().enumerate() {
        let dst = ctx.fb.alloc_vreg(*ty);
        ctx.fb.push(Op::Load {
            dst,
            base: addr,
            offset: (j as u32) * 4,
        });
        out.push(dst);
    }
    Ok(out)
}

pub(crate) fn store_array_element_const(
    ctx: &mut LowerCtx<'_>,
    info: &ArrayInfo,
    index: u32,
    value_expr: Handle<Expression>,
) -> Result<(), LowerError> {
    if info.element_count == 0 {
        return Err(LowerError::Internal(String::from(
            "store_array_element_const: empty array",
        )));
    }
    let index = index.min(info.element_count - 1);
    let elem_inner = &ctx.module.types[info.leaf_element_ty].inner;
    let raw = ctx.ensure_expr_vec(value_expr)?;
    let srcs = coerce_assignment_vregs(ctx, elem_inner, value_expr, raw)?;
    let ir_tys = naga_type_to_ir_types(elem_inner)?;
    if srcs.len() != ir_tys.len() {
        return Err(LowerError::UnsupportedStatement(format!(
            "array element store: {} vs {} components",
            srcs.len(),
            ir_tys.len()
        )));
    }
    let base = array_slot_base(ctx, info.slot);
    let byte_off = index
        .checked_mul(info.leaf_stride)
        .ok_or_else(|| LowerError::Internal(String::from("store_array_element_const: overflow")))?;
    for (j, &src) in srcs.iter().enumerate() {
        ctx.fb.push(Op::Store {
            base,
            offset: byte_off + (j as u32) * 4,
            value: src,
        });
    }
    Ok(())
}

pub(crate) fn store_array_element_dynamic(
    ctx: &mut LowerCtx<'_>,
    info: &ArrayInfo,
    index_v: VReg,
    value_expr: Handle<Expression>,
) -> Result<(), LowerError> {
    let clamped = clamp_array_index(ctx, index_v, info.element_count)?;
    let stride_v = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::IconstI32 {
        dst: stride_v,
        value: info.leaf_stride as i32,
    });
    let byte_off = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::Imul {
        dst: byte_off,
        lhs: clamped,
        rhs: stride_v,
    });
    let base = array_slot_base(ctx, info.slot);
    let addr = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::Iadd {
        dst: addr,
        lhs: base,
        rhs: byte_off,
    });

    let elem_inner = &ctx.module.types[info.leaf_element_ty].inner;
    let raw = ctx.ensure_expr_vec(value_expr)?;
    let srcs = coerce_assignment_vregs(ctx, elem_inner, value_expr, raw)?;
    let ir_tys = naga_type_to_ir_types(elem_inner)?;
    if srcs.len() != ir_tys.len() {
        return Err(LowerError::UnsupportedStatement(format!(
            "array element store: {} vs {} components",
            srcs.len(),
            ir_tys.len()
        )));
    }
    for (j, &src) in srcs.iter().enumerate() {
        ctx.fb.push(Op::Store {
            base: addr,
            offset: (j as u32) * 4,
            value: src,
        });
    }
    Ok(())
}

pub(crate) fn lower_array_initializer(
    ctx: &mut LowerCtx<'_>,
    info: &ArrayInfo,
    init_h: Handle<Expression>,
) -> Result<(), LowerError> {
    match &ctx.func.expressions[init_h] {
        Expression::ZeroValue(_) => zero_fill_array(ctx, ctx.module, info),
        Expression::Compose { .. } => {
            // For multi-dimensional arrays, flatten nested Compose expressions.
            // Depth = dimensions.len() - 1 = number of nesting levels to flatten.
            let depth = info.dimensions.len().saturating_sub(1);
            let flat_components = collect_flat_compose_components(ctx.func, init_h, depth)?;
            if flat_components.len() as u32 > info.element_count {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "array initializer: too many elements",
                )));
            }
            let base = array_slot_base(ctx, info.slot);
            for (i, &comp) in flat_components.iter().enumerate() {
                let byte_off = (i as u32)
                    .checked_mul(info.leaf_stride)
                    .ok_or_else(|| LowerError::Internal(String::from("init: byte_off overflow")))?;
                store_element_at_byte_offset(ctx, info, base, byte_off, comp)?;
            }
            for i in flat_components.len() as u32..info.element_count {
                let byte_off = i.checked_mul(info.leaf_stride).ok_or_else(|| {
                    LowerError::Internal(String::from("init: tail byte_off overflow"))
                })?;
                zero_element_at_byte_offset(ctx, info, base, byte_off)?;
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

fn store_element_at_byte_offset(
    ctx: &mut LowerCtx<'_>,
    info: &ArrayInfo,
    base: VReg,
    byte_off: u32,
    expr: Handle<Expression>,
) -> Result<(), LowerError> {
    let elem_inner = &ctx.module.types[info.leaf_element_ty].inner;
    let raw = ctx.ensure_expr_vec(expr)?;
    let srcs = coerce_assignment_vregs(ctx, elem_inner, expr, raw)?;
    let ir_tys = naga_type_to_ir_types(elem_inner)?;
    if srcs.len() != ir_tys.len() {
        return Err(LowerError::UnsupportedStatement(format!(
            "array init element: {} vs {} components",
            srcs.len(),
            ir_tys.len()
        )));
    }
    for (j, &src) in srcs.iter().enumerate() {
        ctx.fb.push(Op::Store {
            base,
            offset: byte_off + (j as u32) * 4,
            value: src,
        });
    }
    Ok(())
}

fn zero_element_at_byte_offset(
    ctx: &mut LowerCtx<'_>,
    info: &ArrayInfo,
    base: VReg,
    byte_off: u32,
) -> Result<(), LowerError> {
    let elem_inner = &ctx.module.types[info.leaf_element_ty].inner;
    let ir_tys = naga_type_to_ir_types(elem_inner)?;
    for (j, ty) in ir_tys.iter().enumerate() {
        let z = ctx.fb.alloc_vreg(*ty);
        push_zero_for_ir_type(&mut ctx.fb, z, *ty);
        ctx.fb.push(Op::Store {
            base,
            offset: byte_off + (j as u32) * 4,
            value: z,
        });
    }
    Ok(())
}
