//! Naga [`naga::Expression`] → LPIR ops with vector and matrix scalarization.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use lpir::{IrType, Op, VReg};
use naga::{
    BinaryOperator, Expression, Handle, Literal, RelationalFunction, ScalarKind, TypeInner,
};

use crate::lower_binary::lower_binary_vec;
use crate::lower_cast::{lower_as_scalar, lower_as_vec, root_scalar_kind};
use crate::lower_ctx::{LowerCtx, VRegVec, naga_scalar_to_ir_type, vector_size_usize};
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
                if ctx.array_map.contains_key(lv) {
                    return Err(LowerError::UnsupportedExpression(String::from(
                        "Load of whole array local is not supported",
                    )));
                }
                // Snapshot into fresh VRegs so the loaded value does not alias the local's
                // mutable slots (needed for postfix ++/-- and any use-after-store of the same
                // Load expression handle).
                let srcs = ctx.resolve_local(*lv)?;
                let lv_ty = &ctx.module.types[ctx.func.local_variables[*lv].ty].inner;
                let ir_tys = crate::lower_ctx::naga_type_to_ir_types(lv_ty)?;
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
                    ctx.fb.push(Op::Copy { dst, src });
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
                let ir_tys = crate::lower_ctx::naga_type_to_ir_types(base_inner)?;
                let addr = ctx.arg_vregs_for(idx)?[0];
                let mut vregs = VRegVec::new();
                for (j, ty) in ir_tys.iter().enumerate() {
                    let dst = ctx.fb.alloc_vreg(*ty);
                    ctx.fb.push(Op::Load {
                        dst,
                        base: addr,
                        offset: (j * 4) as u32,
                    });
                    vregs.push(dst);
                }
                Ok(vregs)
            }
            _ => Err(LowerError::UnsupportedExpression(String::from(
                "Load from non-local pointer",
            ))),
        },
        Expression::CallResult(_) => {
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
            if let Some((root, ops)) =
                crate::lower_array_multidim::peel_array_subscript_chain(ctx.func, expr)
            {
                if let Some(info) = ctx.array_info_for_subscript_root(root)? {
                    if ops.len() == info.dimensions.len() {
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
                                &info.dimensions,
                                &idxs,
                            )?;
                            return crate::lower_array::load_array_element_const(ctx, &info, flat);
                        }
                        let flat_v = crate::lower_array::emit_row_major_flat_from_operands(
                            ctx,
                            &info.dimensions,
                            &ops,
                        )?;
                        return crate::lower_array::load_array_element_dynamic(ctx, &info, flat_v);
                    }
                    if ops.len() < info.dimensions.len() {
                        return Err(LowerError::UnsupportedExpression(String::from(
                            "partial indexing of multi-dimensional array as rvalue is not supported",
                        )));
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
                                ctx.fb.push(Op::Load {
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
                    if ctx.array_info_for_subscript_root(root)?.is_some() {
                        return crate::lower_access::lower_access_expr_vec(ctx, expr);
                    }
                }
                // Fall back to pure Access chain (matrices/vectors).
                if is_access_chain {
                    if let Some((lv, _)) =
                        crate::lower_array_multidim::peel_access_chain(ctx.func, expr)
                    {
                        if ctx.array_map.contains_key(&lv) {
                            return crate::lower_access::lower_access_expr_vec(ctx, expr);
                        }
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
                ctx.fb.push(Op::IconstI32 {
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
        Expression::LocalVariable(_) => Err(LowerError::UnsupportedExpression(String::from(
            "LocalVariable must be used through Load",
        ))),
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
            ctx.fb.push(Op::FconstF32 { dst, value: 0.0 });
        }
        ScalarKind::Sint | ScalarKind::Uint | ScalarKind::Bool => {
            ctx.fb.push(Op::IconstI32 { dst, value: 0 });
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
            fb.push(Op::FconstF32 { dst: d, value: v });
            Ok(d)
        }
        Literal::I32(v) => {
            let d = fb.alloc_vreg(IrType::I32);
            fb.push(Op::IconstI32 { dst: d, value: v });
            Ok(d)
        }
        Literal::U32(v) => {
            let d = fb.alloc_vreg(IrType::I32);
            fb.push(Op::IconstI32 {
                dst: d,
                value: v as i32,
            });
            Ok(d)
        }
        Literal::Bool(b) => {
            let d = fb.alloc_vreg(IrType::I32);
            fb.push(Op::IconstI32 {
                dst: d,
                value: b as i32,
            });
            Ok(d)
        }
        Literal::F64(v) => {
            let f = v as f32;
            let d = fb.alloc_vreg(IrType::F32);
            fb.push(Op::FconstF32 { dst: d, value: f });
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
        ctx.fb.push(Op::Select {
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
        None => crate::lower_ctx::naga_type_to_ir_types(dst_ty_inner)?.to_vec(),
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
                    ctx.fb.push(Op::Iand {
                        dst: d,
                        lhs: acc,
                        rhs: next,
                    });
                } else {
                    ctx.fb.push(Op::Ior {
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
                ctx.fb.push(Op::IconstI32 { dst: b, value: 0 });
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
    let ir_tys = crate::lower_ctx::naga_type_to_ir_types(&value_inner)?;
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
        ctx.fb.push(Op::Copy { dst, src });
        out.push(dst);
    }
    Ok(out)
}
