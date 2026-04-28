//! Naga [`naga::Expression`] → LPIR ops with vector and matrix scalarization.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use lpir::{IrType, LpirOp, VMCTX_VREG, VReg};
use lps_shared::{LayoutRules, LpsType, array_stride, type_alignment, type_size};
use naga::{
    ArraySize, BinaryOperator, Expression, GlobalVariable, Handle, Literal, RelationalFunction,
    ScalarKind, TypeInner,
};

use crate::lower_binary::lower_binary_vec;
use crate::lower_cast::{lower_as_scalar, lower_as_vec, root_scalar_kind};
use crate::lower_ctx::{
    LowerCtx, UniformVmctxDeferred, VRegVec, naga_scalar_to_ir_type, vector_size_usize,
};
use crate::lower_error::LowerError;
use crate::lower_math;
use crate::lower_unary::lower_unary_vec;
use crate::naga_util::{expr_scalar_kind, expr_type_inner, type_handle_scalar_kind};

pub(crate) fn lower_expr_vec(
    ctx: &mut LowerCtx<'_>,
    expr: Handle<Expression>,
) -> Result<VRegVec, LowerError> {
    let i = expr.index();
    if let Some(vs) = ctx.expr_cache.get(i).and_then(|c| c.as_ref()) {
        return Ok(vs.clone());
    }
    let vs = lower_expr_vec_uncached(ctx, expr)?;
    if let Some(slot) = ctx.expr_cache.get_mut(i) {
        *slot = Some(vs.clone());
    }
    Ok(vs)
}

#[allow(dead_code, reason = "scalar convenience over lower_expr_vec")]
pub(crate) fn lower_expr(
    ctx: &mut LowerCtx<'_>,
    expr: Handle<Expression>,
) -> Result<VReg, LowerError> {
    let vs = lower_expr_vec(ctx, expr)?;
    if vs.len() != 1 {
        return Err(LowerError::Internal(format!(
            "expected scalar expression, got {} components",
            vs.len()
        )));
    }
    Ok(vs[0])
}

fn lower_expr_vec_uncached(
    ctx: &mut LowerCtx<'_>,
    expr: Handle<Expression>,
) -> Result<VRegVec, LowerError> {
    match &ctx.func.expressions[expr] {
        Expression::FunctionArgument(i) => ctx.arg_vregs_for(*i),
        Expression::Load { pointer } => match &ctx.func.expressions[*pointer] {
            Expression::LocalVariable(lv) => {
                if ctx.aggregate_map.contains_key(lv) {
                    return Err(LowerError::UnsupportedExpression(String::from(
                        "Load of whole aggregate (array/struct) local is not supported",
                    )));
                }
                // Snapshot into fresh VRegs so the loaded value does not alias the local's
                // mutable slots (needed for postfix ++/-- and any use-after-store of the same
                // Load expression handle).
                let srcs = ctx.resolve_local(*lv)?;
                let lv_ty = &ctx.module.types[ctx.func.local_variables[*lv].ty].inner;
                let ir_tys = crate::lower_ctx::naga_type_to_ir_types(ctx.module, lv_ty)?;
                if srcs.len() != ir_tys.len() {
                    return Err(LowerError::Internal(format!(
                        "Load local: {} vregs vs {} IR types for {:?}",
                        srcs.len(),
                        ir_tys.len(),
                        lv
                    )));
                }
                let mut snapped = VRegVec::new();
                for (&src, ty) in srcs.iter().zip(ir_tys.iter()) {
                    let dst = ctx.fb.alloc_vreg(*ty);
                    ctx.fb.push(LpirOp::Copy { dst, src });
                    snapped.push(dst);
                }
                Ok(snapped)
            }
            // Subscripted locals (`m[i][j]`) are pointers in Naga; value lives in vregs.
            Expression::AccessIndex { .. } | Expression::Access { .. } => {
                let vs = lower_expr_vec(ctx, *pointer)?;
                snapshot_load_result_vregs(ctx, expr, vs)
            }
            Expression::FunctionArgument(i) => {
                let idx = *i;
                let Some(&base_ty_h) = ctx.pointer_args.get(&idx) else {
                    return Err(LowerError::UnsupportedExpression(String::from(
                        "Load from non-pointer",
                    )));
                };
                let base_inner = &ctx.module.types[base_ty_h].inner;
                if matches!(base_inner, TypeInner::Struct { .. }) {
                    return crate::lower_struct::load_struct_value_vregs_from_base(
                        ctx,
                        ctx.arg_vregs_for(idx)?[0],
                        0,
                        base_ty_h,
                    );
                }
                let ir_tys = crate::lower_ctx::naga_type_to_ir_types(ctx.module, base_inner)?;
                let addr = ctx.arg_vregs_for(idx)?[0];
                let mut vregs = VRegVec::new();
                for (j, ty) in ir_tys.iter().enumerate() {
                    let dst = ctx.fb.alloc_vreg(*ty);
                    ctx.fb.push(LpirOp::Load {
                        dst,
                        base: addr,
                        offset: (j * 4) as u32,
                    });
                    vregs.push(dst);
                }
                Ok(vregs)
            }
            Expression::GlobalVariable(gv_handle) => {
                // Load from a global variable (uniform or private global).
                let (byte_offset, ty) = {
                    let Some(info) = ctx.global_map.get(gv_handle) else {
                        return Err(LowerError::Internal(format!(
                            "GlobalVariable {gv_handle:?} not found in global_map"
                        )));
                    };
                    (info.byte_offset, info.ty.clone())
                };
                load_lps_value_from_vmctx(ctx, byte_offset, &ty)
            }
            _ => Err(LowerError::UnsupportedExpression(String::from(
                "Load from non-local pointer",
            ))),
        },
        Expression::CallResult(_) => {
            if let Some(info) = ctx.call_result_aggregates.get(&expr).cloned() {
                let addr = crate::lower_array::aggregate_storage_base_vreg(ctx, &info.slot)?;
                return Ok(smallvec::smallvec![addr]);
            }
            let i = expr.index();
            ctx.expr_cache
                .get(i)
                .and_then(|c| c.as_ref())
                .cloned()
                .ok_or_else(|| {
                    LowerError::Internal(String::from(
                        "CallResult used before matching Call statement",
                    ))
                })
        }
        Expression::Compose { ty, .. }
            if matches!(&ctx.module.types[*ty].inner, TypeInner::Struct { .. }) =>
        {
            Err(LowerError::UnsupportedExpression(String::from(
                "struct Compose without destination slot — phase 05 routes call args",
            )))
        }
        Expression::Compose { components, .. } => {
            let mut result = VRegVec::new();
            for &comp in components {
                let vs = lower_expr_vec(ctx, comp)?;
                result.extend_from_slice(&vs);
            }
            Ok(result)
        }
        Expression::Splat { size, value } => {
            let vs = lower_expr_vec(ctx, *value)?;
            if vs.len() != 1 {
                return Err(LowerError::Internal(String::from("Splat of non-scalar")));
            }
            let scalar = vs[0];
            let n = vector_size_usize(*size);
            Ok(SmallVecRepeat::repeat(scalar, n))
        }
        Expression::Swizzle {
            size,
            vector,
            pattern,
        } => {
            let base = lower_expr_vec(ctx, *vector)?;
            let n = vector_size_usize(*size);
            let mut result = VRegVec::new();
            for i in 0..n {
                let comp_idx = pattern[i] as usize;
                result.push(base[comp_idx]);
            }
            Ok(result)
        }
        Expression::AccessIndex { base, index } => {
            if let Some(chain) = crate::lower_struct::peel_arrayofstruct_chain(ctx, expr) {
                return crate::lower_struct::load_array_struct_element(ctx, &chain);
            }
            if let Some((lv, chain)) =
                crate::lower_struct::peel_struct_access_index_chain_to_local(ctx.func, expr)
            {
                if let Some(info) = ctx.aggregate_map.get(&lv).cloned() {
                    if matches!(
                        &info.layout.kind,
                        crate::naga_util::AggregateKind::Struct { .. }
                    ) {
                        return crate::lower_struct::load_struct_path_from_local(
                            ctx, &info, &chain,
                        );
                    }
                }
            }
            if let Some((gv, chain)) =
                crate::lower_struct::peel_struct_access_index_chain_to_global(ctx.func, expr)
            {
                let mut root_ty = ctx.module.global_variables[gv].ty;
                if let TypeInner::Pointer { base: inner, .. } = &ctx.module.types[root_ty].inner {
                    root_ty = *inner;
                }
                if !chain.is_empty()
                    && matches!(&ctx.module.types[root_ty].inner, TypeInner::Struct { .. })
                {
                    if global_peel_chain_ends_at_array_field(&ctx.module, gv, &chain)? {
                        return uniform_global_access_index_path(ctx, expr, gv, &chain);
                    }
                    return crate::lower_struct::load_struct_path_from_global(ctx, gv, &chain);
                }
            }
            if let Some((root, ops)) =
                crate::lower_array_multidim::peel_array_subscript_chain(ctx.func, expr)
            {
                if let Some(info) = ctx.aggregate_info_for_subscript_root(root)? {
                    if matches!(
                        &info.layout.kind,
                        crate::naga_util::AggregateKind::Array { .. }
                    ) {
                        if ops.len() == info.dimensions().len() {
                            if ops.iter().all(|o| {
                                matches!(o, crate::lower_array_multidim::SubscriptOperand::Const(_))
                            }) {
                                use crate::lower_array_multidim::SubscriptOperand::Const as SConst;
                                let idxs: alloc::vec::Vec<u32> = ops
                                    .iter()
                                    .map(|o| match o {
                                        SConst(c) => *c,
                                        _ => 0,
                                    })
                                    .collect();
                                let flat = crate::lower_array_multidim::flat_index_const_clamped(
                                    &info.dimensions(),
                                    &idxs,
                                )?;
                                return crate::lower_array::load_array_element_const(
                                    ctx, &info, flat,
                                );
                            }
                            let flat_v = crate::lower_array::emit_row_major_flat_from_operands(
                                ctx,
                                &info.dimensions(),
                                &ops,
                            )?;
                            return crate::lower_array::load_array_element_dynamic(
                                ctx, &info, flat_v,
                            );
                        }
                        if ops.len() < info.dimensions().len() {
                            return Err(LowerError::UnsupportedExpression(String::from(
                                "partial indexing of multi-dimensional array as rvalue is not supported",
                            )));
                        }
                    }
                }
            }
            let base_inner = expr_type_inner(ctx.module, ctx.func, *base)?;
            match base_inner {
                TypeInner::Vector { .. } => {
                    let base_vs = lower_expr_vec(ctx, *base)?;
                    Ok(smallvec::smallvec![base_vs[*index as usize]])
                }
                TypeInner::Matrix { rows, .. } => {
                    let base_vs = lower_expr_vec(ctx, *base)?;
                    let n = vector_size_usize(rows);
                    let start = (*index as usize) * n;
                    Ok(base_vs[start..start + n].into())
                }
                TypeInner::Struct { .. } => match &ctx.func.expressions[*base] {
                    Expression::GlobalVariable(gv_handle) => {
                        access_index_uniform_global_struct_member(
                            ctx,
                            expr,
                            *gv_handle,
                            *index as usize,
                        )
                    }
                    Expression::LocalVariable(lv) => {
                        if let Some(info) = ctx.aggregate_map.get(lv).cloned() {
                            if matches!(
                                &info.layout.kind,
                                crate::naga_util::AggregateKind::Struct { .. }
                            ) {
                                return crate::lower_struct::load_struct_member_to_vregs(
                                    ctx,
                                    &info,
                                    *index as usize,
                                );
                            }
                        }
                        if let Some(&gv) = ctx.uniform_instance_locals.get(lv) {
                            return access_index_uniform_global_struct_member(
                                ctx,
                                expr,
                                gv,
                                *index as usize,
                            );
                        }
                        Err(LowerError::UnsupportedExpression(String::from(
                            "AccessIndex: struct rvalue LocalVariable not supported",
                        )))
                    }
                    Expression::FunctionArgument(arg_i) => {
                        let arg_ty = ctx
                            .func
                            .arguments
                            .get(*arg_i as usize)
                            .ok_or_else(|| {
                                LowerError::Internal(String::from("bad argument index"))
                            })?
                            .ty;
                        let layout = crate::naga_util::aggregate_layout(ctx.module, arg_ty)?
                            .ok_or_else(|| {
                                LowerError::Internal(String::from("struct member load: layout"))
                            })?;
                        let members = layout.struct_members().ok_or_else(|| {
                            LowerError::Internal(String::from("struct member load: members"))
                        })?;
                        let midx = *index as usize;
                        let m = members.get(midx).ok_or_else(|| {
                            LowerError::UnsupportedExpression(String::from(
                                "struct member index out of range",
                            ))
                        })?;
                        let naga_inner = &ctx.module.types[m.naga_ty].inner;
                        let ir_tys =
                            crate::lower_ctx::naga_type_to_ir_types(ctx.module, naga_inner)?;
                        let base_ptr = ctx.arg_vregs_for(*arg_i)?[0];
                        let mut out = VRegVec::new();
                        for (j, ty) in ir_tys.iter().enumerate() {
                            let dst = ctx.fb.alloc_vreg(*ty);
                            ctx.fb.push(LpirOp::Load {
                                dst,
                                base: base_ptr,
                                offset: m.byte_offset + (j as u32) * 4,
                            });
                            out.push(dst);
                        }
                        Ok(out)
                    }
                    Expression::Load { pointer } => {
                        let root = peel_load_chain(ctx.func, *pointer);
                        match &ctx.func.expressions[root] {
                            Expression::LocalVariable(lv) => {
                                if let Some(info) = ctx.aggregate_map.get(lv).cloned() {
                                    if matches!(
                                        &info.layout.kind,
                                        crate::naga_util::AggregateKind::Struct { .. }
                                    ) {
                                        return crate::lower_struct::load_struct_member_to_vregs(
                                            ctx,
                                            &info,
                                            *index as usize,
                                        );
                                    }
                                }
                                if let Some(&gv) = ctx.uniform_instance_locals.get(lv) {
                                    return access_index_uniform_global_struct_member(
                                        ctx,
                                        expr,
                                        gv,
                                        *index as usize,
                                    );
                                }
                                Err(LowerError::UnsupportedExpression(String::from(
                                    "AccessIndex: struct value behind Load is not a slot-backed struct local",
                                )))
                            }
                            Expression::FunctionArgument(arg_i)
                                if ctx.pointer_args.contains_key(arg_i) =>
                            {
                                let pointee = ctx.pointer_args[arg_i];
                                if let TypeInner::Struct { .. } = &ctx.module.types[pointee].inner {
                                    let layout =
                                        crate::naga_util::aggregate_layout(ctx.module, pointee)?
                                            .ok_or_else(|| {
                                                LowerError::Internal(String::from(
                                                    "struct member load: layout",
                                                ))
                                            })?;
                                    let members = layout.struct_members().ok_or_else(|| {
                                        LowerError::Internal(String::from("struct load members"))
                                    })?;
                                    let midx = *index as usize;
                                    let m = members.get(midx).ok_or_else(|| {
                                        LowerError::UnsupportedExpression(String::from(
                                            "struct member index out of range",
                                        ))
                                    })?;
                                    let naga_inner = &ctx.module.types[m.naga_ty].inner;
                                    let ir_tys = crate::lower_ctx::naga_type_to_ir_types(
                                        ctx.module, naga_inner,
                                    )?;
                                    let base_ptr = ctx.arg_vregs_for(*arg_i)?[0];
                                    let mut out = VRegVec::new();
                                    for (j, ty) in ir_tys.iter().enumerate() {
                                        let dst = ctx.fb.alloc_vreg(*ty);
                                        ctx.fb.push(LpirOp::Load {
                                            dst,
                                            base: base_ptr,
                                            offset: m.byte_offset + (j as u32) * 4,
                                        });
                                        out.push(dst);
                                    }
                                    Ok(out)
                                } else {
                                    Err(LowerError::UnsupportedExpression(String::from(
                                        "AccessIndex: inout param is not pointer-to-struct",
                                    )))
                                }
                            }
                            Expression::GlobalVariable(gv_handle) => {
                                access_index_uniform_global_struct_member(
                                    ctx,
                                    expr,
                                    *gv_handle,
                                    *index as usize,
                                )
                            }
                            Expression::AccessIndex { .. } => {
                                if let Some((gv, mut chain)) =
                                    crate::lower_struct::peel_struct_access_index_chain_to_global(
                                        ctx.func, root,
                                    )
                                {
                                    chain.push(*index);
                                    return uniform_global_access_index_path(ctx, expr, gv, &chain);
                                }
                                Err(LowerError::UnsupportedExpression(String::from(
                                    "AccessIndex: struct value behind Load: uniform chain peel failed",
                                )))
                            }
                            Expression::Access { .. } => {
                                let _ = lower_expr_vec(ctx, root)?;
                                if let Some(UniformVmctxDeferred::ElementAddr {
                                    addr_vreg,
                                    element,
                                }) = ctx.uniform_vmctx_deferred.get(&root.index()).cloned()
                                {
                                    let LpsType::Struct { members, .. } = element else {
                                        return Err(LowerError::Internal(String::from(
                                            "Load uniform element: expected struct",
                                        )));
                                    };
                                    let midx = *index as usize;
                                    let m = members.get(midx).ok_or_else(|| {
                                        LowerError::UnsupportedExpression(String::from(
                                            "struct member index out of range",
                                        ))
                                    })?;
                                    let rel = struct_member_start_offset_u32(
                                        &members,
                                        midx,
                                        LayoutRules::Std430,
                                    )?;
                                    return load_lps_value_from_vmctx_with_base(
                                        ctx, addr_vreg, rel, &m.ty,
                                    );
                                }
                                Err(LowerError::UnsupportedExpression(String::from(
                                    "AccessIndex: struct value behind Load: Access base has no uniform element addr",
                                )))
                            }
                            _ => Err(LowerError::UnsupportedExpression(String::from(
                                "AccessIndex: struct value behind Load is not a slot-backed struct local",
                            ))),
                        }
                    }
                    Expression::CallResult(_) => {
                        let info =
                            ctx.call_result_aggregates
                                .get(base)
                                .cloned()
                                .ok_or_else(|| {
                                    LowerError::Internal(String::from(
                                        "AccessIndex: struct CallResult without aggregate slot",
                                    ))
                                })?;
                        if !matches!(
                            &info.layout.kind,
                            crate::naga_util::AggregateKind::Struct { .. }
                        ) {
                            return Err(LowerError::UnsupportedExpression(String::from(
                                "AccessIndex: expected struct aggregate call result",
                            )));
                        }
                        crate::lower_struct::load_struct_member_to_vregs(
                            ctx,
                            &info,
                            *index as usize,
                        )
                    }
                    Expression::Access { .. } => {
                        if let Some(UniformVmctxDeferred::ElementAddr { addr_vreg, element }) =
                            ctx.uniform_vmctx_deferred.get(&base.index()).cloned()
                        {
                            let LpsType::Struct { members, .. } = element else {
                                return Err(LowerError::Internal(String::from(
                                    "AccessIndex: uniform array element is not a struct",
                                )));
                            };
                            let midx = *index as usize;
                            let m = members.get(midx).ok_or_else(|| {
                                LowerError::UnsupportedExpression(String::from(
                                    "struct member index out of range",
                                ))
                            })?;
                            let rel = struct_member_start_offset_u32(
                                &members,
                                midx,
                                LayoutRules::Std430,
                            )?;
                            return load_lps_value_from_vmctx_with_base(ctx, addr_vreg, rel, &m.ty);
                        }
                        Err(LowerError::UnsupportedExpression(String::from(
                            "AccessIndex: struct rvalue base not supported",
                        )))
                    }
                    _ => Err(LowerError::UnsupportedExpression(String::from(
                        "AccessIndex: struct rvalue base not supported",
                    ))),
                },
                // Naga types locals as `Pointer` to value type; `v.x` is `AccessIndex` on that pointer.
                TypeInner::Pointer { base: ty_h, .. } => {
                    let inner = &ctx.module.types[ty_h].inner;
                    match inner {
                        TypeInner::Vector { scalar, .. } => match &ctx.func.expressions[*base] {
                            Expression::LocalVariable(lv) => {
                                let vs = ctx.resolve_local(*lv)?;
                                let i = *index as usize;
                                let v = *vs.get(i).ok_or_else(|| {
                                    LowerError::UnsupportedExpression(format!(
                                        "AccessIndex index {i} out of range (len {})",
                                        vs.len()
                                    ))
                                })?;
                                Ok(smallvec::smallvec![v])
                            }
                            Expression::FunctionArgument(arg_i)
                                if ctx.pointer_args.contains_key(arg_i) =>
                            {
                                let t = crate::lower_ctx::naga_scalar_to_ir_type(scalar.kind)?;
                                let dst = ctx.fb.alloc_vreg(t);
                                let addr = ctx.arg_vregs_for(*arg_i)?[0];
                                ctx.fb.push(LpirOp::Load {
                                    dst,
                                    base: addr,
                                    offset: *index * 4,
                                });
                                Ok(smallvec::smallvec![dst])
                            }
                            _ => Err(LowerError::UnsupportedExpression(String::from(
                                "AccessIndex: pointer to vector must be local or parameter",
                            ))),
                        },
                        TypeInner::Matrix { rows, .. } => {
                            let Expression::LocalVariable(lv) = &ctx.func.expressions[*base] else {
                                return Err(LowerError::UnsupportedExpression(String::from(
                                    "AccessIndex: matrix pointer base must be LocalVariable",
                                )));
                            };
                            let m = ctx.resolve_local(*lv)?;
                            let n = vector_size_usize(*rows);
                            let start = (*index as usize) * n;
                            Ok(m[start..start + n].into())
                        }
                        TypeInner::Array {
                            base: _elem_ty_h,
                            size,
                            ..
                        } => {
                            let field_source = match &ctx.func.expressions[*base] {
                                Expression::Load { pointer } => peel_load_chain(ctx.func, *pointer),
                                _ => *base,
                            };
                            if let Some(UniformVmctxDeferred::ArrayField {
                                base_offset,
                                element,
                                stride,
                                len,
                            }) = ctx
                                .uniform_vmctx_deferred
                                .get(&field_source.index())
                                .cloned()
                            {
                                let n = match size {
                                    ArraySize::Constant(nz) => nz.get(),
                                    ArraySize::Pending(_) | ArraySize::Dynamic => {
                                        return Err(LowerError::UnsupportedExpression(
                                            String::from(
                                                "AccessIndex on dynamically-sized array pointer",
                                            ),
                                        ));
                                    }
                                };
                                if n == 0 {
                                    return Err(LowerError::UnsupportedExpression(String::from(
                                        "AccessIndex on zero-sized array pointer",
                                    )));
                                }
                                let i =
                                    (*index).min(n.saturating_sub(1)).min(len.saturating_sub(1));
                                let off = base_offset.wrapping_add(i.saturating_mul(stride));
                                return load_lps_value_from_vmctx(ctx, off, &element);
                            }
                            if let Expression::GlobalVariable(gv_handle) =
                                &ctx.func.expressions[field_source]
                            {
                                if let Some(info) = ctx.global_map.get(gv_handle).cloned() {
                                    if let LpsType::Array { element, len } = info.ty {
                                        let n = match size {
                                            ArraySize::Constant(nz) => nz.get(),
                                            ArraySize::Pending(_) | ArraySize::Dynamic => {
                                                return Err(LowerError::UnsupportedExpression(
                                                    String::from(
                                                        "AccessIndex on dynamically-sized array pointer",
                                                    ),
                                                ));
                                            }
                                        };
                                        if n == 0 {
                                            return Err(LowerError::UnsupportedExpression(
                                                String::from(
                                                    "AccessIndex on zero-sized array pointer",
                                                ),
                                            ));
                                        }
                                        let i = (*index)
                                            .min(n.saturating_sub(1))
                                            .min(len.saturating_sub(1));
                                        let stride =
                                            array_stride(&element, LayoutRules::Std430) as u32;
                                        let off =
                                            info.byte_offset.wrapping_add(i.saturating_mul(stride));
                                        return load_lps_value_from_vmctx(ctx, off, &element);
                                    }
                                }
                            }
                            Err(LowerError::UnsupportedExpression(String::from(
                                "AccessIndex: pointer-to-array base is not a deferred uniform field",
                            )))
                        }
                        TypeInner::Struct { .. } => {
                            let base_root = peel_load_chain(ctx.func, *base);
                            match &ctx.func.expressions[base_root] {
                                Expression::GlobalVariable(gv_handle) => {
                                    access_index_uniform_global_struct_member(
                                        ctx,
                                        expr,
                                        *gv_handle,
                                        *index as usize,
                                    )
                                }
                                Expression::LocalVariable(lv) => {
                                    if let Some(info) = ctx.aggregate_map.get(lv).cloned() {
                                        if matches!(
                                            &info.layout.kind,
                                            crate::naga_util::AggregateKind::Struct { .. }
                                        ) {
                                            return crate::lower_struct::load_struct_member_to_vregs(
                                            ctx,
                                            &info,
                                            *index as usize,
                                        );
                                        }
                                    }
                                    if let Some(&gv) = ctx.uniform_instance_locals.get(lv) {
                                        return access_index_uniform_global_struct_member(
                                            ctx,
                                            expr,
                                            gv,
                                            *index as usize,
                                        );
                                    }
                                    Err(LowerError::UnsupportedExpression(String::from(
                                        "AccessIndex: pointer to struct must be a slot-backed struct local",
                                    )))
                                }
                                Expression::FunctionArgument(arg_i)
                                    if ctx.pointer_args.contains_key(arg_i) =>
                                {
                                    let pointee = ctx.pointer_args[arg_i];
                                    if let TypeInner::Struct { .. } =
                                        &ctx.module.types[pointee].inner
                                    {
                                        let layout = crate::naga_util::aggregate_layout(
                                            ctx.module, pointee,
                                        )?
                                        .ok_or_else(|| {
                                            LowerError::Internal(String::from(
                                                "struct member load: layout",
                                            ))
                                        })?;
                                        let members = layout.struct_members().ok_or_else(|| {
                                            LowerError::Internal(String::from(
                                                "struct load members",
                                            ))
                                        })?;
                                        let midx = *index as usize;
                                        let m = members.get(midx).ok_or_else(|| {
                                            LowerError::UnsupportedExpression(String::from(
                                                "struct member index out of range",
                                            ))
                                        })?;
                                        let naga_inner = &ctx.module.types[m.naga_ty].inner;
                                        let ir_tys = crate::lower_ctx::naga_type_to_ir_types(
                                            ctx.module, naga_inner,
                                        )?;
                                        let base = ctx.arg_vregs_for(*arg_i)?[0];
                                        let mut out = VRegVec::new();
                                        for (j, ty) in ir_tys.iter().enumerate() {
                                            let dst = ctx.fb.alloc_vreg(*ty);
                                            ctx.fb.push(LpirOp::Load {
                                                dst,
                                                base,
                                                offset: m.byte_offset + (j as u32) * 4,
                                            });
                                            out.push(dst);
                                        }
                                        return Ok(out);
                                    }
                                    Err(LowerError::UnsupportedExpression(String::from(
                                        "AccessIndex: pointer to struct inout",
                                    )))
                                }
                                Expression::Access { .. } => {
                                    if let Some(UniformVmctxDeferred::ElementAddr {
                                        addr_vreg,
                                        element,
                                    }) =
                                        ctx.uniform_vmctx_deferred.get(&base_root.index()).cloned()
                                    {
                                        let LpsType::Struct { members, .. } = element else {
                                            return Err(LowerError::Internal(String::from(
                                                "AccessIndex: ptr uniform element: expected struct",
                                            )));
                                        };
                                        let midx = *index as usize;
                                        let m = members.get(midx).ok_or_else(|| {
                                            LowerError::UnsupportedExpression(String::from(
                                                "struct member index out of range",
                                            ))
                                        })?;
                                        let rel = struct_member_start_offset_u32(
                                            &members,
                                            midx,
                                            LayoutRules::Std430,
                                        )?;
                                        return load_lps_value_from_vmctx_with_base(
                                            ctx, addr_vreg, rel, &m.ty,
                                        );
                                    }
                                    Err(LowerError::UnsupportedExpression(String::from(
                                        "AccessIndex: pointer to struct: unsupported base",
                                    )))
                                }
                                _ => Err(LowerError::UnsupportedExpression(String::from(
                                    "AccessIndex: pointer to struct: unsupported base",
                                ))),
                            }
                        }
                        _ => Err(LowerError::UnsupportedExpression(format!(
                            "AccessIndex on pointer to {inner:?}"
                        ))),
                    }
                }
                // Matrix column or other vector behind a pointer (Naga `ValuePointer`).
                TypeInner::ValuePointer { size: Some(_), .. } => {
                    let base_vs = lower_expr_vec(ctx, *base)?;
                    let i = *index as usize;
                    let v = *base_vs.get(i).ok_or_else(|| {
                        LowerError::UnsupportedExpression(format!(
                            "AccessIndex index {i} out of range (len {})",
                            base_vs.len()
                        ))
                    })?;
                    Ok(smallvec::smallvec![v])
                }
                // Value array: `in` / `const in T[n] arr` (flattened vregs), or `arr[i]` on a nested
                // array rvalue after an outer `AccessIndex`.
                TypeInner::Array {
                    base: elem_ty_h,
                    size,
                    ..
                } => {
                    let field_source = match &ctx.func.expressions[*base] {
                        Expression::Load { pointer } => peel_load_chain(ctx.func, *pointer),
                        _ => *base,
                    };
                    if let Some(UniformVmctxDeferred::ArrayField {
                        base_offset,
                        element,
                        stride,
                        len,
                    }) = ctx
                        .uniform_vmctx_deferred
                        .get(&field_source.index())
                        .cloned()
                    {
                        let n = match size {
                            ArraySize::Constant(nz) => nz.get(),
                            ArraySize::Pending(_) | ArraySize::Dynamic => {
                                return Err(LowerError::UnsupportedExpression(String::from(
                                    "AccessIndex on dynamically-sized array value",
                                )));
                            }
                        };
                        if n == 0 {
                            return Err(LowerError::UnsupportedExpression(String::from(
                                "AccessIndex on zero-sized array",
                            )));
                        }
                        let i = (*index).min(n.saturating_sub(1)).min(len.saturating_sub(1));
                        let off = base_offset.wrapping_add(i.saturating_mul(stride));
                        return load_lps_value_from_vmctx(ctx, off, &element);
                    }
                    let n = match size {
                        ArraySize::Constant(nz) => nz.get(),
                        ArraySize::Pending(_) | ArraySize::Dynamic => {
                            return Err(LowerError::UnsupportedExpression(String::from(
                                "AccessIndex on dynamically-sized array value",
                            )));
                        }
                    };
                    if n == 0 {
                        return Err(LowerError::UnsupportedExpression(String::from(
                            "AccessIndex on zero-sized array",
                        )));
                    }
                    let elem_inner = &ctx.module.types[elem_ty_h].inner;
                    let elem_ir_count =
                        crate::lower_ctx::naga_type_to_ir_types(ctx.module, elem_inner)?.len();
                    let base_vs = lower_expr_vec(ctx, *base)?;
                    let total = (n as usize).checked_mul(elem_ir_count).ok_or_else(|| {
                        LowerError::Internal(String::from("AccessIndex array: vreg count overflow"))
                    })?;
                    if base_vs.len() != total {
                        return Err(LowerError::Internal(format!(
                            "AccessIndex array: base has {} vregs, expected {} ({} × {} components)",
                            base_vs.len(),
                            total,
                            n,
                            elem_ir_count
                        )));
                    }
                    let i = (*index as usize).min(n as usize - 1);
                    let start = i * elem_ir_count;
                    Ok(base_vs[start..start + elem_ir_count].into())
                }
                _ => Err(LowerError::UnsupportedExpression(format!(
                    "AccessIndex on {base_inner:?}"
                ))),
            }
        }
        Expression::Access { base, .. } => {
            // Handle multi-dimensional array access chains (Access/AccessIndex mixed).
            let base_expr = &ctx.func.expressions[*base];
            let is_access_chain = matches!(base_expr, Expression::Access { .. });
            let is_mixed_chain = matches!(base_expr, Expression::AccessIndex { .. });
            if is_access_chain || is_mixed_chain {
                // Try mixed Access/AccessIndex chain first for arrays.
                if let Some((root, _)) =
                    crate::lower_array_multidim::peel_array_subscript_chain(ctx.func, expr)
                {
                    if ctx.aggregate_info_for_subscript_root(root)?.is_some() {
                        return crate::lower_access::lower_access_expr_vec(ctx, expr);
                    }
                }
                // Fall back to pure Access chain (matrices/vectors).
                if is_access_chain {
                    if let Some((lv, _)) =
                        crate::lower_array_multidim::peel_access_chain(ctx.func, expr)
                    {
                        if ctx.aggregate_map.contains_key(&lv) {
                            return crate::lower_access::lower_access_expr_vec(ctx, expr);
                        }
                    }
                }
                if is_mixed_chain {
                    let field_source = match base_expr {
                        Expression::Load { pointer } => peel_load_chain(ctx.func, *pointer),
                        _ => *base,
                    };
                    // Install `ArrayField` defers (etc.) on `field_source` before lookup — `Access` may be
                    // lowered before the base `AccessIndex` is visited otherwise.
                    let _ = lower_expr_vec(ctx, field_source)?;
                    if let Some(UniformVmctxDeferred::ArrayField {
                        base_offset: field_base,
                        element,
                        stride,
                        ..
                    }) = ctx
                        .uniform_vmctx_deferred
                        .get(&field_source.index())
                        .cloned()
                    {
                        let Expression::Access { index, .. } = &ctx.func.expressions[expr] else {
                            return Err(LowerError::Internal(String::from("Access shape")));
                        };
                        let index_v = ctx.ensure_expr(*index)?;
                        let stride_i = i32::try_from(stride).map_err(|_| {
                            LowerError::Internal(String::from("uniform array stride: ImulImm"))
                        })?;
                        let prod = ctx.fb.alloc_vreg(IrType::I32);
                        ctx.fb.push(LpirOp::ImulImm {
                            dst: prod,
                            src: index_v,
                            imm: stride_i,
                        });
                        let off_imm = i32::try_from(field_base).map_err(|_| {
                            LowerError::Internal(String::from(
                                "uniform array field offset: IaddImm",
                            ))
                        })?;
                        let byte_off = ctx.fb.alloc_vreg(IrType::I32);
                        ctx.fb.push(LpirOp::IaddImm {
                            dst: byte_off,
                            src: prod,
                            imm: off_imm,
                        });
                        let addr = ctx.fb.alloc_vreg(IrType::Pointer);
                        ctx.fb.push(LpirOp::Iadd {
                            dst: addr,
                            lhs: VMCTX_VREG,
                            rhs: byte_off,
                        });
                        ctx.uniform_vmctx_deferred.insert(
                            expr.index(),
                            UniformVmctxDeferred::ElementAddr {
                                addr_vreg: addr,
                                element: element.clone(),
                            },
                        );
                        return Ok(VRegVec::new());
                    }
                }
                // Not an array - handle as matrix/vector dynamic index.
                let col_vs = lower_expr_vec(ctx, *base)?;
                let Expression::Access { index, .. } = &ctx.func.expressions[expr] else {
                    return Err(LowerError::Internal(String::from("Access shape")));
                };
                let index_v = ctx.ensure_expr(*index)?;
                let res_ty = expr_type_inner(ctx.module, ctx.func, expr)?;
                let scalar = match res_ty {
                    TypeInner::Scalar(s) => s,
                    TypeInner::ValuePointer {
                        size: None, scalar, ..
                    } => scalar,
                    _ => {
                        return Err(LowerError::UnsupportedExpression(format!(
                            "nested Access: expected scalar or scalar value pointer, got {res_ty:?}"
                        )));
                    }
                };
                let t = naga_scalar_to_ir_type(scalar.kind)?;
                let out = crate::lower_access::select_lane_dynamic(ctx, &col_vs, index_v, t)?;
                Ok(smallvec::smallvec![out])
            } else {
                crate::lower_access::lower_access_expr_vec(ctx, expr)
            }
        }
        Expression::ZeroValue(ty_h) => lower_zero_value_vec(ctx, *ty_h),
        Expression::Constant(h) => {
            let init = ctx.module.constants[*h].init;
            lower_global_expr_vec(ctx, init)
        }
        Expression::Literal(l) => {
            if let Some(fix) = ctx.array_length_literal_fixes.get(&expr) {
                let v = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(LpirOp::IconstI32 {
                    dst: v,
                    value: *fix,
                });
                return Ok(smallvec::smallvec![v]);
            }
            let v = push_literal(&mut ctx.fb, l)?;
            Ok(smallvec::smallvec![v])
        }
        Expression::Binary { op, left, right } => {
            if matches!(op, BinaryOperator::Equal | BinaryOperator::NotEqual) {
                let left_inner = expr_type_inner(ctx.module, ctx.func, *left)?;
                let right_inner = expr_type_inner(ctx.module, ctx.func, *right)?;
                if let (TypeInner::Array { .. }, TypeInner::Array { .. }) =
                    (&left_inner, &right_inner)
                {
                    return crate::lower_array::lower_array_equality_vec(ctx, *op, *left, *right);
                }
            }
            if *op == BinaryOperator::Multiply {
                let li = expr_type_inner(ctx.module, ctx.func, *left)?;
                let ri = expr_type_inner(ctx.module, ctx.func, *right)?;
                match (&li, &ri) {
                    (
                        TypeInner::Matrix {
                            columns: lc,
                            rows: lr,
                            ..
                        },
                        TypeInner::Vector { size: vs, .. },
                    ) if vector_size_usize(*lc) == vector_size_usize(*vs) => {
                        let mat_vs = lower_expr_vec(ctx, *left)?;
                        let vec_vs = lower_expr_vec(ctx, *right)?;
                        crate::lower_matrix::lower_mat_vec_mul(
                            ctx,
                            &mat_vs,
                            &vec_vs,
                            vector_size_usize(*lc),
                            vector_size_usize(*lr),
                        )
                    }
                    (
                        TypeInner::Vector { size: vs, .. },
                        TypeInner::Matrix {
                            columns: rc,
                            rows: rr,
                            ..
                        },
                    ) if vector_size_usize(*vs) == vector_size_usize(*rr) => {
                        let vec_vs = lower_expr_vec(ctx, *left)?;
                        let mat_vs = lower_expr_vec(ctx, *right)?;
                        crate::lower_matrix::lower_vec_mat_mul(
                            ctx,
                            &vec_vs,
                            &mat_vs,
                            vector_size_usize(*rc),
                            vector_size_usize(*rr),
                        )
                    }
                    (TypeInner::Matrix { .. }, TypeInner::Matrix { .. }) => {
                        let left_vs = lower_expr_vec(ctx, *left)?;
                        let right_vs = lower_expr_vec(ctx, *right)?;
                        let (lc, lr, rc, rr) = matrix_dims(&li, &ri)?;
                        crate::lower_matrix::lower_mat_mat_mul(
                            ctx, &left_vs, &right_vs, lc, lr, rc, rr,
                        )
                    }
                    _ => lower_binary_vec(ctx, *op, *left, *right),
                }
            } else {
                lower_binary_vec(ctx, *op, *left, *right)
            }
        }
        Expression::Unary { op, expr: inner } => lower_unary_vec(ctx, *op, *inner),
        Expression::Select {
            condition,
            accept,
            reject,
        } => lower_select_vec(ctx, *condition, *accept, *reject),
        Expression::As {
            expr: inner,
            kind,
            convert,
        } => {
            // Naga uses 1-byte convert for bool targets; lowering uses I32 truthiness like other scalars.
            if *kind != naga::ScalarKind::Bool && convert.is_some_and(|w| w != 4) {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "As with non-32-bit byte convert",
                )));
            }
            lower_as_vec(ctx, *inner, *kind)
        }
        Expression::Math {
            fun,
            arg,
            arg1,
            arg2,
            arg3,
        } => lower_math::lower_math_vec(ctx, *fun, *arg, *arg1, *arg2, *arg3),
        Expression::Relational { fun, argument } => lower_relational(ctx, *fun, *argument),
        Expression::ArrayLength(array_h) => crate::lower_array::lower_array_length(ctx, *array_h),
        Expression::ImageLoad {
            image,
            coordinate,
            array_index,
            sample,
            level,
        } => {
            if sample.is_some() {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "texelFetch: multisampled image loads are not supported",
                )));
            }
            if array_index.is_some() {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "texelFetch: layered/arrayed image loads are not supported",
                )));
            }
            let Some(level_expr) = level else {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "imageLoad(storage) not supported",
                )));
            };
            crate::lower_texture::lower_image_load_texel_fetch(
                ctx,
                *image,
                *coordinate,
                *level_expr,
            )
        }
        Expression::LocalVariable(_) => Err(LowerError::UnsupportedExpression(String::from(
            "LocalVariable must be used through Load",
        ))),
        Expression::GlobalVariable(gv_handle) => {
            // A bare GlobalVariable expression (used as a pointer, e.g. for Store).
            // Return the info needed to access it later.
            // This should only be reached when the global is used as a pointer (not loaded).
            let Some(info) = ctx.global_map.get(&gv_handle) else {
                return Err(LowerError::Internal(format!(
                    "GlobalVariable {gv_handle:?} not found in global_map"
                )));
            };
            if info.component_count == 1 {
                // Scalar: return the offset as a "virtual vreg" that the Store handler will recognize
                // We encode this as a special pattern - the Store handler will check for this.
                Err(LowerError::UnsupportedExpression(String::from(
                    "GlobalVariable bare expression not fully implemented - use Load/GlobalVariable pattern",
                )))
            } else {
                Err(LowerError::UnsupportedExpression(String::from(
                    "GlobalVariable bare expression for multi-component types not implemented",
                )))
            }
        }
        _ => Err(LowerError::UnsupportedExpression(format!(
            "{:?}",
            ctx.func.expressions[expr]
        ))),
    }
}

/// Repeat a scalar VReg `n` times (same VReg reused).
struct SmallVecRepeat;
impl SmallVecRepeat {
    fn repeat(scalar: VReg, n: usize) -> VRegVec {
        let mut v = VRegVec::new();
        v.resize(n, scalar);
        v
    }
}

fn matrix_dims(
    left: &TypeInner,
    right: &TypeInner,
) -> Result<(usize, usize, usize, usize), LowerError> {
    let TypeInner::Matrix {
        columns: lc,
        rows: lr,
        ..
    } = left
    else {
        return Err(LowerError::Internal(String::from("matrix_dims left")));
    };
    let TypeInner::Matrix {
        columns: rc,
        rows: rr,
        ..
    } = right
    else {
        return Err(LowerError::Internal(String::from("matrix_dims right")));
    };
    Ok((
        vector_size_usize(*lc),
        vector_size_usize(*lr),
        vector_size_usize(*rc),
        vector_size_usize(*rr),
    ))
}

fn lower_zero_value_vec(
    ctx: &mut LowerCtx<'_>,
    ty_h: Handle<naga::Type>,
) -> Result<VRegVec, LowerError> {
    match &ctx.module.types[ty_h].inner {
        TypeInner::Scalar(scalar) => {
            let d = ctx.fb.alloc_vreg(naga_scalar_to_ir_type(scalar.kind)?);
            push_zero_to(ctx, d, scalar.kind)?;
            Ok(smallvec::smallvec![d])
        }
        TypeInner::Vector { size, scalar, .. } => {
            let ir_ty = naga_scalar_to_ir_type(scalar.kind)?;
            let n = vector_size_usize(*size);
            let mut result = VRegVec::new();
            for _ in 0..n {
                let d = ctx.fb.alloc_vreg(ir_ty);
                push_zero_to(ctx, d, scalar.kind)?;
                result.push(d);
            }
            Ok(result)
        }
        TypeInner::Matrix {
            columns,
            rows,
            scalar,
            ..
        } => {
            let ir_ty = naga_scalar_to_ir_type(scalar.kind)?;
            let n = vector_size_usize(*columns) * vector_size_usize(*rows);
            let mut result = VRegVec::new();
            for _ in 0..n {
                let d = ctx.fb.alloc_vreg(ir_ty);
                push_zero_to(ctx, d, scalar.kind)?;
                result.push(d);
            }
            Ok(result)
        }
        _ => Err(LowerError::UnsupportedType(String::from(
            "ZeroValue unsupported type",
        ))),
    }
}

fn push_zero_to(ctx: &mut LowerCtx<'_>, dst: VReg, kind: ScalarKind) -> Result<(), LowerError> {
    match kind {
        ScalarKind::Float => {
            ctx.fb.push(LpirOp::FconstF32 { dst, value: 0.0 });
        }
        ScalarKind::Sint | ScalarKind::Uint | ScalarKind::Bool => {
            ctx.fb.push(LpirOp::IconstI32 { dst, value: 0 });
        }
        ScalarKind::AbstractInt | ScalarKind::AbstractFloat => {
            return Err(LowerError::UnsupportedType(String::from(
                "abstract zero value",
            )));
        }
    }
    Ok(())
}

fn lower_global_expr_vec(
    ctx: &mut LowerCtx<'_>,
    expr: Handle<Expression>,
) -> Result<VRegVec, LowerError> {
    match &ctx.module.global_expressions[expr] {
        Expression::Literal(l) => {
            let v = push_literal(&mut ctx.fb, l)?;
            Ok(smallvec::smallvec![v])
        }
        Expression::Compose { components, .. } => {
            let mut result = VRegVec::new();
            for &c in components {
                result.extend_from_slice(&lower_global_expr_vec(ctx, c)?);
            }
            Ok(result)
        }
        _ => Err(LowerError::UnsupportedExpression(format!(
            "unsupported global expression init {expr:?}"
        ))),
    }
}

fn push_literal(fb: &mut lpir::FunctionBuilder, lit: &Literal) -> Result<VReg, LowerError> {
    match *lit {
        Literal::F32(v) => {
            let d = fb.alloc_vreg(IrType::F32);
            fb.push(LpirOp::FconstF32 { dst: d, value: v });
            Ok(d)
        }
        Literal::I32(v) => {
            let d = fb.alloc_vreg(IrType::I32);
            fb.push(LpirOp::IconstI32 { dst: d, value: v });
            Ok(d)
        }
        Literal::U32(v) => {
            let d = fb.alloc_vreg(IrType::I32);
            fb.push(LpirOp::IconstI32 {
                dst: d,
                value: v as i32,
            });
            Ok(d)
        }
        Literal::Bool(b) => {
            let d = fb.alloc_vreg(IrType::I32);
            fb.push(LpirOp::IconstI32 {
                dst: d,
                value: b as i32,
            });
            Ok(d)
        }
        Literal::F64(v) => {
            let f = v as f32;
            let d = fb.alloc_vreg(IrType::F32);
            fb.push(LpirOp::FconstF32 { dst: d, value: f });
            Ok(d)
        }
        _ => Err(LowerError::UnsupportedExpression(format!(
            "unsupported literal {lit:?}"
        ))),
    }
}

fn lower_select_vec(
    ctx: &mut LowerCtx<'_>,
    condition: Handle<Expression>,
    accept: Handle<Expression>,
    reject: Handle<Expression>,
) -> Result<VRegVec, LowerError> {
    let cond_vs = lower_expr_vec(ctx, condition)?;
    let accept_vs = lower_expr_vec(ctx, accept)?;
    let reject_vs = lower_expr_vec(ctx, reject)?;
    if accept_vs.len() != reject_vs.len() {
        return Err(LowerError::UnsupportedExpression(String::from(
            "select accept/reject width mismatch",
        )));
    }
    let n = accept_vs.len();
    let ty = expr_scalar_kind(ctx.module, ctx.func, accept)?;
    let dst_ty = match ty {
        ScalarKind::Float => IrType::F32,
        _ => IrType::I32,
    };
    let mut result = VRegVec::new();
    for i in 0..n {
        let c = cond_vs[i.min(cond_vs.len().saturating_sub(1).max(0))];
        let dst = ctx.fb.alloc_vreg(dst_ty);
        ctx.fb.push(LpirOp::Select {
            dst,
            cond: c,
            if_true: accept_vs[i],
            if_false: reject_vs[i],
        });
        result.push(dst);
    }
    Ok(result)
}

/// Coerce a lowered value to match a local/result type (implicit conversion on assignment/return).
///
/// When `dst_ty_handle` is `Some`, layout (including fixed-size arrays) uses that type handle.
/// When `None`, `dst_ty_inner` must not be a top-level `Array` — use [`TypeInner`] only for
/// synthetic scalar/vector/matrix shapes.
pub(crate) fn coerce_assignment_vregs(
    ctx: &mut LowerCtx<'_>,
    dst_ty_handle: Option<Handle<naga::Type>>,
    dst_ty_inner: &TypeInner,
    value_expr: Handle<Expression>,
    srcs: VRegVec,
) -> Result<VRegVec, LowerError> {
    let dst_tys: Vec<IrType> = match dst_ty_handle {
        Some(h) => crate::naga_util::ir_types_for_naga_type(ctx.module, h)?,
        None => crate::lower_ctx::naga_type_to_ir_types(ctx.module, dst_ty_inner)?.to_vec(),
    };
    let dst_scalar_kind = match dst_ty_handle {
        Some(h) => type_handle_scalar_kind(ctx.module, h)?,
        None => root_scalar_kind(dst_ty_inner)?,
    };
    // Naga lowers some scalar casts (e.g. `float(bvec2)`) as per-lane vector `Select`/math; the
    // declared type is still scalar. When scalar kinds already match, use the first lane.
    if dst_tys.len() == 1 && srcs.len() > 1 {
        let src_k = expr_scalar_kind(ctx.module, ctx.func, value_expr)?;
        if src_k == dst_scalar_kind {
            return Ok(smallvec::smallvec![srcs[0]]);
        }
    }
    if dst_tys.len() != srcs.len() {
        return Err(LowerError::Internal(format!(
            "assignment component count {} vs {}",
            dst_tys.len(),
            srcs.len()
        )));
    }
    let src_k = expr_scalar_kind(ctx.module, ctx.func, value_expr)?;
    let dst_k = dst_scalar_kind;
    if src_k == dst_k {
        return Ok(srcs);
    }
    let mut out = VRegVec::new();
    for &src in &srcs {
        out.push(lower_as_scalar(ctx, src, src_k, dst_k)?);
    }
    Ok(out)
}

/// Naga `all` / `any` / `isnan` / `isinf` (`Expression::Relational`).
fn lower_relational(
    ctx: &mut LowerCtx<'_>,
    fun: RelationalFunction,
    argument: Handle<Expression>,
) -> Result<VRegVec, LowerError> {
    let arg_vs = lower_expr_vec(ctx, argument)?;
    let k = expr_scalar_kind(ctx.module, ctx.func, argument)?;
    match fun {
        RelationalFunction::All | RelationalFunction::Any => {
            if k != ScalarKind::Bool {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "relational all/any expects bool vector",
                )));
            }
            if arg_vs.is_empty() {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "relational all/any of empty vector",
                )));
            }
            let mut acc = arg_vs[0];
            for i in 1..arg_vs.len() {
                let next = arg_vs[i];
                let d = ctx.fb.alloc_vreg(IrType::I32);
                if fun == RelationalFunction::All {
                    ctx.fb.push(LpirOp::Iand {
                        dst: d,
                        lhs: acc,
                        rhs: next,
                    });
                } else {
                    ctx.fb.push(LpirOp::Ior {
                        dst: d,
                        lhs: acc,
                        rhs: next,
                    });
                }
                acc = d;
            }
            Ok(smallvec::smallvec![acc])
        }
        RelationalFunction::IsNan | RelationalFunction::IsInf => {
            if k != ScalarKind::Float {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "isnan/isinf expect float",
                )));
            }
            // Q32 (and filetest targets): docs/design/q32.md §6 — no NaN/Inf encoding; div0
            // saturation values are not exposed as infinity through `isinf`.
            let mut out = VRegVec::new();
            for _ in 0..arg_vs.len() {
                let b = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(LpirOp::IconstI32 { dst: b, value: 0 });
                out.push(b);
            }
            Ok(out)
        }
    }
}

/// Copy a `Load`'s lowered value into fresh VRegs when the load uses a sub-pointer (`v.x`,
/// `m[i]`, …) that aliases the owning aggregate's mutable slots (same motivation as
/// [`Expression::LocalVariable`] loads above).
fn snapshot_load_result_vregs(
    ctx: &mut LowerCtx<'_>,
    load_expr: Handle<Expression>,
    srcs: VRegVec,
) -> Result<VRegVec, LowerError> {
    let value_inner = expr_type_inner(ctx.module, ctx.func, load_expr)?;
    let ir_tys = crate::lower_ctx::naga_type_to_ir_types(ctx.module, &value_inner)?;
    if srcs.len() != ir_tys.len() {
        return Err(LowerError::Internal(format!(
            "Load snapshot: {} vregs vs {} types for {:?}",
            srcs.len(),
            ir_tys.len(),
            load_expr
        )));
    }
    let mut out = VRegVec::new();
    for (&src, ty) in srcs.iter().zip(ir_tys.iter()) {
        let dst = ctx.fb.alloc_vreg(*ty);
        ctx.fb.push(LpirOp::Copy { dst, src });
        out.push(dst);
    }
    Ok(out)
}

fn peel_load_chain(func: &naga::Function, mut h: Handle<Expression>) -> Handle<Expression> {
    while let Expression::Load { pointer } = &func.expressions[h] {
        h = *pointer;
    }
    h
}

/// True when `indices` (from [`peel_struct_access_index_chain_to_global`]) selects a struct member
/// of array type and does not include a const element index — must use deferred uniform array path.
fn global_peel_chain_ends_at_array_field(
    module: &naga::Module,
    gv: Handle<GlobalVariable>,
    indices: &[u32],
) -> Result<bool, LowerError> {
    if indices.is_empty() {
        return Ok(false);
    }
    let mut naga_ty = module.global_variables[gv].ty;
    if let TypeInner::Pointer { base: inner, .. } = &module.types[naga_ty].inner {
        naga_ty = *inner;
    }
    if !matches!(&module.types[naga_ty].inner, TypeInner::Struct { .. }) {
        return Ok(false);
    }
    let mut i = 0usize;
    while i < indices.len() {
        let layout = crate::naga_util::aggregate_layout(module, naga_ty)?
            .ok_or_else(|| LowerError::Internal(String::from("ends_at_array: layout")))?;
        let members = layout
            .struct_members()
            .ok_or_else(|| LowerError::Internal(String::from("ends_at_array: members")))?;
        let midx = indices[i] as usize;
        let m = members.get(midx).ok_or_else(|| {
            LowerError::UnsupportedExpression(String::from("ends_at_array: member OOB"))
        })?;
        i += 1;
        let mem_inner = &module.types[m.naga_ty].inner;
        match mem_inner {
            TypeInner::Array { .. } => return Ok(i == indices.len()),
            TypeInner::Struct { .. } => {
                naga_ty = m.naga_ty;
            }
            _ => return Ok(false),
        }
    }
    Ok(false)
}

/// Walk `indices` from `gv` (struct member / const array element indices as Naga [`AccessIndex`] chains).
/// Defers when the final step selects an **array-typed struct member** (uniform array field).
fn uniform_global_access_index_path(
    ctx: &mut LowerCtx<'_>,
    access_index_expr: Handle<Expression>,
    gv: Handle<GlobalVariable>,
    indices: &[u32],
) -> Result<VRegVec, LowerError> {
    if indices.is_empty() {
        return Err(LowerError::Internal(String::from(
            "uniform_global_access_index_path: empty path",
        )));
    }
    let ginfo = ctx.global_map.get(&gv).cloned().ok_or_else(|| {
        LowerError::Internal(format!(
            "GlobalVariable {gv:?} not found in global_map (path load)",
        ))
    })?;
    let gv_rec = &ctx.module.global_variables[gv];
    let mut naga_ty = gv_rec.ty;
    if let TypeInner::Pointer { base: inner, .. } = &ctx.module.types[naga_ty].inner {
        naga_ty = *inner;
    }
    if !matches!(&ctx.module.types[naga_ty].inner, TypeInner::Struct { .. }) {
        return Err(LowerError::UnsupportedExpression(String::from(
            "uniform path: root not a struct",
        )));
    }
    let mut off = ginfo.byte_offset;
    let mut i = 0usize;
    while i < indices.len() {
        let layout = crate::naga_util::aggregate_layout(ctx.module, naga_ty)?
            .ok_or_else(|| LowerError::Internal(String::from("uniform path: layout")))?;
        let members = layout
            .struct_members()
            .ok_or_else(|| LowerError::Internal(String::from("uniform path: members")))?;
        let midx = indices[i] as usize;
        let m = members.get(midx).ok_or_else(|| {
            LowerError::UnsupportedExpression(String::from("uniform path: member OOB"))
        })?;
        off = off
            .checked_add(m.byte_offset)
            .ok_or_else(|| LowerError::Internal(String::from("uniform path: offset")))?;
        i += 1;
        let mem_inner = &ctx.module.types[m.naga_ty].inner;
        match mem_inner {
            TypeInner::Array {
                base: elem_ty_h,
                size,
                ..
            } => {
                let n = match size {
                    ArraySize::Constant(nz) => nz.get(),
                    ArraySize::Pending(_) | ArraySize::Dynamic => {
                        return Err(LowerError::UnsupportedExpression(String::from(
                            "uniform path: dynamic outer array in chain",
                        )));
                    }
                };
                if i >= indices.len() {
                    let lps_arr =
                        crate::lower_aggregate_layout::naga_to_lps_type(ctx.module, m.naga_ty)?;
                    if let LpsType::Array { element, len } = &lps_arr {
                        let stride = array_stride(element.as_ref(), LayoutRules::Std430) as u32;
                        ctx.uniform_vmctx_deferred.insert(
                            access_index_expr.index(),
                            UniformVmctxDeferred::ArrayField {
                                base_offset: off,
                                element: (*element.as_ref()).clone(),
                                stride,
                                len: *len,
                            },
                        );
                        return Ok(VRegVec::new());
                    }
                    return Err(LowerError::Internal(String::from(
                        "uniform path: array member without index and not LpsType::Array",
                    )));
                }
                let elem_idx = indices[i];
                i += 1;
                let elem_idx = elem_idx.min(n.saturating_sub(1));
                let lps_elem =
                    crate::lower_aggregate_layout::naga_to_lps_type(ctx.module, *elem_ty_h)?;
                let stride = array_stride(&lps_elem, LayoutRules::Std430) as u32;
                off = off.wrapping_add(elem_idx.saturating_mul(stride));
                naga_ty = *elem_ty_h;
            }
            TypeInner::Struct { .. } => {
                naga_ty = m.naga_ty;
                if i >= indices.len() {
                    let lps = crate::lower_aggregate_layout::naga_to_lps_type(ctx.module, naga_ty)?;
                    return load_lps_value_from_vmctx(ctx, off, &lps);
                }
            }
            _ => {
                if i != indices.len() {
                    return Err(LowerError::Internal(String::from(
                        "uniform path: tail indices on non-aggregate",
                    )));
                }
                let lps = crate::lower_aggregate_layout::naga_to_lps_type(ctx.module, m.naga_ty)?;
                return load_lps_value_from_vmctx(ctx, off, &lps);
            }
        }
    }
    Err(LowerError::Internal(String::from(
        "uniform_global_access_index_path: fallthrough",
    )))
}

/// `AccessIndex` on a uniform struct-typed [`GlobalVariable`] (member may be a deferred array field).
fn access_index_uniform_global_struct_member(
    ctx: &mut LowerCtx<'_>,
    access_index_expr: Handle<Expression>,
    gv_handle: Handle<GlobalVariable>,
    member_index: usize,
) -> Result<VRegVec, LowerError> {
    let (byte_offset, member_ty) = {
        let Some(info) = ctx.global_map.get(&gv_handle) else {
            return Err(LowerError::Internal(format!(
                "GlobalVariable {gv_handle:?} not found in global_map"
            )));
        };
        let LpsType::Struct { members, .. } = &info.ty else {
            return Err(LowerError::Internal(String::from(
                "AccessIndex: uniform global is not a struct",
            )));
        };
        let Some(m) = members.get(member_index) else {
            return Err(LowerError::UnsupportedExpression(format!(
                "AccessIndex struct member index {member_index} out of range (len {})",
                members.len()
            )));
        };
        let rel = struct_member_start_offset_u32(members, member_index, LayoutRules::Std430)?;
        (info.byte_offset.wrapping_add(rel), m.ty.clone())
    };
    if let LpsType::Array { element, len } = &member_ty {
        let rules = LayoutRules::Std430;
        let stride = array_stride(element.as_ref(), rules) as u32;
        ctx.uniform_vmctx_deferred.insert(
            access_index_expr.index(),
            UniformVmctxDeferred::ArrayField {
                base_offset: byte_offset,
                element: (*element.as_ref()).clone(),
                stride,
                len: *len,
            },
        );
        return Ok(VRegVec::new());
    }
    load_lps_value_from_vmctx(ctx, byte_offset, &member_ty)
}

fn round_up_u32_vmctx(size: u32, alignment: u32) -> u32 {
    ((size + alignment - 1) / alignment) * alignment
}

fn struct_member_start_offset_u32(
    members: &[lps_shared::StructMember],
    member_index: usize,
    rules: LayoutRules,
) -> Result<u32, LowerError> {
    if member_index >= members.len() {
        return Err(LowerError::UnsupportedExpression(format!(
            "struct member index {member_index} >= {}",
            members.len()
        )));
    }
    let mut off = 0u32;
    for (i, m) in members.iter().enumerate() {
        let align = type_alignment(&m.ty, rules) as u32;
        off = round_up_u32_vmctx(off, align);
        if i == member_index {
            return Ok(off);
        }
        off = off.wrapping_add(type_size(&m.ty, rules) as u32);
    }
    Err(LowerError::Internal(String::from(
        "struct_member_start_offset_u32: fallthrough",
    )))
}

/// Load a value of `ty` from VMContext at `base_byte_offset` using std430 member layout.
fn load_lps_value_from_vmctx(
    ctx: &mut LowerCtx<'_>,
    base_byte_offset: u32,
    ty: &LpsType,
) -> Result<VRegVec, LowerError> {
    load_lps_value_from_vmctx_with_base(ctx, VMCTX_VREG, base_byte_offset, ty)
}

/// Like [`load_lps_value_from_vmctx`], but loads from `base` + `base_byte_offset` (e.g. dynamic VMContext address).
pub(crate) fn load_lps_value_from_vmctx_with_base(
    ctx: &mut LowerCtx<'_>,
    base: VReg,
    base_byte_offset: u32,
    ty: &LpsType,
) -> Result<VRegVec, LowerError> {
    let rules = LayoutRules::Std430;
    match ty {
        LpsType::Float => {
            let dst = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(LpirOp::Load {
                dst,
                base,
                offset: base_byte_offset,
            });
            Ok(smallvec::smallvec![dst])
        }
        LpsType::Int | LpsType::UInt | LpsType::Bool => {
            let dst = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::Load {
                dst,
                base,
                offset: base_byte_offset,
            });
            Ok(smallvec::smallvec![dst])
        }
        LpsType::Vec2 | LpsType::Vec3 | LpsType::Vec4 => {
            let n = ty.component_count().ok_or_else(|| {
                LowerError::Internal(String::from("load_lps_value_from_vmctx: vector"))
            })? as u32;
            let mut out = VRegVec::new();
            for i in 0..n {
                let dst = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(LpirOp::Load {
                    dst,
                    base,
                    offset: base_byte_offset.wrapping_add(i * 4),
                });
                out.push(dst);
            }
            Ok(out)
        }
        LpsType::IVec2
        | LpsType::IVec3
        | LpsType::IVec4
        | LpsType::UVec2
        | LpsType::UVec3
        | LpsType::UVec4
        | LpsType::BVec2
        | LpsType::BVec3
        | LpsType::BVec4 => {
            let n = ty.component_count().ok_or_else(|| {
                LowerError::Internal(String::from("load_lps_value_from_vmctx: ivec vector"))
            })? as u32;
            let mut out = VRegVec::new();
            for i in 0..n {
                let dst = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(LpirOp::Load {
                    dst,
                    base,
                    offset: base_byte_offset.wrapping_add(i * 4),
                });
                out.push(dst);
            }
            Ok(out)
        }
        LpsType::Texture2D => {
            let mut out = VRegVec::new();
            for i in 0..4 {
                let dst = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(LpirOp::Load {
                    dst,
                    base,
                    offset: base_byte_offset.wrapping_add(i * 4),
                });
                out.push(dst);
            }
            Ok(out)
        }
        LpsType::Mat2 | LpsType::Mat3 | LpsType::Mat4 => {
            let col_ty = ty.matrix_column_type().ok_or_else(|| {
                LowerError::Internal(String::from("load_lps_value_from_vmctx: matrix columns"))
            })?;
            let (cols, _) = ty.matrix_dims().ok_or_else(|| {
                LowerError::Internal(String::from("load_lps_value_from_vmctx: mat"))
            })?;
            let mut out = VRegVec::new();
            let mut col_off: u32 = 0;
            for _ in 0..cols {
                let align = type_alignment(&col_ty, rules) as u32;
                col_off = round_up_u32_vmctx(col_off, align);
                out.extend(load_lps_value_from_vmctx_with_base(
                    ctx,
                    base,
                    base_byte_offset.wrapping_add(col_off),
                    &col_ty,
                )?);
                col_off = col_off.wrapping_add(type_size(&col_ty, rules) as u32);
            }
            Ok(out)
        }
        LpsType::Struct { members, .. } => {
            let mut out = VRegVec::new();
            let mut member_off: u32 = 0;
            for m in members {
                let align = type_alignment(&m.ty, rules) as u32;
                member_off = round_up_u32_vmctx(member_off, align);
                out.extend(load_lps_value_from_vmctx_with_base(
                    ctx,
                    base,
                    base_byte_offset.wrapping_add(member_off),
                    &m.ty,
                )?);
                member_off = member_off.wrapping_add(type_size(&m.ty, rules) as u32);
            }
            Ok(out)
        }
        LpsType::Array { element, len } => {
            let mut out = VRegVec::new();
            let stride = array_stride(element, rules) as u32;
            for i in 0..*len {
                let off = i.saturating_mul(stride);
                out.extend(load_lps_value_from_vmctx_with_base(
                    ctx,
                    base,
                    base_byte_offset.wrapping_add(off),
                    element,
                )?);
            }
            Ok(out)
        }
        LpsType::Void => Ok(VRegVec::new()),
    }
}
