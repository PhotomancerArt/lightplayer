//! Writable `out` / `inout` call-actual resolution.
//!
//! Phase 1: bare [`Expression::LocalVariable`].
//! Phase 2: local vector/matrix [`Expression::Access`] / [`Expression::AccessIndex`] leaves and
//! single-component [`Expression::Swizzle`] on stack vectors (temp slot + post-call writeback).
//! Phase 3: local aggregate [`Access`] / [`AccessIndex`] chains (direct aggregate addresses plus
//! temp/writeback for scalar/vector/matrix leaves).
//! Phase 4: [`Expression::FunctionArgument`] roots in [`crate::lower_ctx::LowerCtx::pointer_args`].
//! Phase 5: [`Expression::GlobalVariable`] private globals (reject uniforms).

use alloc::format;
use alloc::string::String;

use lpir::{IrType, LpirOp, SlotId, VMCTX_VREG, VReg};
use naga::{Expression, GlobalVariable, Handle, LocalVariable, TypeInner};

use crate::lower_array::{ElementIndex, aggregate_storage_base_vreg, array_element_address};
use crate::lower_array_multidim::ArraySubscriptRoot;
use crate::lower_ctx::{AggregateSlot, LowerCtx, naga_type_to_ir_types};
use crate::lower_error::LowerError;

/// Naga types some `Access`/`AccessIndex` leaves as [`TypeInner::ValuePointer`]; LPIR lowering uses
/// plain scalar/vector [`TypeInner`] for flat vregs.
fn value_type_for_writable_leaf(inner: TypeInner) -> TypeInner {
    match inner {
        TypeInner::ValuePointer {
            size: None, scalar, ..
        } => TypeInner::Scalar(scalar),
        TypeInner::ValuePointer {
            size: Some(sz),
            scalar,
            ..
        } => TypeInner::Vector { size: sz, scalar },
        o => o,
    }
}

fn pointee_is_aggregate(ty: &TypeInner) -> bool {
    let v = value_type_for_writable_leaf(ty.clone());
    matches!(v, TypeInner::Struct { .. } | TypeInner::Array { .. })
}

pub(crate) struct WritableActual {
    pub(crate) addr: VReg,
    pub(crate) writeback: Option<WritableWriteback>,
}

pub(crate) enum WritableWriteback {
    LocalFlat {
        local: Handle<LocalVariable>,
        slot: SlotId,
    },
    /// Post-call VMContext store after temp slot (`lower_stmt`-style bare [`GlobalVariable`] writes).
    GlobalFlat {
        gv: Handle<GlobalVariable>,
        slot: SlotId,
    },
    /// Post-call copy from `slot` through a local access path ([`apply_writable_writeback`]).
    AccessExpr {
        pointer: Handle<Expression>,
        slot: SlotId,
    },
}

fn unwrap_load_to_pointer(mut expr: Handle<Expression>, ctx: &LowerCtx<'_>) -> Handle<Expression> {
    while let Expression::Load { pointer } = &ctx.func.expressions[expr] {
        expr = *pointer;
    }
    expr
}

fn peel_to_local_variable_root(
    mut ptr: Handle<Expression>,
    ctx: &LowerCtx<'_>,
) -> Option<Handle<LocalVariable>> {
    loop {
        match &ctx.func.expressions[ptr] {
            Expression::Access { base, .. } | Expression::AccessIndex { base, .. } => {
                ptr = *base;
            }
            Expression::Swizzle { vector, .. } => {
                ptr = *vector;
            }
            Expression::LocalVariable(lv) => return Some(*lv),
            Expression::Load { pointer } => ptr = *pointer,
            _ => return None,
        }
    }
}

/// Walk to a pointer-formal [`Expression::FunctionArgument`] (see [`LowerCtx::pointer_args`]).
fn peel_to_global_variable_root(
    mut ptr: Handle<Expression>,
    ctx: &LowerCtx<'_>,
) -> Option<Handle<GlobalVariable>> {
    loop {
        match &ctx.func.expressions[ptr] {
            Expression::Access { base, .. } | Expression::AccessIndex { base, .. } => {
                ptr = *base;
            }
            Expression::Swizzle { vector, .. } => {
                ptr = *vector;
            }
            Expression::GlobalVariable(gv) => return Some(*gv),
            Expression::Load { pointer } => ptr = *pointer,
            _ => return None,
        }
    }
}

fn peel_to_pointer_arg_root(mut ptr: Handle<Expression>, ctx: &LowerCtx<'_>) -> Option<u32> {
    loop {
        match &ctx.func.expressions[ptr] {
            Expression::Access { base, .. } | Expression::AccessIndex { base, .. } => {
                ptr = *base;
            }
            Expression::Swizzle { vector, .. } => {
                ptr = *vector;
            }
            Expression::FunctionArgument(arg_i) if ctx.pointer_args.contains_key(arg_i) => {
                return Some(*arg_i);
            }
            Expression::Load { pointer } => ptr = *pointer,
            _ => return None,
        }
    }
}

fn types_compatible_for_inout_actual(
    ctx: &LowerCtx<'_>,
    actual: Handle<Expression>,
    pointee_ty: Handle<naga::Type>,
) -> Result<(), LowerError> {
    let actual_inner = value_type_for_writable_leaf(crate::naga_util::expr_type_inner(
        ctx.module, ctx.func, actual,
    )?);
    let pointee_inner = value_type_for_writable_leaf(ctx.module.types[pointee_ty].inner.clone());
    let a = naga_type_to_ir_types(ctx.module, &actual_inner)?;
    let p = naga_type_to_ir_types(ctx.module, &pointee_inner)?;
    if a != p {
        return Err(LowerError::UnsupportedExpression(String::from(
            "inout/out actual value type does not match pointer pointee type",
        )));
    }
    Ok(())
}

/// Rejects uniform-instance locals and deferred uniform indexing paths writable `out` / `inout` cannot target.
fn reject_uniform_derived_writable_actual(
    ctx: &LowerCtx<'_>,
    ptr: Handle<Expression>,
) -> Result<(), LowerError> {
    let mut cur = ptr;
    loop {
        if ctx.uniform_vmctx_deferred.contains_key(&cur.index()) {
            return Err(LowerError::UnsupportedExpression(String::from(
                "cannot write to uniform variable",
            )));
        }
        match &ctx.func.expressions[cur] {
            Expression::LocalVariable(lv) => {
                if ctx.uniform_instance_locals.contains_key(lv) {
                    return Err(LowerError::UnsupportedExpression(String::from(
                        "cannot write to uniform variable",
                    )));
                }
                break;
            }
            Expression::GlobalVariable(gv) => {
                if ctx.global_map.get(gv).is_some_and(|g| g.is_uniform) {
                    return Err(LowerError::UnsupportedExpression(String::from(
                        "cannot write to uniform variable",
                    )));
                }
                break;
            }
            Expression::Access { base, .. } | Expression::AccessIndex { base, .. } => cur = *base,
            Expression::Swizzle { vector, .. } => cur = *vector,
            Expression::Load { pointer } => cur = *pointer,
            _ => break,
        }
    }
    Ok(())
}

fn global_unwrapped_root_naga_ty(
    ctx: &LowerCtx<'_>,
    gv: Handle<GlobalVariable>,
) -> Handle<naga::Type> {
    let mut ty = ctx.module.global_variables[gv].ty;
    while let TypeInner::Pointer { base: inner, .. } = &ctx.module.types[ty].inner {
        ty = *inner;
    }
    ty
}

fn try_resolve_global_aggregate_access(
    ctx: &mut LowerCtx<'_>,
    ptr: Handle<Expression>,
    actual: Handle<Expression>,
    pointee_ty: Handle<naga::Type>,
) -> Result<Option<WritableActual>, LowerError> {
    let callee_pointee_ty = ctx.module.types[pointee_ty].inner.clone();
    if !pointee_is_aggregate(&callee_pointee_ty) {
        let ir_tys = naga_type_to_ir_types(ctx.module, &callee_pointee_ty)?;
        let slot = ctx.fb.alloc_slot(ir_tys.len() as u32 * 4);
        let addr = ctx.fb.alloc_vreg(IrType::Pointer);
        ctx.fb.push(LpirOp::SlotAddr { dst: addr, slot });
        let cur = ctx.ensure_expr_vec(actual)?;
        if cur.len() != ir_tys.len() {
            return Err(LowerError::Internal(format!(
                "writable global aggregate leaf temp: expected {} rvalue components, got {}",
                ir_tys.len(),
                cur.len()
            )));
        }
        for (j, &src) in cur.iter().enumerate() {
            ctx.fb.push(LpirOp::Store {
                base: addr,
                offset: (j * 4) as u32,
                value: src,
            });
        }
        return Ok(Some(WritableActual {
            addr,
            writeback: Some(WritableWriteback::AccessExpr { pointer: ptr, slot }),
        }));
    }

    if let Some((gv, idx_chain)) =
        crate::lower_struct::peel_struct_access_index_chain_to_global(ctx.func, ptr)
    {
        if ctx.global_map.get(&gv).is_some_and(|g| g.is_uniform) {
            return Err(LowerError::UnsupportedExpression(String::from(
                "cannot write to uniform variable",
            )));
        }
        let root_ty = global_unwrapped_root_naga_ty(ctx, gv);
        if let Some(layout) = crate::naga_util::aggregate_layout(ctx.module, root_ty)? {
            if matches!(&layout.kind, crate::naga_util::AggregateKind::Struct { .. }) {
                let (slot_base, byte_off, dest_ty) =
                    crate::lower_struct::global_struct_path_target_addr(ctx, gv, &idx_chain)?;
                if dest_ty == pointee_ty {
                    let addr = vreg_pointer_plus_u32_byte_offset(ctx, slot_base, byte_off)?;
                    return Ok(Some(WritableActual {
                        addr,
                        writeback: None,
                    }));
                }
            }
        }
    }

    if let Some((root, ops)) =
        crate::lower_array_multidim::peel_array_subscript_chain(ctx.func, ptr)
    {
        use crate::lower_array_multidim::{ArraySubscriptRoot, SubscriptOperand};
        if let ArraySubscriptRoot::Global(gv) = root {
            if ctx.global_map.get(&gv).is_some_and(|g| g.is_uniform) {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "cannot write to uniform variable",
                )));
            }
            let maybe_info =
                ctx.aggregate_info_for_subscript_root(ArraySubscriptRoot::Global(gv))?;
            if let Some(info) = maybe_info {
                if matches!(
                    &info.layout.kind,
                    crate::naga_util::AggregateKind::Array { .. }
                ) && ops.len() == info.dimensions().len()
                    && info.leaf_element_ty() == pointee_ty
                {
                    let elem_addr = if ops.iter().all(|o| matches!(o, SubscriptOperand::Const(_))) {
                        let idxs: alloc::vec::Vec<u32> = ops
                            .iter()
                            .map(|o| match o {
                                SubscriptOperand::Const(c) => *c,
                                SubscriptOperand::Dynamic(_) => 0,
                            })
                            .collect();
                        let flat = crate::lower_array_multidim::flat_index_const_clamped(
                            &info.dimensions(),
                            &idxs,
                        )?;
                        array_element_address(ctx, &info, ElementIndex::Const(flat))?
                    } else {
                        let index_v = crate::lower_array::emit_row_major_flat_from_operands(
                            ctx,
                            info.dimensions(),
                            &ops,
                        )?;
                        array_element_address(ctx, &info, ElementIndex::Dynamic(index_v))?
                    };
                    return Ok(Some(WritableActual {
                        addr: elem_addr,
                        writeback: None,
                    }));
                }
            }
        }
    }

    let ir_tys = naga_type_to_ir_types(ctx.module, &callee_pointee_ty)?;
    let slot = ctx.fb.alloc_slot(ir_tys.len() as u32 * 4);
    let addr = ctx.fb.alloc_vreg(IrType::Pointer);
    ctx.fb.push(LpirOp::SlotAddr { dst: addr, slot });
    let cur = ctx.ensure_expr_vec(actual)?;
    if cur.len() != ir_tys.len() {
        return Err(LowerError::Internal(format!(
            "writable global aggregate fallback temp: expected {} components, got {}",
            ir_tys.len(),
            cur.len()
        )));
    }
    for (j, &src) in cur.iter().enumerate() {
        ctx.fb.push(LpirOp::Store {
            base: addr,
            offset: (j * 4) as u32,
            value: src,
        });
    }
    Ok(Some(WritableActual {
        addr,
        writeback: Some(WritableWriteback::AccessExpr { pointer: ptr, slot }),
    }))
}

fn try_resolve_global_flat_vector_matrix_swizzle_temp(
    ctx: &mut LowerCtx<'_>,
    ptr: Handle<Expression>,
    actual: Handle<Expression>,
    pointee_ty: Handle<naga::Type>,
) -> Result<Option<WritableActual>, LowerError> {
    match &ctx.func.expressions[ptr] {
        Expression::Swizzle { vector, .. } => {
            if !matches!(
                &ctx.func.expressions[*vector],
                Expression::GlobalVariable(_),
            ) {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "inout/out swizzle actual must target a global vector",
                )));
            }
        }
        Expression::AccessIndex { base, .. } => match &ctx.func.expressions[*base] {
            Expression::GlobalVariable(_) => {}
            Expression::AccessIndex { base: inner, .. } => {
                if !matches!(&ctx.func.expressions[*inner], Expression::GlobalVariable(_),) {
                    return Err(LowerError::UnsupportedExpression(String::from(
                        "inout/out matrix cell: expected global matrix variable",
                    )));
                }
            }
            _ => {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "inout/out AccessIndex actual must index a global vector or matrix",
                )));
            }
        },
        Expression::Access { base, .. } => match &ctx.func.expressions[*base] {
            Expression::GlobalVariable(_) => {}
            Expression::Access { base: inner, .. } => {
                if !matches!(&ctx.func.expressions[*inner], Expression::GlobalVariable(_),) {
                    return Err(LowerError::UnsupportedExpression(String::from(
                        "inout/out dynamic matrix cell: expected global matrix variable",
                    )));
                }
            }
            _ => {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "inout/out Access actual must subscript a global vector or matrix",
                )));
            }
        },
        _ => return Ok(None),
    }

    let pointee_inner = &ctx.module.types[pointee_ty].inner;
    let ir_tys = naga_type_to_ir_types(ctx.module, pointee_inner)?;
    let slot = ctx.fb.alloc_slot(ir_tys.len() as u32 * 4);
    let addr = ctx.fb.alloc_vreg(IrType::Pointer);
    ctx.fb.push(LpirOp::SlotAddr { dst: addr, slot });

    let cur = ctx.ensure_expr_vec(actual)?;
    if cur.len() != ir_tys.len() {
        return Err(LowerError::Internal(format!(
            "writable global actual temp: expected {} rvalue components, got {}",
            ir_tys.len(),
            cur.len()
        )));
    }
    for (j, &src) in cur.iter().enumerate() {
        ctx.fb.push(LpirOp::Store {
            base: addr,
            offset: (j * 4) as u32,
            value: src,
        });
    }

    Ok(Some(WritableActual {
        addr,
        writeback: Some(WritableWriteback::AccessExpr { pointer: ptr, slot }),
    }))
}

fn try_resolve_global_access_or_swizzle_temp(
    ctx: &mut LowerCtx<'_>,
    actual: Handle<Expression>,
    pointee_ty: Handle<naga::Type>,
) -> Result<Option<WritableActual>, LowerError> {
    let ptr = unwrap_load_to_pointer(actual, ctx);
    match &ctx.func.expressions[ptr] {
        Expression::Access { .. } | Expression::AccessIndex { .. } | Expression::Swizzle { .. } => {
        }
        _ => return Ok(None),
    }

    reject_uniform_derived_writable_actual(ctx, ptr)?;
    types_compatible_for_inout_actual(ctx, actual, pointee_ty)?;

    if peel_to_local_variable_root(ptr, ctx).is_some() {
        return Ok(None);
    }
    if peel_to_pointer_arg_root(ptr, ctx).is_some() {
        return Ok(None);
    }
    let Some(gv) = peel_to_global_variable_root(ptr, ctx) else {
        return Ok(None);
    };
    if ctx.global_map.get(&gv).is_some_and(|g| g.is_uniform) {
        return Err(LowerError::UnsupportedExpression(String::from(
            "cannot write to uniform variable",
        )));
    }

    let root_ty = global_unwrapped_root_naga_ty(ctx, gv);
    if let Some(layout) = crate::naga_util::aggregate_layout(ctx.module, root_ty)? {
        if matches!(
            &layout.kind,
            crate::naga_util::AggregateKind::Struct { .. }
                | crate::naga_util::AggregateKind::Array { .. }
        ) {
            return try_resolve_global_aggregate_access(ctx, ptr, actual, pointee_ty);
        }
    }

    try_resolve_global_flat_vector_matrix_swizzle_temp(ctx, ptr, actual, pointee_ty)
}

fn vreg_pointer_plus_u32_byte_offset(
    ctx: &mut LowerCtx<'_>,
    base_addr: VReg,
    off: u32,
) -> Result<VReg, LowerError> {
    if off == 0 {
        return Ok(base_addr);
    }
    let off_v = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::IconstI32 {
        dst: off_v,
        value: i32::try_from(off).map_err(|_| {
            LowerError::Internal(String::from(
                "writable aggregate: byte offset overflows i32",
            ))
        })?,
    });
    let out = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Iadd {
        dst: out,
        lhs: base_addr,
        rhs: off_v,
    });
    Ok(out)
}

/// Writable post-call stores (same peel order as [`crate::lower_stmt`]).
fn store_vregs_into_writable_access_writeback(
    ctx: &mut LowerCtx<'_>,
    ptr: Handle<Expression>,
    pointee_inner: &TypeInner,
    srcs: &[VReg],
) -> Result<(), LowerError> {
    if let Some(chain) = crate::lower_struct::peel_arrayofstruct_chain(ctx, ptr) {
        return crate::lower_struct::store_array_struct_element_vregs(ctx, &chain, srcs);
    }
    if let Some((lv, chain)) =
        crate::lower_struct::peel_struct_access_index_chain_to_local(ctx.func, ptr)
    {
        if let Some(info) = ctx.aggregate_map.get(&lv).cloned() {
            if matches!(
                &info.layout.kind,
                crate::naga_util::AggregateKind::Struct { .. }
            ) {
                return crate::lower_struct::store_struct_path_into_local_vregs(
                    ctx, &info, &chain, srcs,
                );
            }
        }
    }
    if let Some((arg_i, chain)) =
        crate::lower_struct::peel_struct_access_index_chain_to_param(ctx.func, ptr)
    {
        if let Some(info) =
            ctx.aggregate_info_for_subscript_root(ArraySubscriptRoot::Param(arg_i))?
        {
            if matches!(
                &info.layout.kind,
                crate::naga_util::AggregateKind::Struct { .. }
            ) {
                return crate::lower_struct::store_struct_path_into_local_vregs(
                    ctx, &info, &chain, srcs,
                );
            }
        }
    }
    if let Some((gv, chain)) =
        crate::lower_struct::peel_struct_access_index_chain_to_global(ctx.func, ptr)
    {
        if ctx
            .global_map
            .get(&gv)
            .map(|m| !m.is_uniform)
            .unwrap_or(false)
        {
            return crate::lower_struct::store_struct_path_into_global_vregs(ctx, gv, &chain, srcs);
        }
        return Err(LowerError::UnsupportedExpression(String::from(
            "cannot write to uniform variable",
        )));
    }
    if let Some((lv, idxs)) = crate::lower_array_multidim::peel_access_index_chain(ctx.func, ptr) {
        if let Some(info) = ctx.aggregate_map.get(&lv).cloned() {
            if matches!(
                &info.layout.kind,
                crate::naga_util::AggregateKind::Array { .. }
            ) && idxs.len() == info.dimensions().len()
            {
                let flat = crate::lower_array_multidim::flat_index_const_clamped(
                    &info.dimensions(),
                    &idxs,
                )?;
                return crate::lower_array::store_array_element_const_vregs(ctx, &info, flat, srcs);
            }
        }
    }
    if let Some((root, ops)) =
        crate::lower_array_multidim::peel_array_subscript_chain(ctx.func, ptr)
    {
        use crate::lower_array_multidim::{ArraySubscriptRoot, SubscriptOperand};
        let maybe_info = match root {
            ArraySubscriptRoot::Local(lv) => ctx.aggregate_map.get(&lv).cloned(),
            ArraySubscriptRoot::Param(arg_i) => {
                ctx.aggregate_info_for_subscript_root(ArraySubscriptRoot::Param(arg_i))?
            }
            ArraySubscriptRoot::Global(gv) => {
                if ctx.global_map.get(&gv).is_some_and(|g| g.is_uniform) {
                    return Err(LowerError::UnsupportedExpression(String::from(
                        "cannot write to uniform variable",
                    )));
                }
                ctx.aggregate_info_for_subscript_root(ArraySubscriptRoot::Global(gv))?
            }
            _ => None,
        };
        if let Some(info) = maybe_info {
            if matches!(
                &info.layout.kind,
                crate::naga_util::AggregateKind::Array { .. }
            ) && ops.len() == info.dimensions().len()
            {
                if ops.iter().all(|o| matches!(o, SubscriptOperand::Const(_))) {
                    let idxs: alloc::vec::Vec<u32> = ops
                        .iter()
                        .map(|o| match o {
                            SubscriptOperand::Const(c) => *c,
                            SubscriptOperand::Dynamic(_) => 0,
                        })
                        .collect();
                    let flat = crate::lower_array_multidim::flat_index_const_clamped(
                        &info.dimensions(),
                        &idxs,
                    )?;
                    return crate::lower_array::store_array_element_const_vregs(
                        ctx, &info, flat, srcs,
                    );
                }
                let index_v = crate::lower_array::emit_row_major_flat_from_operands(
                    ctx,
                    info.dimensions(),
                    &ops,
                )?;
                return crate::lower_array::store_array_element_dynamic_vregs(
                    ctx, &info, index_v, srcs,
                );
            }
        }
    }
    crate::lower_access::store_vregs_through_local_access_leaf(ctx, ptr, pointee_inner, srcs)
        .or_else(|_| {
            crate::lower_access::store_vregs_through_pointer_arg_access_leaf(
                ctx,
                ptr,
                pointee_inner,
                srcs,
            )
        })
        .or_else(|_| {
            crate::lower_access::store_vregs_through_global_access_leaf(
                ctx,
                ptr,
                pointee_inner,
                srcs,
            )
        })
}

fn try_resolve_local_aggregate_access(
    ctx: &mut LowerCtx<'_>,
    ptr: Handle<Expression>,
    actual: Handle<Expression>,
    pointee_ty: Handle<naga::Type>,
) -> Result<Option<WritableActual>, LowerError> {
    let callee_pointee_ty = ctx.module.types[pointee_ty].inner.clone();
    if !pointee_is_aggregate(&callee_pointee_ty) {
        let ir_tys = naga_type_to_ir_types(ctx.module, &callee_pointee_ty)?;
        let slot = ctx.fb.alloc_slot(ir_tys.len() as u32 * 4);
        let addr = ctx.fb.alloc_vreg(IrType::Pointer);
        ctx.fb.push(LpirOp::SlotAddr { dst: addr, slot });
        let cur = ctx.ensure_expr_vec(actual)?;
        if cur.len() != ir_tys.len() {
            return Err(LowerError::Internal(format!(
                "writable aggregate leaf temp: expected {} rvalue components, got {}",
                ir_tys.len(),
                cur.len()
            )));
        }
        for (j, &src) in cur.iter().enumerate() {
            ctx.fb.push(LpirOp::Store {
                base: addr,
                offset: (j * 4) as u32,
                value: src,
            });
        }
        return Ok(Some(WritableActual {
            addr,
            writeback: Some(WritableWriteback::AccessExpr { pointer: ptr, slot }),
        }));
    }

    if let Some(chain) = crate::lower_struct::peel_arrayofstruct_chain(ctx, ptr) {
        let (tip_addr, tip_ty) = crate::lower_struct::array_of_struct_tip_addr_ty(ctx, &chain)?;
        if tip_ty == pointee_ty {
            return Ok(Some(WritableActual {
                addr: tip_addr,
                writeback: None,
            }));
        }
    }

    if let Some((lv, idx_chain)) =
        crate::lower_struct::peel_struct_access_index_chain_to_local(ctx.func, ptr)
    {
        if let Some(info) = ctx.aggregate_map.get(&lv).cloned() {
            if matches!(
                &info.layout.kind,
                crate::naga_util::AggregateKind::Struct { .. }
            ) {
                let (slot_base, byte_off, dest_ty) =
                    crate::lower_struct::local_struct_path_target_addr(ctx, &info, &idx_chain)?;
                if dest_ty == pointee_ty {
                    let addr = vreg_pointer_plus_u32_byte_offset(ctx, slot_base, byte_off)?;
                    return Ok(Some(WritableActual {
                        addr,
                        writeback: None,
                    }));
                }
            }
        }
    }

    if let Some((root, ops)) =
        crate::lower_array_multidim::peel_array_subscript_chain(ctx.func, ptr)
    {
        use crate::lower_array_multidim::ArraySubscriptRoot;
        use crate::lower_array_multidim::SubscriptOperand;
        if let ArraySubscriptRoot::Local(lv) = root
            && let Some(info) = ctx.aggregate_map.get(&lv).cloned()
            && matches!(
                &info.layout.kind,
                crate::naga_util::AggregateKind::Array { .. }
            )
            && ops.len() == info.dimensions().len()
            && info.leaf_element_ty() == pointee_ty
        {
            let elem_addr = if ops.iter().all(|o| matches!(o, SubscriptOperand::Const(_))) {
                let idxs: alloc::vec::Vec<u32> = ops
                    .iter()
                    .map(|o| match o {
                        SubscriptOperand::Const(c) => *c,
                        SubscriptOperand::Dynamic(_) => 0,
                    })
                    .collect();
                let flat = crate::lower_array_multidim::flat_index_const_clamped(
                    &info.dimensions(),
                    &idxs,
                )?;
                array_element_address(ctx, &info, ElementIndex::Const(flat))?
            } else {
                let index_v = crate::lower_array::emit_row_major_flat_from_operands(
                    ctx,
                    info.dimensions(),
                    &ops,
                )?;
                array_element_address(ctx, &info, ElementIndex::Dynamic(index_v))?
            };
            return Ok(Some(WritableActual {
                addr: elem_addr,
                writeback: None,
            }));
        }
    }

    let ir_tys = naga_type_to_ir_types(ctx.module, &callee_pointee_ty)?;
    let slot = ctx.fb.alloc_slot(ir_tys.len() as u32 * 4);
    let addr = ctx.fb.alloc_vreg(IrType::Pointer);
    ctx.fb.push(LpirOp::SlotAddr { dst: addr, slot });
    let cur = ctx.ensure_expr_vec(actual)?;
    if cur.len() != ir_tys.len() {
        return Err(LowerError::Internal(format!(
            "writable aggregate fallback temp: expected {} components, got {}",
            ir_tys.len(),
            cur.len()
        )));
    }
    for (j, &src) in cur.iter().enumerate() {
        ctx.fb.push(LpirOp::Store {
            base: addr,
            offset: (j * 4) as u32,
            value: src,
        });
    }
    Ok(Some(WritableActual {
        addr,
        writeback: Some(WritableWriteback::AccessExpr { pointer: ptr, slot }),
    }))
}

fn try_resolve_pointer_arg_aggregate_access(
    ctx: &mut LowerCtx<'_>,
    ptr: Handle<Expression>,
    actual: Handle<Expression>,
    pointee_ty: Handle<naga::Type>,
) -> Result<Option<WritableActual>, LowerError> {
    let callee_pointee_ty = ctx.module.types[pointee_ty].inner.clone();
    if !pointee_is_aggregate(&callee_pointee_ty) {
        let ir_tys = naga_type_to_ir_types(ctx.module, &callee_pointee_ty)?;
        let slot = ctx.fb.alloc_slot(ir_tys.len() as u32 * 4);
        let addr = ctx.fb.alloc_vreg(IrType::Pointer);
        ctx.fb.push(LpirOp::SlotAddr { dst: addr, slot });
        let cur = ctx.ensure_expr_vec(actual)?;
        if cur.len() != ir_tys.len() {
            return Err(LowerError::Internal(format!(
                "writable aggregate leaf temp: expected {} rvalue components, got {}",
                ir_tys.len(),
                cur.len()
            )));
        }
        for (j, &src) in cur.iter().enumerate() {
            ctx.fb.push(LpirOp::Store {
                base: addr,
                offset: (j * 4) as u32,
                value: src,
            });
        }
        return Ok(Some(WritableActual {
            addr,
            writeback: Some(WritableWriteback::AccessExpr { pointer: ptr, slot }),
        }));
    }

    if let Some(chain) = crate::lower_struct::peel_arrayofstruct_chain(ctx, ptr) {
        let (tip_addr, tip_ty) = crate::lower_struct::array_of_struct_tip_addr_ty(ctx, &chain)?;
        if tip_ty == pointee_ty {
            return Ok(Some(WritableActual {
                addr: tip_addr,
                writeback: None,
            }));
        }
    }

    if let Some((arg_i, idx_chain)) =
        crate::lower_struct::peel_struct_access_index_chain_to_param(ctx.func, ptr)
    {
        if let Some(info) =
            ctx.aggregate_info_for_subscript_root(ArraySubscriptRoot::Param(arg_i))?
        {
            if matches!(
                &info.layout.kind,
                crate::naga_util::AggregateKind::Struct { .. }
            ) {
                let (slot_base, byte_off, dest_ty) =
                    crate::lower_struct::local_struct_path_target_addr(ctx, &info, &idx_chain)?;
                if dest_ty == pointee_ty {
                    let addr = vreg_pointer_plus_u32_byte_offset(ctx, slot_base, byte_off)?;
                    return Ok(Some(WritableActual {
                        addr,
                        writeback: None,
                    }));
                }
            }
        }
    }

    if let Some((root, ops)) =
        crate::lower_array_multidim::peel_array_subscript_chain(ctx.func, ptr)
    {
        use crate::lower_array_multidim::ArraySubscriptRoot;
        use crate::lower_array_multidim::SubscriptOperand;
        if let ArraySubscriptRoot::Param(arg_i) = root
            && let Some(info) =
                ctx.aggregate_info_for_subscript_root(ArraySubscriptRoot::Param(arg_i))?
            && matches!(
                &info.layout.kind,
                crate::naga_util::AggregateKind::Array { .. }
            )
            && ops.len() == info.dimensions().len()
            && info.leaf_element_ty() == pointee_ty
        {
            let elem_addr = if ops.iter().all(|o| matches!(o, SubscriptOperand::Const(_))) {
                let idxs: alloc::vec::Vec<u32> = ops
                    .iter()
                    .map(|o| match o {
                        SubscriptOperand::Const(c) => *c,
                        SubscriptOperand::Dynamic(_) => 0,
                    })
                    .collect();
                let flat = crate::lower_array_multidim::flat_index_const_clamped(
                    &info.dimensions(),
                    &idxs,
                )?;
                array_element_address(ctx, &info, ElementIndex::Const(flat))?
            } else {
                let index_v = crate::lower_array::emit_row_major_flat_from_operands(
                    ctx,
                    info.dimensions(),
                    &ops,
                )?;
                array_element_address(ctx, &info, ElementIndex::Dynamic(index_v))?
            };
            return Ok(Some(WritableActual {
                addr: elem_addr,
                writeback: None,
            }));
        }
    }

    let ir_tys = naga_type_to_ir_types(ctx.module, &callee_pointee_ty)?;
    let slot = ctx.fb.alloc_slot(ir_tys.len() as u32 * 4);
    let addr = ctx.fb.alloc_vreg(IrType::Pointer);
    ctx.fb.push(LpirOp::SlotAddr { dst: addr, slot });
    let cur = ctx.ensure_expr_vec(actual)?;
    if cur.len() != ir_tys.len() {
        return Err(LowerError::Internal(format!(
            "writable aggregate fallback temp: expected {} components, got {}",
            ir_tys.len(),
            cur.len()
        )));
    }
    for (j, &src) in cur.iter().enumerate() {
        ctx.fb.push(LpirOp::Store {
            base: addr,
            offset: (j * 4) as u32,
            value: src,
        });
    }
    Ok(Some(WritableActual {
        addr,
        writeback: Some(WritableWriteback::AccessExpr { pointer: ptr, slot }),
    }))
}

fn try_resolve_pointer_arg_flat_vector_matrix_swizzle_temp(
    ctx: &mut LowerCtx<'_>,
    ptr: Handle<Expression>,
    actual: Handle<Expression>,
    pointee_ty: Handle<naga::Type>,
) -> Result<Option<WritableActual>, LowerError> {
    let root_arg = match &ctx.func.expressions[ptr] {
        Expression::Swizzle { vector, .. } => match &ctx.func.expressions[*vector] {
            Expression::FunctionArgument(arg_i) if ctx.pointer_args.contains_key(arg_i) => *arg_i,
            _ => {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "inout/out swizzle actual must target a vector (local or pointer formal)",
                )));
            }
        },
        Expression::AccessIndex { base, .. } => match &ctx.func.expressions[*base] {
            Expression::FunctionArgument(arg_i) if ctx.pointer_args.contains_key(arg_i) => *arg_i,
            Expression::AccessIndex { base: inner, .. } => {
                let Expression::FunctionArgument(arg_i) = &ctx.func.expressions[*inner] else {
                    return Err(LowerError::UnsupportedExpression(String::from(
                        "inout/out matrix cell: expected matrix variable (local or pointer formal)",
                    )));
                };
                if !ctx.pointer_args.contains_key(arg_i) {
                    return Err(LowerError::UnsupportedExpression(String::from(
                        "inout/out matrix cell: expected pointer formal matrix",
                    )));
                }
                *arg_i
            }
            _ => {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "inout/out AccessIndex actual must index a vector or matrix (local or pointer formal)",
                )));
            }
        },
        Expression::Access { base, .. } => match &ctx.func.expressions[*base] {
            Expression::FunctionArgument(arg_i) if ctx.pointer_args.contains_key(arg_i) => *arg_i,
            Expression::Access { base: inner, .. } => {
                let Expression::FunctionArgument(arg_i) = &ctx.func.expressions[*inner] else {
                    return Err(LowerError::UnsupportedExpression(String::from(
                        "inout/out dynamic matrix cell: expected matrix variable (local or pointer formal)",
                    )));
                };
                if !ctx.pointer_args.contains_key(arg_i) {
                    return Err(LowerError::UnsupportedExpression(String::from(
                        "inout/out dynamic matrix cell: expected pointer formal matrix",
                    )));
                }
                *arg_i
            }
            _ => {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "inout/out Access actual must subscript a vector or matrix (local or pointer formal)",
                )));
            }
        },
        _ => return Ok(None),
    };

    if let Some(info) =
        ctx.aggregate_info_for_subscript_root(ArraySubscriptRoot::Param(root_arg))?
    {
        if matches!(
            &info.layout.kind,
            crate::naga_util::AggregateKind::Struct { .. }
                | crate::naga_util::AggregateKind::Array { .. }
        ) {
            return Ok(None);
        }
    }

    let pointee_inner = &ctx.module.types[pointee_ty].inner;
    let ir_tys = naga_type_to_ir_types(ctx.module, pointee_inner)?;
    let slot = ctx.fb.alloc_slot(ir_tys.len() as u32 * 4);
    let addr = ctx.fb.alloc_vreg(IrType::Pointer);
    ctx.fb.push(LpirOp::SlotAddr { dst: addr, slot });

    let cur = ctx.ensure_expr_vec(actual)?;
    if cur.len() != ir_tys.len() {
        return Err(LowerError::Internal(format!(
            "writable actual temp: expected {} rvalue components, got {}",
            ir_tys.len(),
            cur.len()
        )));
    }
    for (j, &src) in cur.iter().enumerate() {
        ctx.fb.push(LpirOp::Store {
            base: addr,
            offset: (j * 4) as u32,
            value: src,
        });
    }

    Ok(Some(WritableActual {
        addr,
        writeback: Some(WritableWriteback::AccessExpr { pointer: ptr, slot }),
    }))
}

fn try_resolve_pointer_arg_access_or_swizzle_temp(
    ctx: &mut LowerCtx<'_>,
    actual: Handle<Expression>,
    pointee_ty: Handle<naga::Type>,
) -> Result<Option<WritableActual>, LowerError> {
    let ptr = unwrap_load_to_pointer(actual, ctx);
    match &ctx.func.expressions[ptr] {
        Expression::Access { .. } | Expression::AccessIndex { .. } | Expression::Swizzle { .. } => {
        }
        _ => return Ok(None),
    }

    types_compatible_for_inout_actual(ctx, actual, pointee_ty)?;

    let Some(arg_i) = peel_to_pointer_arg_root(ptr, ctx) else {
        return Ok(None);
    };

    if let Some(info) = ctx.aggregate_info_for_subscript_root(ArraySubscriptRoot::Param(arg_i))? {
        if matches!(
            &info.layout.kind,
            crate::naga_util::AggregateKind::Struct { .. }
                | crate::naga_util::AggregateKind::Array { .. }
        ) {
            return try_resolve_pointer_arg_aggregate_access(ctx, ptr, actual, pointee_ty);
        }
    }

    try_resolve_pointer_arg_flat_vector_matrix_swizzle_temp(ctx, ptr, actual, pointee_ty)
}

fn try_resolve_local_flat_vector_matrix_swizzle_temp(
    ctx: &mut LowerCtx<'_>,
    ptr: Handle<Expression>,
    actual: Handle<Expression>,
    pointee_ty: Handle<naga::Type>,
) -> Result<Option<WritableActual>, LowerError> {
    let root_lv = match &ctx.func.expressions[ptr] {
        Expression::Swizzle { vector, .. } => match &ctx.func.expressions[*vector] {
            Expression::LocalVariable(lv) => *lv,
            _ => {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "inout/out swizzle actual must target a local vector",
                )));
            }
        },
        Expression::AccessIndex { base, .. } => match &ctx.func.expressions[*base] {
            Expression::LocalVariable(lv) => *lv,
            Expression::AccessIndex { base: inner, .. } => {
                let Expression::LocalVariable(lv) = &ctx.func.expressions[*inner] else {
                    return Err(LowerError::UnsupportedExpression(String::from(
                        "inout/out matrix cell: expected local matrix variable",
                    )));
                };
                *lv
            }
            _ => {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "inout/out AccessIndex actual must index a local vector or matrix",
                )));
            }
        },
        Expression::Access { base, .. } => match &ctx.func.expressions[*base] {
            Expression::LocalVariable(lv) => *lv,
            Expression::Access { base: inner, .. } => {
                let Expression::LocalVariable(lv) = &ctx.func.expressions[*inner] else {
                    return Err(LowerError::UnsupportedExpression(String::from(
                        "inout/out dynamic matrix cell: expected local matrix variable",
                    )));
                };
                *lv
            }
            _ => {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "inout/out Access actual must subscript a local vector or matrix",
                )));
            }
        },
        _ => return Ok(None),
    };

    if ctx.aggregate_map.contains_key(&root_lv) {
        return Ok(None);
    }

    let pointee_inner = &ctx.module.types[pointee_ty].inner;
    let ir_tys = naga_type_to_ir_types(ctx.module, pointee_inner)?;
    let slot = ctx.fb.alloc_slot(ir_tys.len() as u32 * 4);
    let addr = ctx.fb.alloc_vreg(IrType::Pointer);
    ctx.fb.push(LpirOp::SlotAddr { dst: addr, slot });

    let cur = ctx.ensure_expr_vec(actual)?;
    if cur.len() != ir_tys.len() {
        return Err(LowerError::Internal(format!(
            "writable actual temp: expected {} rvalue components, got {}",
            ir_tys.len(),
            cur.len()
        )));
    }
    for (j, &src) in cur.iter().enumerate() {
        ctx.fb.push(LpirOp::Store {
            base: addr,
            offset: (j * 4) as u32,
            value: src,
        });
    }

    Ok(Some(WritableActual {
        addr,
        writeback: Some(WritableWriteback::AccessExpr { pointer: ptr, slot }),
    }))
}

fn try_resolve_local_access_or_swizzle_temp(
    ctx: &mut LowerCtx<'_>,
    actual: Handle<Expression>,
    pointee_ty: Handle<naga::Type>,
) -> Result<Option<WritableActual>, LowerError> {
    let ptr = unwrap_load_to_pointer(actual, ctx);
    match &ctx.func.expressions[ptr] {
        Expression::Access { .. } | Expression::AccessIndex { .. } | Expression::Swizzle { .. } => {
        }
        _ => return Ok(None),
    }

    reject_uniform_derived_writable_actual(ctx, ptr)?;
    types_compatible_for_inout_actual(ctx, actual, pointee_ty)?;

    if peel_to_local_variable_root(ptr, ctx).is_none()
        && peel_to_pointer_arg_root(ptr, ctx).is_some()
    {
        return Ok(None);
    }

    if let Some(root_lv) = peel_to_local_variable_root(ptr, ctx) {
        if ctx.aggregate_map.contains_key(&root_lv) {
            return try_resolve_local_aggregate_access(ctx, ptr, actual, pointee_ty);
        }
    }

    try_resolve_local_flat_vector_matrix_swizzle_temp(ctx, ptr, actual, pointee_ty)
}

pub(crate) fn resolve_writable_actual(
    ctx: &mut LowerCtx<'_>,
    actual: Handle<Expression>,
    pointee_ty: Handle<naga::Type>,
) -> Result<WritableActual, LowerError> {
    if let Some(wa) = try_resolve_local_access_or_swizzle_temp(ctx, actual, pointee_ty)? {
        return Ok(wa);
    }
    if let Some(wa) = try_resolve_pointer_arg_access_or_swizzle_temp(ctx, actual, pointee_ty)? {
        return Ok(wa);
    }
    if let Some(wa) = try_resolve_global_access_or_swizzle_temp(ctx, actual, pointee_ty)? {
        return Ok(wa);
    }

    let ptr = unwrap_load_to_pointer(actual, ctx);
    reject_uniform_derived_writable_actual(ctx, ptr)?;

    match &ctx.func.expressions[ptr] {
        Expression::LocalVariable(lv) => {
            let lv = *lv;

            if let Some(info) = ctx.aggregate_map.get(&lv).cloned() {
                let addr = aggregate_storage_base_vreg(ctx, &info.slot)?;
                return Ok(WritableActual {
                    addr,
                    writeback: None,
                });
            }

            let base_inner = &ctx.module.types[pointee_ty].inner;
            let ir_tys = naga_type_to_ir_types(ctx.module, base_inner)?;
            let slot = ctx.fb.alloc_slot(ir_tys.len() as u32 * 4);
            let addr = ctx.fb.alloc_vreg(IrType::Pointer);
            ctx.fb.push(LpirOp::SlotAddr { dst: addr, slot });
            let local_vregs = ctx.resolve_local(lv)?;
            for (j, &src) in local_vregs.iter().enumerate() {
                ctx.fb.push(LpirOp::Store {
                    base: addr,
                    offset: (j * 4) as u32,
                    value: src,
                });
            }
            Ok(WritableActual {
                addr,
                writeback: Some(WritableWriteback::LocalFlat { local: lv, slot }),
            })
        }
        Expression::GlobalVariable(gv) => {
            if ctx.global_map.get(gv).is_some_and(|g| g.is_uniform) {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "cannot write to uniform variable",
                )));
            }
            let gv = *gv;
            let ty_h = global_unwrapped_root_naga_ty(ctx, gv);
            if let Some(layout) = crate::naga_util::aggregate_layout(ctx.module, ty_h)? {
                if matches!(
                    &layout.kind,
                    crate::naga_util::AggregateKind::Struct { .. }
                        | crate::naga_util::AggregateKind::Array { .. }
                ) {
                    let addr = aggregate_storage_base_vreg(ctx, &AggregateSlot::Global(gv))?;
                    return Ok(WritableActual {
                        addr,
                        writeback: None,
                    });
                }
            }

            let base_inner = &ctx.module.types[pointee_ty].inner;
            let ir_tys = naga_type_to_ir_types(ctx.module, base_inner)?;
            let slot = ctx.fb.alloc_slot(ir_tys.len() as u32 * 4);
            let addr = ctx.fb.alloc_vreg(IrType::Pointer);
            ctx.fb.push(LpirOp::SlotAddr { dst: addr, slot });
            let gv_vregs = ctx.ensure_expr_vec(actual)?;
            if gv_vregs.len() != ir_tys.len() {
                return Err(LowerError::Internal(format!(
                    "writable global bare: expected {} rvalue components, got {}",
                    ir_tys.len(),
                    gv_vregs.len()
                )));
            }
            for (j, &src) in gv_vregs.iter().enumerate() {
                ctx.fb.push(LpirOp::Store {
                    base: addr,
                    offset: (j * 4) as u32,
                    value: src,
                });
            }
            Ok(WritableActual {
                addr,
                writeback: Some(WritableWriteback::GlobalFlat { gv, slot }),
            })
        }
        _ => Err(LowerError::UnsupportedExpression(String::from(
            "inout/out call argument must be a local variable",
        ))),
    }
}

pub(crate) fn apply_writable_writeback(
    ctx: &mut LowerCtx<'_>,
    writeback: WritableWriteback,
) -> Result<(), LowerError> {
    match writeback {
        WritableWriteback::LocalFlat { local, slot } => {
            let local_vregs = ctx.resolve_local(local)?;
            let addr = ctx.fb.alloc_vreg(IrType::Pointer);
            ctx.fb.push(LpirOp::SlotAddr { dst: addr, slot });
            for (j, dst_v) in local_vregs.iter().enumerate() {
                ctx.fb.push(LpirOp::Load {
                    dst: *dst_v,
                    base: addr,
                    offset: (j * 4) as u32,
                });
            }
            Ok(())
        }
        WritableWriteback::GlobalFlat { gv, slot } => {
            let info = ctx.global_map.get(&gv).cloned().ok_or_else(|| {
                LowerError::Internal(format!("GlobalFlat writeback: {gv:?} not in global_map"))
            })?;
            let ty_h = global_unwrapped_root_naga_ty(ctx, gv);
            let root_inner = &ctx.module.types[ty_h].inner;
            let ir_tys = naga_type_to_ir_types(ctx.module, root_inner)?;
            let addr = ctx.fb.alloc_vreg(IrType::Pointer);
            ctx.fb.push(LpirOp::SlotAddr { dst: addr, slot });
            for (j, ty) in ir_tys.iter().enumerate() {
                let dst = ctx.fb.alloc_vreg(*ty);
                ctx.fb.push(LpirOp::Load {
                    dst,
                    base: addr,
                    offset: (j * 4) as u32,
                });
                ctx.fb.push(LpirOp::Store {
                    base: VMCTX_VREG,
                    offset: info.byte_offset + (j as u32 * 4),
                    value: dst,
                });
            }
            Ok(())
        }
        WritableWriteback::AccessExpr { pointer, slot } => {
            let inner = value_type_for_writable_leaf(crate::naga_util::expr_type_inner(
                ctx.module, ctx.func, pointer,
            )?);
            let ir_tys = naga_type_to_ir_types(ctx.module, &inner)?;
            let addr = ctx.fb.alloc_vreg(IrType::Pointer);
            ctx.fb.push(LpirOp::SlotAddr { dst: addr, slot });
            let mut srcs = alloc::vec::Vec::<VReg>::new();
            for (j, ty) in ir_tys.iter().enumerate() {
                let dst = ctx.fb.alloc_vreg(*ty);
                ctx.fb.push(LpirOp::Load {
                    dst,
                    base: addr,
                    offset: (j * 4) as u32,
                });
                srcs.push(dst);
            }
            store_vregs_into_writable_access_writeback(ctx, pointer, &inner, &srcs)
        }
    }
}
