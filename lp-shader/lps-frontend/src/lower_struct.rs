//! Stack-slot struct locals: member loads, zero-fill, temp materialization.

use alloc::format;
use alloc::string::String;

use smallvec::{SmallVec, smallvec};

use lpir::{FunctionBuilder, IrType, LpirOp, SlotId, VMCTX_VREG, VReg};
use lps_shared::{LayoutRules, LpsType, array_stride};
use naga::{
    ArraySize, Expression, Function, GlobalVariable, Handle, LocalVariable, Module, Type,
    TypeInner, VectorSize,
};

use crate::lower_array::{
    ElementIndex, aggregate_storage_base_vreg, array_element_address_with_field_offset,
};
use crate::lower_array_multidim::{
    ArraySubscriptRoot, SubscriptOperand, peel_array_subscript_chain,
};
use crate::lower_ctx::{
    AggregateInfo, AggregateSlot, LowerCtx, VRegVec,
    debug_assert_not_param_readonly_aggregate_store, naga_type_to_ir_types,
};
use crate::lower_error::LowerError;
use crate::naga_util::MemberInfo;
use alloc::vec::Vec;

/// Local array-of-struct access after Phase 1 layout: array subscripts (outer-first in
/// [`SubscriptOperand`] order) and at most one struct field index (`ps[i].x` → `Some(x)`).
#[derive(Clone, Debug)]
pub(crate) struct ArrayOfStructChain {
    pub info: AggregateInfo,
    pub subscripts: SmallVec<[SubscriptOperand; 4]>,
    pub first_member: Option<u32>,
    /// Byte offset from the local slot base to the array (field in an outer struct local).
    pub field_base_offset: u32,
}

/// Peel `ps[i]`, `ps[0].x`, `ps[i].x`, and `c.ps[i].x` (array field inside a struct local).
pub(crate) fn peel_arrayofstruct_chain(
    ctx: &LowerCtx<'_>,
    expr: Handle<Expression>,
) -> Option<ArrayOfStructChain> {
    let func = ctx.func;
    let (array_expr, first_member) = match &func.expressions[expr] {
        Expression::AccessIndex { base, index } => match &func.expressions[*base] {
            Expression::LocalVariable(_) => (expr, None),
            _ => (*base, Some(*index)),
        },
        _ => (expr, None),
    };
    // Naga may wrap the array subscript in `Load` before a struct `AccessIndex` (e.g. `ps[0].x`).
    let array_expr = match &func.expressions[array_expr] {
        Expression::Load { pointer } => *pointer,
        _ => array_expr,
    };
    let (root, subscripts) = peel_array_subscript_chain(func, array_expr)?;
    let (info, subscripts, field_base_offset) = match root {
        ArraySubscriptRoot::Local(lv) => {
            let outer = ctx.aggregate_map.get(&lv).cloned()?;
            match &outer.layout.kind {
                crate::naga_util::AggregateKind::Array { .. } => (outer, subscripts, 0u32),
                crate::naga_util::AggregateKind::Struct { .. } => {
                    let (off, array_naga_ty, rest) = struct_path_prefix_to_array_of_struct(
                        ctx.module,
                        outer.naga_ty,
                        &subscripts,
                    )?;
                    let field_info = aggregate_info_for_array_naga_in_struct_slot(
                        ctx.module,
                        array_naga_ty,
                        outer.slot,
                    )?;
                    (field_info, rest, off)
                }
            }
        }
        // `inout` / `out` / pointer param (see [`LowerCtx::pointer_args`]) — not by-value `in` (`ParamReadOnly` uses `Local`).
        ArraySubscriptRoot::Param(arg_i) => {
            let ainfo = ctx
                .aggregate_info_for_subscript_root(ArraySubscriptRoot::Param(arg_i))
                .ok()
                .flatten()?;
            if !matches!(
                &ainfo.layout.kind,
                crate::naga_util::AggregateKind::Array { .. }
            ) {
                return None;
            }
            (ainfo, subscripts, 0u32)
        }
        _ => return None,
    };
    let leaf = info.leaf_element_ty();
    if !matches!(&ctx.module.types[leaf].inner, TypeInner::Struct { .. }) {
        return None;
    }
    if subscripts.len() != info.dimensions().len() {
        return None;
    }
    Some(ArrayOfStructChain {
        info,
        subscripts: subscripts.into(),
        first_member,
        field_base_offset,
    })
}

/// Leading `AccessIndex` consts in `subscripts` select struct fields; the remainder index the array.
fn struct_path_prefix_to_array_of_struct(
    module: &Module,
    mut naga_ty: Handle<Type>,
    subscripts: &[SubscriptOperand],
) -> Option<(u32, Handle<Type>, Vec<SubscriptOperand>)> {
    use SubscriptOperand::Const;
    let mut off = 0u32;
    let mut i = 0usize;
    while i < subscripts.len() {
        match &module.types[naga_ty].inner {
            TypeInner::Struct { .. } => {
                let Const(field) = &subscripts[i] else {
                    return None;
                };
                let layout = crate::naga_util::aggregate_layout(module, naga_ty).ok()??;
                let m = layout.struct_members()?.get(*field as usize)?;
                off = off.checked_add(m.byte_offset)?;
                naga_ty = m.naga_ty;
                i += 1;
            }
            TypeInner::Array { .. } => {
                let mut cur = naga_ty;
                loop {
                    match &module.types[cur].inner {
                        TypeInner::Array { base, .. } => cur = *base,
                        _ => break,
                    }
                }
                if !matches!(&module.types[cur].inner, TypeInner::Struct { .. }) {
                    return None;
                }
                return Some((off, naga_ty, subscripts[i..].to_vec()));
            }
            _ => return None,
        }
    }
    None
}

fn aggregate_info_for_array_naga_in_struct_slot(
    module: &Module,
    naga_array_ty: Handle<Type>,
    slot: AggregateSlot,
) -> Option<AggregateInfo> {
    let layout = crate::naga_util::aggregate_layout(module, naga_array_ty).ok()??;
    if !matches!(&layout.kind, crate::naga_util::AggregateKind::Array { .. }) {
        return None;
    }
    Some(AggregateInfo {
        slot,
        layout,
        naga_ty: naga_array_ty,
    })
}

fn subscripts_to_array_element_index(
    ctx: &mut LowerCtx<'_>,
    info: &AggregateInfo,
    subscripts: &[SubscriptOperand],
) -> Result<ElementIndex, LowerError> {
    if subscripts
        .iter()
        .all(|o| matches!(o, SubscriptOperand::Const(_)))
    {
        let idxs: alloc::vec::Vec<u32> = subscripts
            .iter()
            .map(|o| match o {
                SubscriptOperand::Const(c) => *c,
                SubscriptOperand::Dynamic(_) => 0,
            })
            .collect();
        let flat = crate::lower_array_multidim::flat_index_const_clamped(info.dimensions(), &idxs)?;
        Ok(ElementIndex::Const(flat))
    } else {
        let v = crate::lower_array::emit_row_major_flat_from_operands(
            ctx,
            info.dimensions(),
            subscripts,
        )?;
        Ok(ElementIndex::Dynamic(v))
    }
}

pub(crate) fn load_array_struct_element(
    ctx: &mut LowerCtx<'_>,
    chain: &ArrayOfStructChain,
) -> Result<VRegVec, LowerError> {
    let arr_index = subscripts_to_array_element_index(ctx, &chain.info, &chain.subscripts)?;
    let elem_addr = array_element_address_with_field_offset(
        ctx,
        &chain.info,
        arr_index,
        chain.field_base_offset,
    )?;
    let leaf_naga = chain.info.leaf_element_ty();
    if let Some(midx) = chain.first_member {
        let layout =
            crate::naga_util::aggregate_layout(ctx.module, leaf_naga)?.ok_or_else(|| {
                LowerError::Internal(String::from("load_array_struct_element: leaf layout"))
            })?;
        let members = layout.struct_members().ok_or_else(|| {
            LowerError::Internal(String::from("load_array_struct_element: members"))
        })?;
        let m = members.get(midx as usize).ok_or_else(|| {
            LowerError::UnsupportedExpression(String::from(
                "load_array_struct_element: member index OOB",
            ))
        })?;
        if m.ir_tys.is_empty() {
            return load_struct_value_vregs_from_base(ctx, elem_addr, m.byte_offset, m.naga_ty);
        }
        let mut out = VRegVec::new();
        for (j, ty) in m.ir_tys.iter().enumerate() {
            let dst = ctx.fb.alloc_vreg(*ty);
            let off = m.byte_offset.checked_add((j as u32) * 4).ok_or_else(|| {
                LowerError::Internal(String::from("load_array_struct_element: off"))
            })?;
            ctx.fb.push(LpirOp::Load {
                dst,
                base: elem_addr,
                offset: off,
            });
            out.push(dst);
        }
        Ok(out)
    } else {
        load_struct_value_vregs_from_base(ctx, elem_addr, 0, leaf_naga)
    }
}

pub(crate) fn store_array_struct_element(
    ctx: &mut LowerCtx<'_>,
    chain: &ArrayOfStructChain,
    rhs: Handle<Expression>,
) -> Result<(), LowerError> {
    debug_assert_not_param_readonly_aggregate_store(&chain.info, "store_array_struct_element");
    let arr_index = subscripts_to_array_element_index(ctx, &chain.info, &chain.subscripts)?;
    let elem_addr = array_element_address_with_field_offset(
        ctx,
        &chain.info,
        arr_index,
        chain.field_base_offset,
    )?;
    let leaf_naga = chain.info.leaf_element_ty();
    if let Some(midx) = chain.first_member {
        let layout =
            crate::naga_util::aggregate_layout(ctx.module, leaf_naga)?.ok_or_else(|| {
                LowerError::Internal(String::from("store_array_struct_element: leaf layout"))
            })?;
        let members = layout.struct_members().ok_or_else(|| {
            LowerError::Internal(String::from("store_array_struct_element: members"))
        })?;
        let m = members.get(midx as usize).ok_or_else(|| {
            LowerError::UnsupportedStatement(String::from(
                "store_array_struct_element: member index OOB",
            ))
        })?;
        if m.ir_tys.is_empty() {
            let lps_ty = crate::lower_aggregate_layout::naga_to_lps_type(ctx.module, m.naga_ty)?;
            let sub =
                crate::naga_util::aggregate_layout(ctx.module, m.naga_ty)?.ok_or_else(|| {
                    LowerError::Internal(String::from("store_array_struct_element: nested layout"))
                })?;
            return crate::lower_aggregate_write::store_lps_value_into_slot(
                ctx,
                elem_addr,
                m.byte_offset,
                m.naga_ty,
                &lps_ty,
                rhs,
                Some(&sub),
            );
        }
        let naga_inner = &ctx.module.types[m.naga_ty].inner;
        let raw = ctx.ensure_expr_vec(rhs)?;
        let srcs = crate::lower_expr::coerce_assignment_vregs(ctx, None, naga_inner, rhs, raw)?;
        let ir_tys = naga_type_to_ir_types(ctx.module, naga_inner)?;
        if srcs.len() != ir_tys.len() {
            return Err(LowerError::UnsupportedStatement(format!(
                "array-of-struct member store: {} vs {} components",
                srcs.len(),
                ir_tys.len()
            )));
        }
        for (j, &s) in srcs.iter().enumerate() {
            ctx.fb.push(LpirOp::Store {
                base: elem_addr,
                offset: m.byte_offset + (j as u32) * 4,
                value: s,
            });
        }
        Ok(())
    } else {
        let lps_ty = crate::lower_aggregate_layout::naga_to_lps_type(ctx.module, leaf_naga)?;
        let leaf_layout =
            crate::naga_util::aggregate_layout(ctx.module, leaf_naga)?.ok_or_else(|| {
                LowerError::Internal(String::from("store_array_struct_element: leaf layout"))
            })?;
        if crate::lower_aggregate_write::try_memcpy_leaf_slot_into_addr(
            ctx, elem_addr, leaf_naga, rhs,
        )? {
            return Ok(());
        }
        let agg = if matches!(lps_ty, LpsType::Struct { .. }) {
            Some(&leaf_layout)
        } else {
            None
        };
        crate::lower_aggregate_write::store_lps_value_into_slot(
            ctx, elem_addr, 0, leaf_naga, &lps_ty, rhs, agg,
        )
    }
}

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
            let mem_inner = &ctx.module.types[m.naga_ty].inner;
            match mem_inner {
                TypeInner::Struct { .. } => {
                    naga_ty = m.naga_ty;
                    continue;
                }
                TypeInner::Array {
                    base: elem_ty_h,
                    size,
                    ..
                } => {
                    let n = match size {
                        ArraySize::Constant(nz) => nz.get(),
                        ArraySize::Pending(_) | ArraySize::Dynamic => {
                            return Err(LowerError::UnsupportedExpression(String::from(
                                "struct path: array member with non-constant size",
                            )));
                        }
                    };
                    if d >= indices.len() {
                        return Err(LowerError::Internal(String::from(
                            "struct path: array field requires element index",
                        )));
                    }
                    let elem_idx = indices[d].min(n.saturating_sub(1));
                    d += 1;
                    let lps_elem =
                        crate::lower_aggregate_layout::naga_to_lps_type(ctx.module, *elem_ty_h)?;
                    let stride = array_stride(&lps_elem, LayoutRules::Std430) as u32;
                    off = off.wrapping_add(elem_idx.saturating_mul(stride));
                    naga_ty = *elem_ty_h;
                    continue;
                }
                _ => {
                    return Err(LowerError::Internal(format!(
                        "struct path: aggregate member without ir_tys: {mem_inner:?}"
                    )));
                }
            }
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
    if let TypeInner::Pointer { base: inner, .. } = &ctx.module.types[naga_ty].inner {
        naga_ty = *inner;
    }
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
            let mem_inner = &ctx.module.types[m.naga_ty].inner;
            match mem_inner {
                TypeInner::Struct { .. } => {
                    naga_ty = m.naga_ty;
                    continue;
                }
                TypeInner::Array {
                    base: elem_ty_h,
                    size,
                    ..
                } => {
                    let n = match size {
                        ArraySize::Constant(nz) => nz.get(),
                        ArraySize::Pending(_) | ArraySize::Dynamic => {
                            return Err(LowerError::UnsupportedExpression(String::from(
                                "global struct path: array member with non-constant size",
                            )));
                        }
                    };
                    if d >= indices.len() {
                        return Err(LowerError::Internal(String::from(
                            "global struct path: array field requires element index",
                        )));
                    }
                    let elem_idx = indices[d].min(n.saturating_sub(1));
                    d += 1;
                    let lps_elem =
                        crate::lower_aggregate_layout::naga_to_lps_type(ctx.module, *elem_ty_h)?;
                    let stride = array_stride(&lps_elem, LayoutRules::Std430) as u32;
                    off = off.wrapping_add(elem_idx.saturating_mul(stride));
                    naga_ty = *elem_ty_h;
                    continue;
                }
                _ => {
                    return Err(LowerError::Internal(format!(
                        "global struct path: aggregate member without ir_tys: {mem_inner:?}"
                    )));
                }
            }
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
    debug_assert_not_param_readonly_aggregate_store(info, "store_struct_path_into_local");
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

/// Zero a struct value in `[base+offset, …)` using an explicit [`FunctionBuilder`] (e.g. array
/// zero-fill before a full [`LowerCtx`] exists).
pub(crate) fn zero_struct_at_offset_fb(
    fb: &mut FunctionBuilder,
    module: &Module,
    base: VReg,
    offset: u32,
    naga_ty: Handle<Type>,
) -> Result<(), LowerError> {
    zero_struct_region_in_slot(fb, module, base, offset, naga_ty)
}

/// Zero a struct value in `[base+offset, …)` (e.g. `ZeroValue` for a struct rvalue).
pub(crate) fn zero_struct_at_offset(
    ctx: &mut LowerCtx<'_>,
    base: VReg,
    offset: u32,
    naga_ty: Handle<Type>,
) -> Result<(), LowerError> {
    zero_struct_at_offset_fb(&mut ctx.fb, ctx.module, base, offset, naga_ty)
}

/// Zero-initialize a struct stack slot (used from [`crate::lower_ctx::LowerCtx::new`]).
pub(crate) fn zero_fill_struct_slot(
    fb: &mut FunctionBuilder,
    module: &Module,
    info: &AggregateInfo,
) -> Result<(), LowerError> {
    // `Param` / `ParamReadOnly` never use stack zero-fill; only `Local` slots in prologue.
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
        match &module.types[m.naga_ty].inner {
            TypeInner::Array { .. } => {
                crate::lower_array::zero_array_region_in_slot(fb, module, base, off, m.naga_ty)?;
            }
            TypeInner::Struct { .. } => {
                zero_struct_region_in_slot(fb, module, base, off, m.naga_ty)?;
            }
            _ => {
                return Err(LowerError::Internal(String::from(
                    "zero member: nested aggregate is not struct or array",
                )));
            }
        }
    }
    Ok(())
}

fn push_zero_for_ir_type(fb: &mut FunctionBuilder, dst: VReg, ty: IrType) {
    match ty {
        IrType::F32 => fb.push(LpirOp::FconstF32 { dst, value: 0.0 }),
        IrType::I32 | IrType::Pointer => fb.push(LpirOp::IconstI32 { dst, value: 0 }),
    }
}
