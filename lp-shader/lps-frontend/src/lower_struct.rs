//! Stack-slot struct locals: member loads, zero-fill, temp materialization.

use alloc::format;
use alloc::string::String;

use smallvec::smallvec;

use lpir::{FunctionBuilder, IrType, LpirOp, SlotId, VMCTX_VREG, VReg};
use naga::{
    Expression, Function, GlobalVariable, Handle, LocalVariable, Module, Type, TypeInner,
    VectorSize,
};

use crate::lower_array::aggregate_storage_base_vreg;
use crate::lower_ctx::{AggregateInfo, AggregateSlot, LowerCtx, VRegVec};
use crate::lower_error::LowerError;
use crate::naga_util::MemberInfo;

/// Walk `AccessIndex` (and an optional `Load`) chain to a [`LocalVariable`], collecting member indices
/// innermost-to-outermost → reversed to field order root → leaf.
pub(crate) fn peel_struct_access_index_chain_to_local(
    func: &Function,
    mut h: Handle<Expression>,
) -> Option<(Handle<LocalVariable>, alloc::vec::Vec<u32>)> {
    use alloc::vec::Vec;
    let mut idxs: Vec<u32> = Vec::new();
    loop {
        match &func.expressions[h] {
            Expression::AccessIndex { base, index } => {
                idxs.push(*index);
                h = *base;
            }
            Expression::LocalVariable(lv) => {
                idxs.reverse();
                return Some((*lv, idxs));
            }
            Expression::Load { pointer } => h = *pointer,
            _ => return None,
        }
    }
}

/// Like [`peel_struct_access_index_chain_to_local`], but the chain ends at a [`GlobalVariable`].
pub(crate) fn peel_struct_access_index_chain_to_global(
    func: &Function,
    mut h: Handle<Expression>,
) -> Option<(Handle<GlobalVariable>, alloc::vec::Vec<u32>)> {
    use alloc::vec::Vec;
    let mut idxs: Vec<u32> = Vec::new();
    loop {
        match &func.expressions[h] {
            Expression::AccessIndex { base, index } => {
                idxs.push(*index);
                h = *base;
            }
            Expression::GlobalVariable(gv) => {
                idxs.reverse();
                return Some((*gv, idxs));
            }
            Expression::Load { pointer } => h = *pointer,
            _ => return None,
        }
    }
}

fn vector_size(n: VectorSize) -> usize {
    crate::lower_ctx::vector_size_usize(n)
}

/// Apply `indices` to an already-flattened `vregs` list for a Naga `inner` (vector/matrix/scalar).
fn project_access_on_value(
    vregs: VRegVec,
    inner: &TypeInner,
    indices: &[u32],
) -> Result<VRegVec, LowerError> {
    let mut cur = vregs;
    let mut ty = inner.clone();
    for &i in indices {
        let i = i as usize;
        match &ty {
            TypeInner::Vector { size, .. } => {
                let n = vector_size(*size);
                if i >= n {
                    return Err(LowerError::UnsupportedExpression(String::from(
                        "struct path: vector index OOB",
                    )));
                }
                cur = smallvec![cur[i]];
                ty = match &ty {
                    TypeInner::Vector { scalar, .. } => TypeInner::Scalar(*scalar),
                    _ => unreachable!(),
                };
            }
            TypeInner::Matrix {
                columns,
                rows,
                scalar,
            } => {
                let ncols = vector_size(*columns);
                let nrows = vector_size(*rows);
                if i >= ncols {
                    return Err(LowerError::UnsupportedExpression(String::from(
                        "struct path: matrix column OOB",
                    )));
                }
                let col = i;
                let start = col * nrows;
                cur = cur[start..start + nrows].into();
                ty = TypeInner::Vector {
                    size: *rows,
                    scalar: *scalar,
                };
            }
            TypeInner::Scalar(_) => {
                return Err(LowerError::Internal(String::from(
                    "struct path: AccessIndex on scalar",
                )));
            }
            _ => {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "struct path: project on non-scalar/vec/mat",
                )));
            }
        }
    }
    Ok(cur)
}

/// Load a member (possibly nested) from a struct slot local using a member path `indices`.
pub(crate) fn load_struct_path_from_local(
    ctx: &mut LowerCtx<'_>,
    info: &AggregateInfo,
    indices: &[u32],
) -> Result<VRegVec, LowerError> {
    if !matches!(
        &info.layout.kind,
        crate::naga_util::AggregateKind::Struct { .. }
    ) {
        return Err(LowerError::Internal(String::from(
            "load_struct_path_from_local: not a struct",
        )));
    }
    if indices.is_empty() {
        return Err(LowerError::Internal(String::from(
            "load_struct_path_from_local: empty path",
        )));
    }
    let base = aggregate_storage_base_vreg(ctx, &info.slot)?;
    let mut naga_ty = info.naga_ty;
    let mut off = 0u32;
    let mut d = 0;
    while d < indices.len() {
        let layout = match crate::naga_util::aggregate_layout(ctx.module, naga_ty) {
            Ok(Some(l)) => l,
            Ok(None) | Err(_) => {
                return Err(LowerError::Internal(String::from("struct path: layout")));
            }
        };
        let members = layout
            .struct_members()
            .ok_or_else(|| LowerError::Internal(String::from("struct path: members")))?;
        let idx = indices[d] as usize;
        let m = members.get(idx).ok_or_else(|| {
            LowerError::UnsupportedExpression(String::from("struct path: index out of range"))
        })?;
        off = off
            .checked_add(m.byte_offset)
            .ok_or_else(|| LowerError::Internal(String::from("struct path: offset overflow")))?;
        d += 1;
        if m.ir_tys.is_empty() {
            naga_ty = m.naga_ty;
            continue;
        }
        // Load this member (leaf or vec/mat that may have sub-indices in `indices`).
        if d == indices.len() {
            let mut out = VRegVec::new();
            for (j, ty) in m.ir_tys.iter().enumerate() {
                let dst = ctx.fb.alloc_vreg(*ty);
                let joff = (j as u32)
                    .checked_mul(4)
                    .and_then(|b| off.checked_add(b))
                    .ok_or_else(|| LowerError::Internal(String::from("struct path: leaf off")))?;
                ctx.fb.push(LpirOp::Load {
                    dst,
                    base,
                    offset: joff,
                });
                out.push(dst);
            }
            return Ok(out);
        }
        // More indices: load full member, then project on vector/matrix (not nested struct in slot).
        let mem_inner = &ctx.module.types[m.naga_ty].inner;
        if matches!(mem_inner, TypeInner::Struct { .. }) {
            naga_ty = m.naga_ty;
            continue;
        }
        if m.ir_tys.is_empty() {
            return Err(LowerError::Internal(String::from(
                "struct path: non-struct leaf with tail indices",
            )));
        }
        let mut vregs = VRegVec::new();
        for (j, ty) in m.ir_tys.iter().enumerate() {
            let dst = ctx.fb.alloc_vreg(*ty);
            let joff = (j as u32)
                .checked_mul(4)
                .and_then(|b| off.checked_add(b))
                .ok_or_else(|| {
                    LowerError::Internal(String::from("struct path: load member off"))
                })?;
            ctx.fb.push(LpirOp::Load {
                dst,
                base,
                offset: joff,
            });
            vregs.push(dst);
        }
        return project_access_on_value(vregs, mem_inner, &indices[d..]);
    }
    Err(LowerError::Internal(String::from(
        "struct path: fallthrough",
    )))
}

/// Load a member path from a uniform or private global struct laid out in VMContext (std430).
pub(crate) fn load_struct_path_from_global(
    ctx: &mut LowerCtx<'_>,
    gv_handle: Handle<GlobalVariable>,
    indices: &[u32],
) -> Result<VRegVec, LowerError> {
    let ginfo = ctx.global_map.get(&gv_handle).cloned().ok_or_else(|| {
        LowerError::Internal(format!(
            "load_struct_path_from_global: {gv_handle:?} not in global_map"
        ))
    })?;
    let gv = &ctx.module.global_variables[gv_handle];
    let mut naga_ty = gv.ty;
    if !matches!(&ctx.module.types[naga_ty].inner, TypeInner::Struct { .. }) {
        return Err(LowerError::UnsupportedExpression(String::from(
            "global struct path: root not a struct",
        )));
    }
    if indices.is_empty() {
        return Err(LowerError::Internal(String::from(
            "load_struct_path_from_global: empty path",
        )));
    }
    let base = VMCTX_VREG;
    let mut off = ginfo.byte_offset;
    let mut d = 0usize;
    while d < indices.len() {
        let layout = match crate::naga_util::aggregate_layout(ctx.module, naga_ty) {
            Ok(Some(l)) => l,
            Ok(None) | Err(_) => {
                return Err(LowerError::Internal(String::from(
                    "global struct path: layout",
                )));
            }
        };
        let members = layout
            .struct_members()
            .ok_or_else(|| LowerError::Internal(String::from("global struct path: members")))?;
        let idx = indices[d] as usize;
        let m = members.get(idx).ok_or_else(|| {
            LowerError::UnsupportedExpression(String::from(
                "global struct path: index out of range",
            ))
        })?;
        off = off
            .checked_add(m.byte_offset)
            .ok_or_else(|| LowerError::Internal(String::from("global struct path: offset")))?;
        d += 1;
        if m.ir_tys.is_empty() {
            naga_ty = m.naga_ty;
            continue;
        }
        if d == indices.len() {
            let mut out = VRegVec::new();
            for (j, ty) in m.ir_tys.iter().enumerate() {
                let dst = ctx.fb.alloc_vreg(*ty);
                let joff = (j as u32)
                    .checked_mul(4)
                    .and_then(|b| off.checked_add(b))
                    .ok_or_else(|| {
                        LowerError::Internal(String::from("global struct path: leaf off"))
                    })?;
                ctx.fb.push(LpirOp::Load {
                    dst,
                    base,
                    offset: joff,
                });
                out.push(dst);
            }
            return Ok(out);
        }
        let mem_inner = &ctx.module.types[m.naga_ty].inner;
        if matches!(mem_inner, TypeInner::Struct { .. }) {
            naga_ty = m.naga_ty;
            continue;
        }
        if m.ir_tys.is_empty() {
            return Err(LowerError::Internal(String::from(
                "global struct path: non-struct leaf with tail indices",
            )));
        }
        let mut vregs = VRegVec::new();
        for (j, ty) in m.ir_tys.iter().enumerate() {
            let dst = ctx.fb.alloc_vreg(*ty);
            let joff = (j as u32)
                .checked_mul(4)
                .and_then(|b| off.checked_add(b))
                .ok_or_else(|| {
                    LowerError::Internal(String::from("global struct path: load member off"))
                })?;
            ctx.fb.push(LpirOp::Load {
                dst,
                base,
                offset: joff,
            });
            vregs.push(dst);
        }
        return project_access_on_value(vregs, mem_inner, &indices[d..]);
    }
    Err(LowerError::Internal(String::from(
        "global struct path: fallthrough",
    )))
}

/// Store `value` into a path rooted at a private global struct in VMContext.
pub(crate) fn store_struct_path_into_global(
    ctx: &mut LowerCtx<'_>,
    gv_handle: Handle<GlobalVariable>,
    indices: &[u32],
    value: Handle<Expression>,
) -> Result<(), LowerError> {
    let ginfo = ctx.global_map.get(&gv_handle).cloned().ok_or_else(|| {
        LowerError::Internal(format!(
            "store_struct_path_into_global: {gv_handle:?} not in global_map"
        ))
    })?;
    if ginfo.is_uniform {
        return Err(LowerError::UnsupportedStatement(String::from(
            "cannot write to uniform variable",
        )));
    }
    let gv = &ctx.module.global_variables[gv_handle];
    let mut naga_ty = gv.ty;
    if !matches!(&ctx.module.types[naga_ty].inner, TypeInner::Struct { .. }) {
        return Err(LowerError::UnsupportedStatement(String::from(
            "global store path: root not a struct",
        )));
    }
    let base = VMCTX_VREG;
    let mut off = ginfo.byte_offset;
    for (d, &idx) in indices.iter().enumerate() {
        let layout = crate::naga_util::aggregate_layout(ctx.module, naga_ty)?.ok_or_else(|| {
            LowerError::Internal(String::from("global store struct path: layout"))
        })?;
        let members = layout.struct_members().ok_or_else(|| {
            LowerError::Internal(String::from("global store struct path: members"))
        })?;
        let m = members.get(idx as usize).ok_or_else(|| {
            LowerError::UnsupportedStatement(String::from(
                "global struct store path: index out of range",
            ))
        })?;
        off = off.checked_add(m.byte_offset).ok_or_else(|| {
            LowerError::Internal(String::from("global struct store path: offset overflow"))
        })?;
        if d + 1 == indices.len() {
            if m.ir_tys.is_empty() {
                return Err(LowerError::UnsupportedStatement(String::from(
                    "global struct store path: nested aggregate assign",
                )));
            }
            let naga_inner = &ctx.module.types[m.naga_ty].inner;
            let raw = ctx.ensure_expr_vec(value)?;
            let srcs =
                crate::lower_expr::coerce_assignment_vregs(ctx, None, naga_inner, value, raw)?;
            let ir_tys = crate::naga_util::naga_type_to_ir_types(ctx.module, naga_inner)?;
            if srcs.len() != ir_tys.len() {
                return Err(LowerError::UnsupportedStatement(format!(
                    "global struct member store: {} vs {} components",
                    srcs.len(),
                    ir_tys.len()
                )));
            }
            for (j, &s) in srcs.iter().enumerate() {
                ctx.fb.push(LpirOp::Store {
                    base,
                    offset: off + (j as u32) * 4,
                    value: s,
                });
            }
            return Ok(());
        }
        naga_ty = m.naga_ty;
    }
    Err(LowerError::Internal(String::from(
        "global struct store path: empty path",
    )))
}

/// Store `value` into a struct path (only when the final member is scalar/vector/matrix, not a nested
/// struct aggregate).
pub(crate) fn store_struct_path_into_local(
    ctx: &mut LowerCtx<'_>,
    info: &AggregateInfo,
    indices: &[u32],
    value: Handle<Expression>,
) -> Result<(), LowerError> {
    if !matches!(
        &info.layout.kind,
        crate::naga_util::AggregateKind::Struct { .. }
    ) {
        return Err(LowerError::Internal(String::from(
            "store_struct_path: not a struct",
        )));
    }
    let base = aggregate_storage_base_vreg(ctx, &info.slot)?;
    let mut naga_ty = info.naga_ty;
    let mut off = 0u32;
    for (d, &idx) in indices.iter().enumerate() {
        let layout = crate::naga_util::aggregate_layout(ctx.module, naga_ty)?
            .ok_or_else(|| LowerError::Internal(String::from("store struct path: layout")))?;
        let members = layout
            .struct_members()
            .ok_or_else(|| LowerError::Internal(String::from("store struct path: members")))?;
        let m = members.get(idx as usize).ok_or_else(|| {
            LowerError::UnsupportedStatement(String::from("struct store path: index out of range"))
        })?;
        off = off.checked_add(m.byte_offset).ok_or_else(|| {
            LowerError::Internal(String::from("struct store path: offset overflow"))
        })?;
        if d + 1 == indices.len() {
            if m.ir_tys.is_empty() {
                return Err(LowerError::UnsupportedStatement(String::from(
                    "struct store path: nested aggregate assign not in phase 04",
                )));
            }
            let naga_inner = &ctx.module.types[m.naga_ty].inner;
            let raw = ctx.ensure_expr_vec(value)?;
            let srcs =
                crate::lower_expr::coerce_assignment_vregs(ctx, None, naga_inner, value, raw)?;
            let ir_tys = crate::naga_util::naga_type_to_ir_types(ctx.module, naga_inner)?;
            if srcs.len() != ir_tys.len() {
                return Err(LowerError::UnsupportedStatement(format!(
                    "struct member store: {} vs {} components",
                    srcs.len(),
                    ir_tys.len()
                )));
            }
            for (j, &s) in srcs.iter().enumerate() {
                ctx.fb.push(LpirOp::Store {
                    base,
                    offset: off + (j as u32) * 4,
                    value: s,
                });
            }
            return Ok(());
        }
        naga_ty = m.naga_ty;
    }
    Err(LowerError::Internal(String::from(
        "store struct path: empty path",
    )))
}

/// Load a full struct (std430 layout) as a flat vreg list from `base + base_off` (e.g. `inout` param).
pub(crate) fn load_struct_value_vregs_from_base(
    ctx: &mut LowerCtx<'_>,
    base: VReg,
    base_off: u32,
    naga_struct_ty: Handle<Type>,
) -> Result<VRegVec, LowerError> {
    let Some(layout) = crate::naga_util::aggregate_layout(ctx.module, naga_struct_ty)? else {
        return Err(LowerError::Internal(String::from(
            "load_struct_value: not an aggregate naga type",
        )));
    };
    let members = layout.struct_members().ok_or_else(|| {
        LowerError::Internal(String::from("load_struct_value: not a struct layout"))
    })?;
    let mut out = VRegVec::new();
    for m in members {
        if !m.ir_tys.is_empty() {
            for (j, ty) in m.ir_tys.iter().enumerate() {
                let dst = ctx.fb.alloc_vreg(*ty);
                let off = base_off
                    .checked_add(m.byte_offset)
                    .and_then(|o| o.checked_add((j as u32) * 4))
                    .ok_or_else(|| {
                        LowerError::Internal(String::from("load_struct_value: offset overflow"))
                    })?;
                ctx.fb.push(LpirOp::Load {
                    dst,
                    base,
                    offset: off,
                });
                out.push(dst);
            }
        } else {
            let next_off = base_off.checked_add(m.byte_offset).ok_or_else(|| {
                LowerError::Internal(String::from("load_struct_value: nested offset overflow"))
            })?;
            let sub = load_struct_value_vregs_from_base(ctx, base, next_off, m.naga_ty)?;
            out.extend_from_slice(&sub);
        }
    }
    Ok(out)
}

/// Load IR vregs for one struct member from a slot-backed struct.
pub(crate) fn load_struct_member_to_vregs(
    ctx: &mut LowerCtx<'_>,
    info: &AggregateInfo,
    member_idx: usize,
) -> Result<VRegVec, LowerError> {
    let members = info
        .layout
        .struct_members()
        .ok_or_else(|| LowerError::Internal(String::from("load_struct_member: not a struct")))?;
    let m = members.get(member_idx).ok_or_else(|| {
        LowerError::UnsupportedExpression(String::from("struct member index out of range for load"))
    })?;
    if m.ir_tys.is_empty() {
        return Err(LowerError::UnsupportedExpression(String::from(
            "struct member load: nested aggregate leaf needs recursive lowering",
        )));
    }
    let base = aggregate_storage_base_vreg(ctx, &info.slot)?;
    let mut out = VRegVec::new();
    for (j, ty) in m.ir_tys.iter().enumerate() {
        let dst = ctx.fb.alloc_vreg(*ty);
        ctx.fb.push(LpirOp::Load {
            dst,
            base,
            offset: m.byte_offset + (j as u32) * 4,
        });
        out.push(dst);
    }
    Ok(out)
}

/// Zero a struct value in `[base+offset, …)` (e.g. `ZeroValue` for a struct rvalue).
pub(crate) fn zero_struct_at_offset(
    ctx: &mut LowerCtx<'_>,
    base: VReg,
    offset: u32,
    naga_ty: Handle<Type>,
) -> Result<(), LowerError> {
    zero_struct_region_in_slot(&mut ctx.fb, ctx.module, base, offset, naga_ty)
}

/// Zero-initialize a struct stack slot (used from [`crate::lower_ctx::LowerCtx::new`]).
pub(crate) fn zero_fill_struct_slot(
    fb: &mut FunctionBuilder,
    module: &Module,
    info: &AggregateInfo,
) -> Result<(), LowerError> {
    let AggregateSlot::Local(slot) = info.slot else {
        return Err(LowerError::Internal(String::from(
            "zero_fill_struct_slot: not a local stack struct",
        )));
    };
    let base = struct_slot_base_fb(fb, slot);
    zero_struct_region_in_slot(fb, module, base, 0, info.naga_ty)
}

fn struct_slot_base_fb(fb: &mut FunctionBuilder, slot: SlotId) -> VReg {
    let local_addr = fb.alloc_vreg(IrType::Pointer);
    fb.push(LpirOp::SlotAddr {
        dst: local_addr,
        slot,
    });
    local_addr
}

/// Zero `[base+region_off, ...)` for the std430 region of Naga type `ty`.
pub(crate) fn zero_struct_region_in_slot(
    fb: &mut FunctionBuilder,
    module: &Module,
    base: VReg,
    region_off: u32,
    ty: Handle<Type>,
) -> Result<(), LowerError> {
    let Some(layout) = crate::naga_util::aggregate_layout(module, ty)? else {
        return Err(LowerError::Internal(String::from(
            "zero_struct_region: not an aggregate",
        )));
    };
    let members = layout.struct_members().ok_or_else(|| {
        LowerError::Internal(String::from("zero_struct_region: expected struct layout"))
    })?;
    for m in members {
        zero_member_prefix(fb, module, base, region_off, m)?;
    }
    Ok(())
}

fn zero_member_prefix(
    fb: &mut FunctionBuilder,
    module: &Module,
    base: VReg,
    base_off: u32,
    m: &MemberInfo,
) -> Result<(), LowerError> {
    let off = base_off
        .checked_add(m.byte_offset)
        .ok_or_else(|| LowerError::Internal(String::from("zero member: offset overflow")))?;
    if !m.ir_tys.is_empty() {
        for (j, ty) in m.ir_tys.iter().enumerate() {
            let z = fb.alloc_vreg(*ty);
            push_zero_for_ir_type(fb, z, *ty);
            fb.push(LpirOp::Store {
                base,
                offset: off + (j as u32) * 4,
                value: z,
            });
        }
    } else {
        zero_struct_region_in_slot(fb, module, base, off, m.naga_ty)?;
    }
    Ok(())
}

fn push_zero_for_ir_type(fb: &mut FunctionBuilder, dst: VReg, ty: IrType) {
    match ty {
        IrType::F32 => fb.push(LpirOp::FconstF32 { dst, value: 0.0 }),
        IrType::I32 | IrType::Pointer => fb.push(LpirOp::IconstI32 { dst, value: 0 }),
    }
}
