//! Naga [`naga::Expression`] → LPIR ops with vector and matrix scalarization.

use alloc::format;
use alloc::string::String;
use lpir::{IrType, Op, VReg};
use naga::{BinaryOperator, Expression, Handle, Literal, ScalarKind, TypeInner, UnaryOperator};

use crate::expr_scalar::{expr_scalar_kind, expr_type_inner};
use crate::lower_ctx::{LowerCtx, VRegVec, naga_scalar_to_ir_type, vector_size_usize};
use crate::lower_error::LowerError;
use crate::lower_math;

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
            Expression::LocalVariable(lv) => ctx.resolve_local(*lv),
            // Subscripted locals (`m[i][j]`) are pointers in Naga; value lives in vregs.
            Expression::AccessIndex { .. } => lower_expr_vec(ctx, *pointer),
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
                // `mat` local: `t[i]` is a column (vector) in the local's vreg slice.
                TypeInner::Pointer { base: ty_h, .. } => {
                    let Expression::LocalVariable(lv) = &ctx.func.expressions[*base] else {
                        return Err(LowerError::UnsupportedExpression(String::from(
                            "AccessIndex: pointer base must be LocalVariable",
                        )));
                    };
                    let inner = &ctx.module.types[ty_h].inner;
                    let TypeInner::Matrix { rows, .. } = inner else {
                        return Err(LowerError::UnsupportedExpression(format!(
                            "AccessIndex on pointer to non-matrix {inner:?}"
                        )));
                    };
                    let m = ctx.resolve_local(*lv)?;
                    let n = vector_size_usize(*rows);
                    let start = (*index as usize) * n;
                    Ok(m[start..start + n].into())
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
        Expression::Access { .. } => Err(LowerError::UnsupportedExpression(String::from(
            "dynamic vector access not supported",
        ))),
        Expression::ZeroValue(ty_h) => lower_zero_value_vec(ctx, *ty_h),
        Expression::Constant(h) => {
            let init = ctx.module.constants[*h].init;
            lower_global_expr_vec(ctx, init)
        }
        Expression::Literal(l) => {
            let v = push_literal(&mut ctx.fb, l)?;
            Ok(smallvec::smallvec![v])
        }
        Expression::Binary { op, left, right } => {
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

fn lower_binary_vec(
    ctx: &mut LowerCtx<'_>,
    op: BinaryOperator,
    left: Handle<Expression>,
    right: Handle<Expression>,
) -> Result<VRegVec, LowerError> {
    let left_inner = expr_type_inner(ctx.module, ctx.func, left)?;
    let right_inner = expr_type_inner(ctx.module, ctx.func, right)?;
    let lk = expr_scalar_kind(ctx.module, ctx.func, left)?;
    let rk = expr_scalar_kind(ctx.module, ctx.func, right)?;
    if lk != rk {
        return Err(LowerError::UnsupportedExpression(String::from(
            "binary operand kind mismatch",
        )));
    }
    let left_vs = lower_expr_vec(ctx, left)?;
    let right_vs = lower_expr_vec(ctx, right)?;
    let n = left_vs.len().max(right_vs.len());
    if left_vs.len() != right_vs.len() && left_vs.len() != 1 && right_vs.len() != 1 {
        return Err(LowerError::UnsupportedExpression(format!(
            "binary vector width mismatch {} vs {}",
            left_vs.len(),
            right_vs.len()
        )));
    }
    let mut result = VRegVec::new();
    for i in 0..n {
        let l = left_vs[i.min(left_vs.len().saturating_sub(1).max(0))];
        let r = right_vs[i.min(right_vs.len().saturating_sub(1).max(0))];
        let v = lower_binary_scalar(ctx, op, l, r, lk, &left_inner, &right_inner)?;
        result.push(v);
    }
    Ok(result)
}

fn lower_binary_scalar(
    ctx: &mut LowerCtx<'_>,
    op: BinaryOperator,
    lhs: VReg,
    rhs: VReg,
    lk: ScalarKind,
    _left_ty: &TypeInner,
    _right_ty: &TypeInner,
) -> Result<VReg, LowerError> {
    match lk {
        ScalarKind::Float => lower_binary_float(ctx, op, lhs, rhs),
        ScalarKind::Sint => lower_binary_sint(ctx, op, lhs, rhs),
        ScalarKind::Uint => lower_binary_uint(ctx, op, lhs, rhs),
        ScalarKind::Bool => lower_binary_bool(ctx, op, lhs, rhs),
        ScalarKind::AbstractInt | ScalarKind::AbstractFloat => Err(LowerError::UnsupportedType(
            String::from("abstract binary op"),
        )),
    }
}

fn lower_binary_float(
    ctx: &mut LowerCtx<'_>,
    op: BinaryOperator,
    lhs: VReg,
    rhs: VReg,
) -> Result<VReg, LowerError> {
    if op == BinaryOperator::Modulo {
        return lower_float_modulo(ctx, lhs, rhs);
    }
    let dst_ty = match op {
        BinaryOperator::Equal
        | BinaryOperator::NotEqual
        | BinaryOperator::Less
        | BinaryOperator::LessEqual
        | BinaryOperator::Greater
        | BinaryOperator::GreaterEqual => IrType::I32,
        _ => IrType::F32,
    };
    let dst = ctx.fb.alloc_vreg(dst_ty);
    match op {
        BinaryOperator::Add => ctx.fb.push(Op::Fadd { dst, lhs, rhs }),
        BinaryOperator::Subtract => ctx.fb.push(Op::Fsub { dst, lhs, rhs }),
        BinaryOperator::Multiply => ctx.fb.push(Op::Fmul { dst, lhs, rhs }),
        BinaryOperator::Divide => ctx.fb.push(Op::Fdiv { dst, lhs, rhs }),
        BinaryOperator::Equal => ctx.fb.push(Op::Feq { dst, lhs, rhs }),
        BinaryOperator::NotEqual => ctx.fb.push(Op::Fne { dst, lhs, rhs }),
        BinaryOperator::Less => ctx.fb.push(Op::Flt { dst, lhs, rhs }),
        BinaryOperator::LessEqual => ctx.fb.push(Op::Fle { dst, lhs, rhs }),
        BinaryOperator::Greater => ctx.fb.push(Op::Fgt { dst, lhs, rhs }),
        BinaryOperator::GreaterEqual => ctx.fb.push(Op::Fge { dst, lhs, rhs }),
        _ => {
            return Err(LowerError::UnsupportedExpression(format!(
                "unsupported float binary {op:?}"
            )));
        }
    }
    Ok(dst)
}

fn lower_float_modulo(ctx: &mut LowerCtx<'_>, x: VReg, y: VReg) -> Result<VReg, LowerError> {
    let v_div = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fdiv {
        dst: v_div,
        lhs: x,
        rhs: y,
    });
    let v_fl = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Ffloor {
        dst: v_fl,
        src: v_div,
    });
    let v_mul = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fmul {
        dst: v_mul,
        lhs: v_fl,
        rhs: y,
    });
    let dst = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fsub {
        dst,
        lhs: x,
        rhs: v_mul,
    });
    Ok(dst)
}

fn lower_binary_sint(
    ctx: &mut LowerCtx<'_>,
    op: BinaryOperator,
    lhs: VReg,
    rhs: VReg,
) -> Result<VReg, LowerError> {
    let dst = ctx.fb.alloc_vreg(IrType::I32);
    match op {
        BinaryOperator::Add => ctx.fb.push(Op::Iadd { dst, lhs, rhs }),
        BinaryOperator::Subtract => ctx.fb.push(Op::Isub { dst, lhs, rhs }),
        BinaryOperator::Multiply => ctx.fb.push(Op::Imul { dst, lhs, rhs }),
        BinaryOperator::Divide => ctx.fb.push(Op::IdivS { dst, lhs, rhs }),
        BinaryOperator::Modulo => ctx.fb.push(Op::IremS { dst, lhs, rhs }),
        BinaryOperator::Equal => ctx.fb.push(Op::Ieq { dst, lhs, rhs }),
        BinaryOperator::NotEqual => ctx.fb.push(Op::Ine { dst, lhs, rhs }),
        BinaryOperator::Less => ctx.fb.push(Op::IltS { dst, lhs, rhs }),
        BinaryOperator::LessEqual => ctx.fb.push(Op::IleS { dst, lhs, rhs }),
        BinaryOperator::Greater => ctx.fb.push(Op::IgtS { dst, lhs, rhs }),
        BinaryOperator::GreaterEqual => ctx.fb.push(Op::IgeS { dst, lhs, rhs }),
        BinaryOperator::And => ctx.fb.push(Op::Iand { dst, lhs, rhs }),
        BinaryOperator::InclusiveOr => ctx.fb.push(Op::Ior { dst, lhs, rhs }),
        BinaryOperator::ExclusiveOr => ctx.fb.push(Op::Ixor { dst, lhs, rhs }),
        BinaryOperator::ShiftLeft => ctx.fb.push(Op::Ishl { dst, lhs, rhs }),
        BinaryOperator::ShiftRight => ctx.fb.push(Op::IshrS { dst, lhs, rhs }),
        _ => {
            return Err(LowerError::UnsupportedExpression(format!(
                "unsupported sint binary {op:?}"
            )));
        }
    }
    Ok(dst)
}

fn lower_binary_uint(
    ctx: &mut LowerCtx<'_>,
    op: BinaryOperator,
    lhs: VReg,
    rhs: VReg,
) -> Result<VReg, LowerError> {
    let dst = ctx.fb.alloc_vreg(IrType::I32);
    match op {
        BinaryOperator::Add => ctx.fb.push(Op::Iadd { dst, lhs, rhs }),
        BinaryOperator::Subtract => ctx.fb.push(Op::Isub { dst, lhs, rhs }),
        BinaryOperator::Multiply => ctx.fb.push(Op::Imul { dst, lhs, rhs }),
        BinaryOperator::Divide => ctx.fb.push(Op::IdivU { dst, lhs, rhs }),
        BinaryOperator::Modulo => ctx.fb.push(Op::IremU { dst, lhs, rhs }),
        BinaryOperator::Equal => ctx.fb.push(Op::Ieq { dst, lhs, rhs }),
        BinaryOperator::NotEqual => ctx.fb.push(Op::Ine { dst, lhs, rhs }),
        BinaryOperator::Less => ctx.fb.push(Op::IltU { dst, lhs, rhs }),
        BinaryOperator::LessEqual => ctx.fb.push(Op::IleU { dst, lhs, rhs }),
        BinaryOperator::Greater => ctx.fb.push(Op::IgtU { dst, lhs, rhs }),
        BinaryOperator::GreaterEqual => ctx.fb.push(Op::IgeU { dst, lhs, rhs }),
        BinaryOperator::And => ctx.fb.push(Op::Iand { dst, lhs, rhs }),
        BinaryOperator::InclusiveOr => ctx.fb.push(Op::Ior { dst, lhs, rhs }),
        BinaryOperator::ExclusiveOr => ctx.fb.push(Op::Ixor { dst, lhs, rhs }),
        BinaryOperator::ShiftLeft => ctx.fb.push(Op::Ishl { dst, lhs, rhs }),
        BinaryOperator::ShiftRight => ctx.fb.push(Op::IshrU { dst, lhs, rhs }),
        _ => {
            return Err(LowerError::UnsupportedExpression(format!(
                "unsupported uint binary {op:?}"
            )));
        }
    }
    Ok(dst)
}

fn lower_binary_bool(
    ctx: &mut LowerCtx<'_>,
    op: BinaryOperator,
    lhs: VReg,
    rhs: VReg,
) -> Result<VReg, LowerError> {
    let dst = ctx.fb.alloc_vreg(IrType::I32);
    match op {
        BinaryOperator::LogicalAnd | BinaryOperator::And => ctx.fb.push(Op::Iand { dst, lhs, rhs }),
        BinaryOperator::LogicalOr | BinaryOperator::InclusiveOr => {
            ctx.fb.push(Op::Ior { dst, lhs, rhs })
        }
        BinaryOperator::ExclusiveOr => ctx.fb.push(Op::Ixor { dst, lhs, rhs }),
        BinaryOperator::Equal => ctx.fb.push(Op::Ieq { dst, lhs, rhs }),
        BinaryOperator::NotEqual => ctx.fb.push(Op::Ine { dst, lhs, rhs }),
        _ => {
            return Err(LowerError::UnsupportedExpression(format!(
                "unsupported bool binary {op:?}"
            )));
        }
    }
    Ok(dst)
}

fn lower_unary_vec(
    ctx: &mut LowerCtx<'_>,
    op: UnaryOperator,
    inner: Handle<Expression>,
) -> Result<VRegVec, LowerError> {
    let inner_vs = lower_expr_vec(ctx, inner)?;
    let k = expr_scalar_kind(ctx.module, ctx.func, inner)?;
    let mut result = VRegVec::new();
    for &src in &inner_vs {
        let v = lower_unary_scalar(ctx, op, src, k)?;
        result.push(v);
    }
    Ok(result)
}

fn lower_unary_scalar(
    ctx: &mut LowerCtx<'_>,
    op: UnaryOperator,
    src: VReg,
    k: ScalarKind,
) -> Result<VReg, LowerError> {
    let dst = match op {
        UnaryOperator::LogicalNot => {
            if k != ScalarKind::Bool {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "logical not on non-bool",
                )));
            }
            ctx.fb.alloc_vreg(IrType::I32)
        }
        UnaryOperator::BitwiseNot => {
            if k != ScalarKind::Sint && k != ScalarKind::Uint {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "bitwise not on non-integer",
                )));
            }
            ctx.fb.alloc_vreg(IrType::I32)
        }
        UnaryOperator::Negate => match k {
            ScalarKind::Float => ctx.fb.alloc_vreg(IrType::F32),
            ScalarKind::Sint | ScalarKind::Uint | ScalarKind::Bool => {
                ctx.fb.alloc_vreg(IrType::I32)
            }
            ScalarKind::AbstractInt | ScalarKind::AbstractFloat => {
                return Err(LowerError::UnsupportedType(String::from("abstract unary")));
            }
        },
    };
    match op {
        UnaryOperator::Negate => match k {
            ScalarKind::Float => ctx.fb.push(Op::Fneg { dst, src }),
            ScalarKind::Sint | ScalarKind::Uint | ScalarKind::Bool => {
                ctx.fb.push(Op::Ineg { dst, src })
            }
            _ => {}
        },
        UnaryOperator::LogicalNot => {
            ctx.fb.push(Op::IeqImm { dst, src, imm: 0 });
        }
        UnaryOperator::BitwiseNot => ctx.fb.push(Op::Ibnot { dst, src }),
    }
    Ok(dst)
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
pub(crate) fn coerce_assignment_vregs(
    ctx: &mut LowerCtx<'_>,
    dst_ty_inner: &TypeInner,
    value_expr: Handle<Expression>,
    srcs: VRegVec,
) -> Result<VRegVec, LowerError> {
    let dst_tys = crate::lower_ctx::naga_type_to_ir_types(dst_ty_inner)?;
    if dst_tys.len() != srcs.len() {
        return Err(LowerError::Internal(format!(
            "assignment component count {} vs {}",
            dst_tys.len(),
            srcs.len()
        )));
    }
    let src_k = expr_scalar_kind(ctx.module, ctx.func, value_expr)?;
    let dst_k = root_scalar_kind(dst_ty_inner)?;
    if src_k == dst_k {
        return Ok(srcs);
    }
    let mut out = VRegVec::new();
    for &src in &srcs {
        out.push(lower_as_scalar(ctx, src, src_k, dst_k)?);
    }
    Ok(out)
}

fn root_scalar_kind(inner: &TypeInner) -> Result<ScalarKind, LowerError> {
    match *inner {
        TypeInner::Scalar(s) => Ok(s.kind),
        TypeInner::Vector { scalar, .. } | TypeInner::Matrix { scalar, .. } => Ok(scalar.kind),
        _ => Err(LowerError::Internal(String::from(
            "root_scalar_kind: expected scalar, vector, or matrix",
        ))),
    }
}

fn lower_as_vec(
    ctx: &mut LowerCtx<'_>,
    inner: Handle<Expression>,
    target: ScalarKind,
) -> Result<VRegVec, LowerError> {
    let inner_vs = lower_expr_vec(ctx, inner)?;
    let src_k = expr_scalar_kind(ctx.module, ctx.func, inner)?;
    if src_k == target {
        return Ok(inner_vs);
    }
    let mut result = VRegVec::new();
    for &src in &inner_vs {
        let v = lower_as_scalar(ctx, src, src_k, target)?;
        result.push(v);
    }
    Ok(result)
}

fn lower_as_scalar(
    ctx: &mut LowerCtx<'_>,
    v: VReg,
    src_k: ScalarKind,
    target: ScalarKind,
) -> Result<VReg, LowerError> {
    let dst_ty = match target {
        ScalarKind::Float => IrType::F32,
        ScalarKind::Sint | ScalarKind::Uint | ScalarKind::Bool => IrType::I32,
        ScalarKind::AbstractInt | ScalarKind::AbstractFloat => {
            return Err(LowerError::UnsupportedType(String::from(
                "abstract As target",
            )));
        }
    };
    let dst = ctx.fb.alloc_vreg(dst_ty);
    match (src_k, target) {
        (ScalarKind::Float, ScalarKind::Sint) => ctx.fb.push(Op::FtoiSatS { dst, src: v }),
        (ScalarKind::Float, ScalarKind::Uint) => ctx.fb.push(Op::FtoiSatU { dst, src: v }),
        (ScalarKind::Sint, ScalarKind::Float) | (ScalarKind::Bool, ScalarKind::Float) => {
            ctx.fb.push(Op::ItofS { dst, src: v })
        }
        (ScalarKind::Uint, ScalarKind::Float) => ctx.fb.push(Op::ItofU { dst, src: v }),
        (ScalarKind::Sint, ScalarKind::Uint) | (ScalarKind::Uint, ScalarKind::Sint) => {
            ctx.fb.push(Op::Copy { dst, src: v })
        }
        (ScalarKind::Bool, ScalarKind::Sint) | (ScalarKind::Bool, ScalarKind::Uint) => {
            ctx.fb.push(Op::Copy { dst, src: v })
        }
        (ScalarKind::Sint, ScalarKind::Bool) | (ScalarKind::Uint, ScalarKind::Bool) => {
            let z = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(Op::IconstI32 { dst: z, value: 0 });
            ctx.fb.push(Op::Ine {
                dst,
                lhs: v,
                rhs: z,
            });
        }
        (ScalarKind::Float, ScalarKind::Bool) => {
            let z = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(Op::FconstF32 { dst: z, value: 0.0 });
            ctx.fb.push(Op::Fne {
                dst,
                lhs: v,
                rhs: z,
            });
        }
        _ => {
            return Err(LowerError::UnsupportedExpression(format!(
                "unsupported cast {src_k:?} -> {target:?}"
            )));
        }
    }
    Ok(dst)
}
