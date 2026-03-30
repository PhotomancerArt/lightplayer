//! Stack-slot arrays: zero-fill, initializer lists, indexed load/store with bounds clamping.

use alloc::format;
use alloc::string::String;

use lpir::{IrType, Op, VReg};
use naga::{Expression, Handle, Module};

use crate::lower_ctx::{ArrayInfo, LowerCtx, VRegVec, naga_type_to_ir_types};
use crate::lower_error::LowerError;
use crate::lower_expr::coerce_assignment_vregs;

/// Clamp dynamic index to `[0, element_count-1]` (v1 safety; see `docs/design/arrays.md`).
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
    let elem_inner = &module.types[info.element_ty].inner;
    let ir_tys = naga_type_to_ir_types(elem_inner)?;
    let base = array_slot_base_fb(fb, info.slot);

    for i in 0..info.element_count {
        let byte_off = i.checked_mul(info.stride).ok_or_else(|| {
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
    let elem_inner = &ctx.module.types[info.element_ty].inner;
    let ir_tys = naga_type_to_ir_types(elem_inner)?;
    let base = array_slot_base(ctx, info.slot);
    let byte_off = index
        .checked_mul(info.stride)
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
        value: info.stride as i32,
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

    let elem_inner = &ctx.module.types[info.element_ty].inner;
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
    let elem_inner = &ctx.module.types[info.element_ty].inner;
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
        .checked_mul(info.stride)
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
        value: info.stride as i32,
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

    let elem_inner = &ctx.module.types[info.element_ty].inner;
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
        Expression::Compose { components, .. } => {
            if components.len() as u32 > info.element_count {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "array initializer: too many elements",
                )));
            }
            let base = array_slot_base(ctx, info.slot);
            for (i, &comp) in components.iter().enumerate() {
                let byte_off = (i as u32)
                    .checked_mul(info.stride)
                    .ok_or_else(|| LowerError::Internal(String::from("init: byte_off overflow")))?;
                store_element_at_byte_offset(ctx, info, base, byte_off, comp)?;
            }
            for i in components.len() as u32..info.element_count {
                let byte_off = i.checked_mul(info.stride).ok_or_else(|| {
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

fn store_element_at_byte_offset(
    ctx: &mut LowerCtx<'_>,
    info: &ArrayInfo,
    base: VReg,
    byte_off: u32,
    expr: Handle<Expression>,
) -> Result<(), LowerError> {
    let elem_inner = &ctx.module.types[info.element_ty].inner;
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
    let elem_inner = &ctx.module.types[info.element_ty].inner;
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
