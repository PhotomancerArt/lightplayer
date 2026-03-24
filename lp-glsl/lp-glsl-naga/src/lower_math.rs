//! Naga [`Expression::Math`] → LPIR: inline ops, `@std.math` imports, per-component vector math,
//! and geometry/matrix builtins (`dot`, `normalize`, `transpose`, etc.).

use alloc::format;
use alloc::string::String;

use lpir::{IrType, Op, VReg};
use naga::{
    BinaryOperator, Expression, Function, Handle, MathFunction, Module, ScalarKind, TypeInner,
};

use crate::expr_scalar::{expr_scalar_kind, expr_type_inner};
use crate::lower_ctx::{LowerCtx, VRegVec, naga_type_width};
use crate::lower_error::LowerError;
use crate::lower_matrix;

pub(crate) fn lower_math_vec(
    ctx: &mut LowerCtx<'_>,
    fun: MathFunction,
    arg: Handle<naga::Expression>,
    arg1: Option<Handle<naga::Expression>>,
    arg2: Option<Handle<naga::Expression>>,
    arg3: Option<Handle<naga::Expression>>,
) -> Result<VRegVec, LowerError> {
    match fun {
        MathFunction::Dot => {
            let a = ctx.ensure_expr_vec(arg)?;
            let b = ctx
                .ensure_expr_vec(arg1.ok_or_else(|| LowerError::Internal(String::from("dot")))?)?;
            let d = lower_matrix::emit_dot_product(ctx, &a, &b)?;
            Ok(smallvec::smallvec![d])
        }
        MathFunction::Cross => {
            let a = ctx.ensure_expr_vec(arg)?;
            let b = ctx.ensure_expr_vec(
                arg1.ok_or_else(|| LowerError::Internal(String::from("cross")))?,
            )?;
            if a.len() != 3 || b.len() != 3 {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "cross expects vec3",
                )));
            }
            emit_cross(ctx, &a, &b)
        }
        MathFunction::Length => {
            let v = ctx.ensure_expr_vec(arg)?;
            let d = lower_matrix::emit_dot_product(ctx, &v, &v)?;
            let r = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(Op::Fsqrt { dst: r, src: d });
            Ok(smallvec::smallvec![r])
        }
        MathFunction::Distance => {
            let a = ctx.ensure_expr_vec(arg)?;
            let b = ctx.ensure_expr_vec(
                arg1.ok_or_else(|| LowerError::Internal(String::from("distance")))?,
            )?;
            if a.len() != b.len() {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "distance length mismatch",
                )));
            }
            let mut diffs = VRegVec::new();
            for i in 0..a.len() {
                let d = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(Op::Fsub {
                    dst: d,
                    lhs: a[i],
                    rhs: b[i],
                });
                diffs.push(d);
            }
            let d = lower_matrix::emit_dot_product(ctx, &diffs, &diffs)?;
            let r = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(Op::Fsqrt { dst: r, src: d });
            Ok(smallvec::smallvec![r])
        }
        MathFunction::Normalize => {
            let v = ctx.ensure_expr_vec(arg)?;
            let len = {
                let d = lower_matrix::emit_dot_product(ctx, &v, &v)?;
                let r = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(Op::Fsqrt { dst: r, src: d });
                r
            };
            let mut out = VRegVec::new();
            for &c in &v {
                let d = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(Op::Fdiv {
                    dst: d,
                    lhs: c,
                    rhs: len,
                });
                out.push(d);
            }
            Ok(out)
        }
        MathFunction::FaceForward => {
            let n = ctx.ensure_expr_vec(arg)?;
            let i =
                ctx.ensure_expr_vec(arg1.ok_or_else(|| LowerError::Internal(String::from("ff")))?)?;
            let nref =
                ctx.ensure_expr_vec(arg2.ok_or_else(|| LowerError::Internal(String::from("ff")))?)?;
            let d = lower_matrix::emit_dot_product(ctx, &nref, &i)?;
            let z = fconst(ctx, 0.0);
            let cmp = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(Op::Flt {
                dst: cmp,
                lhs: d,
                rhs: z,
            });
            let mut out = VRegVec::new();
            for j in 0..n.len() {
                let neg = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(Op::Fneg {
                    dst: neg,
                    src: n[j],
                });
                let dst = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(Op::Select {
                    dst,
                    cond: cmp,
                    if_true: n[j],
                    if_false: neg,
                });
                out.push(dst);
            }
            Ok(out)
        }
        MathFunction::Reflect => {
            let i = ctx.ensure_expr_vec(arg)?;
            let n = ctx.ensure_expr_vec(
                arg1.ok_or_else(|| LowerError::Internal(String::from("reflect")))?,
            )?;
            let two = fconst(ctx, 2.0);
            let ndi = lower_matrix::emit_dot_product(ctx, &n, &i)?;
            let scale = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(Op::Fmul {
                dst: scale,
                lhs: two,
                rhs: ndi,
            });
            let mut out = VRegVec::new();
            for j in 0..i.len() {
                let pn = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(Op::Fmul {
                    dst: pn,
                    lhs: scale,
                    rhs: n[j],
                });
                let dst = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(Op::Fsub {
                    dst,
                    lhs: i[j],
                    rhs: pn,
                });
                out.push(dst);
            }
            Ok(out)
        }
        MathFunction::Refract => {
            let i = ctx.ensure_expr_vec(arg)?;
            let n = ctx.ensure_expr_vec(
                arg1.ok_or_else(|| LowerError::Internal(String::from("refract")))?,
            )?;
            let eta_v = ctx
                .ensure_expr(arg2.ok_or_else(|| LowerError::Internal(String::from("refract")))?)?;
            let one = fconst(ctx, 1.0);
            let ndi = lower_matrix::emit_dot_product(ctx, &n, &i)?;
            let ndi2 = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(Op::Fmul {
                dst: ndi2,
                lhs: ndi,
                rhs: ndi,
            });
            let t1 = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(Op::Fsub {
                dst: t1,
                lhs: one,
                rhs: ndi2,
            });
            let eta2 = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(Op::Fmul {
                dst: eta2,
                lhs: eta_v,
                rhs: eta_v,
            });
            let k_inner = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(Op::Fmul {
                dst: k_inner,
                lhs: eta2,
                rhs: t1,
            });
            let k = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(Op::Fsub {
                dst: k,
                lhs: one,
                rhs: k_inner,
            });
            let z = fconst(ctx, 0.0);
            let k_neg = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(Op::Flt {
                dst: k_neg,
                lhs: k,
                rhs: z,
            });
            let mut out = VRegVec::new();
            for j in 0..i.len() {
                let etai = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(Op::Fmul {
                    dst: etai,
                    lhs: eta_v,
                    rhs: i[j],
                });
                let root = push_std_math(ctx, "sqrt", &[k])?;
                let eta_ndi = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(Op::Fmul {
                    dst: eta_ndi,
                    lhs: eta_v,
                    rhs: ndi,
                });
                let sum = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(Op::Fadd {
                    dst: sum,
                    lhs: eta_ndi,
                    rhs: root,
                });
                let pn = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(Op::Fmul {
                    dst: pn,
                    lhs: sum,
                    rhs: n[j],
                });
                let refr = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(Op::Fsub {
                    dst: refr,
                    lhs: etai,
                    rhs: pn,
                });
                let zero = fconst(ctx, 0.0);
                let dst = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(Op::Select {
                    dst,
                    cond: k_neg,
                    if_true: zero,
                    if_false: refr,
                });
                out.push(dst);
            }
            Ok(out)
        }
        MathFunction::Transpose => {
            let inner = expr_type_inner(ctx.module, ctx.func, arg)?;
            let TypeInner::Matrix { columns, rows, .. } = inner else {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "transpose non-matrix",
                )));
            };
            let lc = crate::lower_ctx::vector_size_usize(columns);
            let lr = crate::lower_ctx::vector_size_usize(rows);
            let v = ctx.ensure_expr_vec(arg)?;
            Ok(lower_matrix::lower_transpose(&v, lc, lr))
        }
        MathFunction::Determinant => {
            let inner = expr_type_inner(ctx.module, ctx.func, arg)?;
            let TypeInner::Matrix { columns, rows, .. } = inner else {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "determinant non-matrix",
                )));
            };
            let lc = crate::lower_ctx::vector_size_usize(columns);
            let lr = crate::lower_ctx::vector_size_usize(rows);
            if lc != lr {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "determinant non-square",
                )));
            }
            let v = ctx.ensure_expr_vec(arg)?;
            let d = lower_matrix::lower_determinant(ctx, &v, lc)?;
            Ok(smallvec::smallvec![d])
        }
        MathFunction::Inverse => {
            let inner = expr_type_inner(ctx.module, ctx.func, arg)?;
            let TypeInner::Matrix { columns, rows, .. } = inner else {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "inverse non-matrix",
                )));
            };
            let lc = crate::lower_ctx::vector_size_usize(columns);
            let lr = crate::lower_ctx::vector_size_usize(rows);
            if lc != lr {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "inverse non-square",
                )));
            }
            let v = ctx.ensure_expr_vec(arg)?;
            lower_matrix::lower_inverse(ctx, &v, lc)
        }
        MathFunction::Outer => {
            let a = ctx.ensure_expr_vec(arg)?;
            let b = ctx.ensure_expr_vec(
                arg1.ok_or_else(|| LowerError::Internal(String::from("outer")))?,
            )?;
            let mut out = VRegVec::new();
            for c in 0..b.len() {
                for r in 0..a.len() {
                    let d = ctx.fb.alloc_vreg(IrType::F32);
                    ctx.fb.push(Op::Fmul {
                        dst: d,
                        lhs: a[r],
                        rhs: b[c],
                    });
                    out.push(d);
                }
            }
            Ok(out)
        }
        _ => {
            // GLSL smears scalars to match `genType` operands (`mix(float, vec3, float)`, etc.).
            // Width must reflect every argument, not only the first. Also, `expr_type_inner` for
            // `float * vec3` follows the left operand only; recurse through arithmetic trees so
            // `pow(vec3*scalar, y)` does not take the scalar-only path.
            let mut w = math_dispatch_width_expr(ctx.module, ctx.func, arg)?;
            if let Some(a) = arg1 {
                w = w.max(math_dispatch_width_expr(ctx.module, ctx.func, a)?);
            }
            if let Some(a) = arg2 {
                w = w.max(math_dispatch_width_expr(ctx.module, ctx.func, a)?);
            }
            if let Some(a) = arg3 {
                w = w.max(math_dispatch_width_expr(ctx.module, ctx.func, a)?);
            }
            if w == 1 {
                return Ok(smallvec::smallvec![lower_math_scalar_impl(
                    ctx, fun, arg, arg1, arg2, arg3
                )?]);
            }
            lower_math_per_component(ctx, fun, arg, arg1, arg2, arg3)
        }
    }
}

#[allow(dead_code, reason = "scalar wrapper over lower_math_vec")]
pub(crate) fn lower_math(
    ctx: &mut LowerCtx<'_>,
    fun: MathFunction,
    arg: Handle<naga::Expression>,
    arg1: Option<Handle<naga::Expression>>,
    arg2: Option<Handle<naga::Expression>>,
    arg3: Option<Handle<naga::Expression>>,
) -> Result<VReg, LowerError> {
    let vs = lower_math_vec(ctx, fun, arg, arg1, arg2, arg3)?;
    if vs.len() != 1 {
        return Err(LowerError::Internal(format!(
            "expected scalar math result, got {} components",
            vs.len()
        )));
    }
    Ok(vs[0])
}

fn lower_math_scalar_impl(
    ctx: &mut LowerCtx<'_>,
    fun: MathFunction,
    arg: Handle<naga::Expression>,
    arg1: Option<Handle<naga::Expression>>,
    arg2: Option<Handle<naga::Expression>>,
    _arg3: Option<Handle<naga::Expression>>,
) -> Result<VReg, LowerError> {
    let k0 = expr_scalar_kind(ctx.module, ctx.func, arg)?;
    match fun {
        MathFunction::Abs => match k0 {
            ScalarKind::Float => {
                let s = ctx.ensure_expr(arg)?;
                let d = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(Op::Fabs { dst: d, src: s });
                Ok(d)
            }
            ScalarKind::Sint => {
                let s = ctx.ensure_expr(arg)?;
                let z = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(Op::IconstI32 { dst: z, value: 0 });
                let neg = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(Op::Ineg { dst: neg, src: s });
                let lt = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(Op::IltS {
                    dst: lt,
                    lhs: s,
                    rhs: z,
                });
                let d = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(Op::Select {
                    dst: d,
                    cond: lt,
                    if_true: neg,
                    if_false: s,
                });
                Ok(d)
            }
            ScalarKind::Uint => ctx.ensure_expr(arg),
            _ => Err(LowerError::UnsupportedExpression(String::from(
                "abs on non-numeric scalar",
            ))),
        },
        MathFunction::Sqrt => {
            let s = ctx.ensure_expr(arg)?;
            push_std_math(ctx, "sqrt", &[s])
        }
        MathFunction::Floor => {
            let s = ctx.ensure_expr(arg)?;
            let d = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(Op::Ffloor { dst: d, src: s });
            Ok(d)
        }
        MathFunction::Ceil => {
            let s = ctx.ensure_expr(arg)?;
            let d = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(Op::Fceil { dst: d, src: s });
            Ok(d)
        }
        MathFunction::Round => {
            let s = ctx.ensure_expr(arg)?;
            push_std_math(ctx, "round", &[s])
        }
        MathFunction::Trunc => {
            let s = ctx.ensure_expr(arg)?;
            let d = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(Op::Ftrunc { dst: d, src: s });
            Ok(d)
        }
        MathFunction::Min => lower_min_max(ctx, arg, arg1, k0, true),
        MathFunction::Max => lower_min_max(ctx, arg, arg1, k0, false),
        MathFunction::Mix => lower_mix(ctx, arg, arg1, arg2, k0),
        MathFunction::SmoothStep => lower_smoothstep(ctx, arg, arg1, arg2),
        MathFunction::Step => lower_step(ctx, arg, arg1),
        MathFunction::Fma => lower_fma(ctx, arg, arg1, arg2),
        MathFunction::Clamp => lower_clamp(ctx, arg, arg1, arg2, k0),
        MathFunction::Sign => lower_sign(ctx, arg, k0),
        MathFunction::Fract => lower_fract(ctx, arg),
        MathFunction::InverseSqrt => lower_inverse_sqrt(ctx, arg),
        MathFunction::Saturate => lower_saturate(ctx, arg),
        MathFunction::Radians => lower_radians(ctx, arg),
        MathFunction::Degrees => lower_degrees(ctx, arg),

        MathFunction::Sin => std_math_unary(ctx, "sin", arg),
        MathFunction::Cos => std_math_unary(ctx, "cos", arg),
        MathFunction::Tan => std_math_unary(ctx, "tan", arg),
        MathFunction::Asin => std_math_unary(ctx, "asin", arg),
        MathFunction::Acos => std_math_unary(ctx, "acos", arg),
        MathFunction::Atan => std_math_unary(ctx, "atan", arg),
        MathFunction::Atan2 => std_math_binary(ctx, "atan2", arg, arg1),
        MathFunction::Sinh => std_math_unary(ctx, "sinh", arg),
        MathFunction::Cosh => std_math_unary(ctx, "cosh", arg),
        MathFunction::Tanh => std_math_unary(ctx, "tanh", arg),
        MathFunction::Asinh => std_math_unary(ctx, "asinh", arg),
        MathFunction::Acosh => std_math_unary(ctx, "acosh", arg),
        MathFunction::Atanh => std_math_unary(ctx, "atanh", arg),
        MathFunction::Exp => std_math_unary(ctx, "exp", arg),
        MathFunction::Exp2 => std_math_unary(ctx, "exp2", arg),
        MathFunction::Log => std_math_unary(ctx, "log", arg),
        MathFunction::Log2 => std_math_unary(ctx, "log2", arg),
        MathFunction::Pow => std_math_binary(ctx, "pow", arg, arg1),
        MathFunction::Ldexp => lower_ldexp_import(ctx, arg, arg1),

        _ => Err(LowerError::UnsupportedExpression(format!(
            "Math::{fun:?} (scalar stage)"
        ))),
    }
}

fn lower_min_max(
    ctx: &mut LowerCtx<'_>,
    a: Handle<naga::Expression>,
    b: Option<Handle<naga::Expression>>,
    k: ScalarKind,
    is_min: bool,
) -> Result<VReg, LowerError> {
    let b = b.ok_or_else(|| LowerError::Internal(String::from("min/max missing arg")))?;
    let lk = expr_scalar_kind(ctx.module, ctx.func, a)?;
    let rk = expr_scalar_kind(ctx.module, ctx.func, b)?;
    if lk != rk || lk != k {
        return Err(LowerError::UnsupportedExpression(String::from(
            "min/max operand mismatch",
        )));
    }
    let lhs = ctx.ensure_expr(a)?;
    let rhs = ctx.ensure_expr(b)?;
    match k {
        ScalarKind::Float => {
            let d = ctx.fb.alloc_vreg(IrType::F32);
            if is_min {
                ctx.fb.push(Op::Fmin { dst: d, lhs, rhs });
            } else {
                ctx.fb.push(Op::Fmax { dst: d, lhs, rhs });
            }
            Ok(d)
        }
        ScalarKind::Sint => {
            let cmp = ctx.fb.alloc_vreg(IrType::I32);
            if is_min {
                ctx.fb.push(Op::IltS { dst: cmp, lhs, rhs });
            } else {
                ctx.fb.push(Op::IgtS { dst: cmp, lhs, rhs });
            }
            let d = ctx.fb.alloc_vreg(IrType::I32);
            // `IltS` / `IgtS`: non-zero cond → take `lhs`, else `rhs` (min vs max only changes the cmp).
            ctx.fb.push(Op::Select {
                dst: d,
                cond: cmp,
                if_true: lhs,
                if_false: rhs,
            });
            Ok(d)
        }
        ScalarKind::Uint => {
            let cmp = ctx.fb.alloc_vreg(IrType::I32);
            if is_min {
                ctx.fb.push(Op::IltU { dst: cmp, lhs, rhs });
            } else {
                ctx.fb.push(Op::IgtU { dst: cmp, lhs, rhs });
            }
            let d = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(Op::Select {
                dst: d,
                cond: cmp,
                if_true: lhs,
                if_false: rhs,
            });
            Ok(d)
        }
        _ => Err(LowerError::UnsupportedExpression(String::from(
            "min/max on non-numeric",
        ))),
    }
}

fn lower_mix(
    ctx: &mut LowerCtx<'_>,
    x: Handle<naga::Expression>,
    y: Option<Handle<naga::Expression>>,
    t: Option<Handle<naga::Expression>>,
    k: ScalarKind,
) -> Result<VReg, LowerError> {
    let y = y.ok_or_else(|| LowerError::Internal(String::from("mix missing y")))?;
    let t = t.ok_or_else(|| LowerError::Internal(String::from("mix missing t")))?;
    if k != ScalarKind::Float {
        return Err(LowerError::UnsupportedExpression(String::from(
            "mix non-float",
        )));
    }
    let xv = ctx.ensure_expr(x)?;
    let yv = ctx.ensure_expr(y)?;
    let tv = ctx.ensure_expr(t)?;
    let d = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fsub {
        dst: d,
        lhs: yv,
        rhs: xv,
    });
    let m = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fmul {
        dst: m,
        lhs: d,
        rhs: tv,
    });
    let r = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fadd {
        dst: r,
        lhs: xv,
        rhs: m,
    });
    Ok(r)
}

fn lower_smoothstep(
    ctx: &mut LowerCtx<'_>,
    e0: Handle<naga::Expression>,
    e1: Option<Handle<naga::Expression>>,
    x: Option<Handle<naga::Expression>>,
) -> Result<VReg, LowerError> {
    let e1 = e1.ok_or_else(|| LowerError::Internal(String::from("smoothstep")))?;
    let x = x.ok_or_else(|| LowerError::Internal(String::from("smoothstep")))?;
    let e0v = ctx.ensure_expr(e0)?;
    let e1v = ctx.ensure_expr(e1)?;
    let xv = ctx.ensure_expr(x)?;
    let range = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fsub {
        dst: range,
        lhs: e1v,
        rhs: e0v,
    });
    let raw = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fsub {
        dst: raw,
        lhs: xv,
        rhs: e0v,
    });
    let div = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fdiv {
        dst: div,
        lhs: raw,
        rhs: range,
    });
    let z = fconst(ctx, 0.0);
    let lo = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fmax {
        dst: lo,
        lhs: div,
        rhs: z,
    });
    let one = fconst(ctx, 1.0);
    let t = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fmin {
        dst: t,
        lhs: lo,
        rhs: one,
    });
    let two = fconst(ctx, 2.0);
    let three = fconst(ctx, 3.0);
    let t2 = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fmul {
        dst: t2,
        lhs: t,
        rhs: t,
    });
    let twot = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fmul {
        dst: twot,
        lhs: two,
        rhs: t,
    });
    let diff = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fsub {
        dst: diff,
        lhs: three,
        rhs: twot,
    });
    let r = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fmul {
        dst: r,
        lhs: t2,
        rhs: diff,
    });
    Ok(r)
}

fn lower_step(
    ctx: &mut LowerCtx<'_>,
    edge: Handle<naga::Expression>,
    x: Option<Handle<naga::Expression>>,
) -> Result<VReg, LowerError> {
    let x = x.ok_or_else(|| LowerError::Internal(String::from("step")))?;
    let ev = ctx.ensure_expr(edge)?;
    let xv = ctx.ensure_expr(x)?;
    let cmp = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::Fge {
        dst: cmp,
        lhs: xv,
        rhs: ev,
    });
    let one = fconst(ctx, 1.0);
    let zero = fconst(ctx, 0.0);
    let r = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Select {
        dst: r,
        cond: cmp,
        if_true: one,
        if_false: zero,
    });
    Ok(r)
}

fn lower_fma(
    ctx: &mut LowerCtx<'_>,
    a: Handle<naga::Expression>,
    b: Option<Handle<naga::Expression>>,
    c: Option<Handle<naga::Expression>>,
) -> Result<VReg, LowerError> {
    let b = b.ok_or_else(|| LowerError::Internal(String::from("fma")))?;
    let c = c.ok_or_else(|| LowerError::Internal(String::from("fma")))?;
    let av = ctx.ensure_expr(a)?;
    let bv = ctx.ensure_expr(b)?;
    let cv = ctx.ensure_expr(c)?;
    let m = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fmul {
        dst: m,
        lhs: av,
        rhs: bv,
    });
    let r = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fadd {
        dst: r,
        lhs: m,
        rhs: cv,
    });
    Ok(r)
}

fn lower_clamp(
    ctx: &mut LowerCtx<'_>,
    x: Handle<naga::Expression>,
    lo: Option<Handle<naga::Expression>>,
    hi: Option<Handle<naga::Expression>>,
    k: ScalarKind,
) -> Result<VReg, LowerError> {
    let lo = lo.ok_or_else(|| LowerError::Internal(String::from("clamp")))?;
    let hi = hi.ok_or_else(|| LowerError::Internal(String::from("clamp")))?;
    match k {
        ScalarKind::Float => {
            let xv = ctx.ensure_expr(x)?;
            let lov = ctx.ensure_expr(lo)?;
            let hiv = ctx.ensure_expr(hi)?;
            let t = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(Op::Fmax {
                dst: t,
                lhs: xv,
                rhs: lov,
            });
            let r = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(Op::Fmin {
                dst: r,
                lhs: t,
                rhs: hiv,
            });
            Ok(r)
        }
        ScalarKind::Sint => {
            let xv = ctx.ensure_expr(x)?;
            let lov = ctx.ensure_expr(lo)?;
            let hiv = ctx.ensure_expr(hi)?;
            let lt = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(Op::IltS {
                dst: lt,
                lhs: xv,
                rhs: lov,
            });
            let t = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(Op::Select {
                dst: t,
                cond: lt,
                if_true: lov,
                if_false: xv,
            });
            let gt = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(Op::IgtS {
                dst: gt,
                lhs: t,
                rhs: hiv,
            });
            let r = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(Op::Select {
                dst: r,
                cond: gt,
                if_true: hiv,
                if_false: t,
            });
            Ok(r)
        }
        ScalarKind::Uint => {
            let xv = ctx.ensure_expr(x)?;
            let lov = ctx.ensure_expr(lo)?;
            let hiv = ctx.ensure_expr(hi)?;
            let lt = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(Op::IltU {
                dst: lt,
                lhs: xv,
                rhs: lov,
            });
            let t = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(Op::Select {
                dst: t,
                cond: lt,
                if_true: lov,
                if_false: xv,
            });
            let gt = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(Op::IgtU {
                dst: gt,
                lhs: t,
                rhs: hiv,
            });
            let r = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(Op::Select {
                dst: r,
                cond: gt,
                if_true: hiv,
                if_false: t,
            });
            Ok(r)
        }
        _ => Err(LowerError::UnsupportedExpression(String::from(
            "clamp non-numeric",
        ))),
    }
}

fn lower_sign(
    ctx: &mut LowerCtx<'_>,
    arg: Handle<naga::Expression>,
    k: ScalarKind,
) -> Result<VReg, LowerError> {
    match k {
        ScalarKind::Float => {
            let x = ctx.ensure_expr(arg)?;
            let zero = fconst(ctx, 0.0);
            let one = fconst(ctx, 1.0);
            let neg1 = fconst(ctx, -1.0);
            let gt = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(Op::Fgt {
                dst: gt,
                lhs: x,
                rhs: zero,
            });
            let lt = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(Op::Flt {
                dst: lt,
                lhs: x,
                rhs: zero,
            });
            let z = fconst(ctx, 0.0);
            let r1 = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(Op::Select {
                dst: r1,
                cond: gt,
                if_true: one,
                if_false: z,
            });
            let r = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(Op::Select {
                dst: r,
                cond: lt,
                if_true: neg1,
                if_false: r1,
            });
            Ok(r)
        }
        ScalarKind::Sint => {
            let x = ctx.ensure_expr(arg)?;
            let z = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(Op::IconstI32 { dst: z, value: 0 });
            let gt = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(Op::IgtS {
                dst: gt,
                lhs: x,
                rhs: z,
            });
            let lt = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(Op::IltS {
                dst: lt,
                lhs: x,
                rhs: z,
            });
            let one = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(Op::IconstI32 { dst: one, value: 1 });
            let neg1 = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(Op::IconstI32 {
                dst: neg1,
                value: -1,
            });
            let r1 = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(Op::Select {
                dst: r1,
                cond: gt,
                if_true: one,
                if_false: z,
            });
            let r = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(Op::Select {
                dst: r,
                cond: lt,
                if_true: neg1,
                if_false: r1,
            });
            Ok(r)
        }
        ScalarKind::Uint => {
            let x = ctx.ensure_expr(arg)?;
            let z = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(Op::IconstI32 { dst: z, value: 0 });
            let gt = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(Op::IgtU {
                dst: gt,
                lhs: x,
                rhs: z,
            });
            let one = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(Op::IconstI32 { dst: one, value: 1 });
            let r = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(Op::Select {
                dst: r,
                cond: gt,
                if_true: one,
                if_false: z,
            });
            Ok(r)
        }
        _ => Err(LowerError::UnsupportedExpression(String::from(
            "sign non-numeric",
        ))),
    }
}

fn lower_fract(ctx: &mut LowerCtx<'_>, arg: Handle<naga::Expression>) -> Result<VReg, LowerError> {
    let x = ctx.ensure_expr(arg)?;
    let fl = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Ffloor { dst: fl, src: x });
    let r = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fsub {
        dst: r,
        lhs: x,
        rhs: fl,
    });
    Ok(r)
}

fn lower_inverse_sqrt(
    ctx: &mut LowerCtx<'_>,
    arg: Handle<naga::Expression>,
) -> Result<VReg, LowerError> {
    let x = ctx.ensure_expr(arg)?;
    let sq = push_std_math(ctx, "sqrt", &[x])?;
    let one = fconst(ctx, 1.0);
    let r = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fdiv {
        dst: r,
        lhs: one,
        rhs: sq,
    });
    Ok(r)
}

fn lower_saturate(
    ctx: &mut LowerCtx<'_>,
    arg: Handle<naga::Expression>,
) -> Result<VReg, LowerError> {
    let x = ctx.ensure_expr(arg)?;
    let z = fconst(ctx, 0.0);
    let one = fconst(ctx, 1.0);
    let t = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fmax {
        dst: t,
        lhs: x,
        rhs: z,
    });
    let r = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fmin {
        dst: r,
        lhs: t,
        rhs: one,
    });
    Ok(r)
}

fn lower_radians(
    ctx: &mut LowerCtx<'_>,
    arg: Handle<naga::Expression>,
) -> Result<VReg, LowerError> {
    let x = ctx.ensure_expr(arg)?;
    let factor = fconst(ctx, core::f32::consts::PI / 180.0);
    let r = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fmul {
        dst: r,
        lhs: x,
        rhs: factor,
    });
    Ok(r)
}

fn lower_degrees(
    ctx: &mut LowerCtx<'_>,
    arg: Handle<naga::Expression>,
) -> Result<VReg, LowerError> {
    let x = ctx.ensure_expr(arg)?;
    let factor = fconst(ctx, 180.0 / core::f32::consts::PI);
    let r = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fmul {
        dst: r,
        lhs: x,
        rhs: factor,
    });
    Ok(r)
}

fn std_math_unary(
    ctx: &mut LowerCtx<'_>,
    name: &'static str,
    arg: Handle<naga::Expression>,
) -> Result<VReg, LowerError> {
    let s = ctx.ensure_expr(arg)?;
    push_std_math(ctx, name, &[s])
}

fn std_math_binary(
    ctx: &mut LowerCtx<'_>,
    name: &'static str,
    a: Handle<naga::Expression>,
    b: Option<Handle<naga::Expression>>,
) -> Result<VReg, LowerError> {
    let b = b.ok_or_else(|| LowerError::Internal(format!("{name} missing arg")))?;
    let av = ctx.ensure_expr(a)?;
    let bv = ctx.ensure_expr(b)?;
    push_std_math(ctx, name, &[av, bv])
}

fn lower_ldexp_import(
    ctx: &mut LowerCtx<'_>,
    x: Handle<naga::Expression>,
    e: Option<Handle<naga::Expression>>,
) -> Result<VReg, LowerError> {
    let e = e.ok_or_else(|| LowerError::Internal(String::from("ldexp")))?;
    let xv = ctx.ensure_expr(x)?;
    let ev = ctx.ensure_expr(e)?;
    push_std_math(ctx, "ldexp", &[xv, ev])
}

fn push_std_math(
    ctx: &mut LowerCtx<'_>,
    name: &'static str,
    args: &[VReg],
) -> Result<VReg, LowerError> {
    let key = format!("std.math::{name}");
    let callee = ctx
        .import_map
        .get(&key)
        .copied()
        .ok_or_else(|| LowerError::Internal(format!("missing import {key}")))?;
    let r = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push_call(callee, args, &[r]);
    Ok(r)
}

fn emit_fsub_fmul_pair(
    ctx: &mut LowerCtx<'_>,
    a1: VReg,
    b1: VReg,
    a2: VReg,
    b2: VReg,
) -> Result<VReg, LowerError> {
    let p1 = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fmul {
        dst: p1,
        lhs: a1,
        rhs: b1,
    });
    let p2 = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fmul {
        dst: p2,
        lhs: a2,
        rhs: b2,
    });
    let d = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fsub {
        dst: d,
        lhs: p1,
        rhs: p2,
    });
    Ok(d)
}

fn emit_cross(ctx: &mut LowerCtx<'_>, a: &[VReg], b: &[VReg]) -> Result<VRegVec, LowerError> {
    let x = emit_fsub_fmul_pair(ctx, a[1], b[2], a[2], b[1])?;
    let y = emit_fsub_fmul_pair(ctx, a[2], b[0], a[0], b[2])?;
    let z = emit_fsub_fmul_pair(ctx, a[0], b[1], a[1], b[0])?;
    Ok(smallvec::smallvec![x, y, z])
}

fn vat(v: &[VReg], i: usize) -> VReg {
    v[i.min(v.len().saturating_sub(1))]
}

fn lower_math_per_component(
    ctx: &mut LowerCtx<'_>,
    fun: MathFunction,
    arg: Handle<naga::Expression>,
    arg1: Option<Handle<naga::Expression>>,
    arg2: Option<Handle<naga::Expression>>,
    arg3: Option<Handle<naga::Expression>>,
) -> Result<VRegVec, LowerError> {
    let _ = arg3;
    let k0 = expr_scalar_kind(ctx.module, ctx.func, arg)?;
    match fun {
        MathFunction::Abs => match k0 {
            ScalarKind::Float => {
                let vs = ctx.ensure_expr_vec(arg)?;
                let mut o = VRegVec::new();
                for s in vs {
                    let d = ctx.fb.alloc_vreg(IrType::F32);
                    ctx.fb.push(Op::Fabs { dst: d, src: s });
                    o.push(d);
                }
                Ok(o)
            }
            ScalarKind::Sint => {
                let vs = ctx.ensure_expr_vec(arg)?;
                let mut o = VRegVec::new();
                for s in vs {
                    let z = ctx.fb.alloc_vreg(IrType::I32);
                    ctx.fb.push(Op::IconstI32 { dst: z, value: 0 });
                    let neg = ctx.fb.alloc_vreg(IrType::I32);
                    ctx.fb.push(Op::Ineg { dst: neg, src: s });
                    let lt = ctx.fb.alloc_vreg(IrType::I32);
                    ctx.fb.push(Op::IltS {
                        dst: lt,
                        lhs: s,
                        rhs: z,
                    });
                    let d = ctx.fb.alloc_vreg(IrType::I32);
                    ctx.fb.push(Op::Select {
                        dst: d,
                        cond: lt,
                        if_true: neg,
                        if_false: s,
                    });
                    o.push(d);
                }
                Ok(o)
            }
            ScalarKind::Uint => ctx.ensure_expr_vec(arg),
            _ => Err(LowerError::UnsupportedExpression(String::from(
                "abs on non-numeric vector",
            ))),
        },
        MathFunction::Sqrt => std_math_unary_vec(ctx, "sqrt", arg),
        MathFunction::Floor => unary_float_op_vec(ctx, arg, |fb, d, s| {
            fb.push(Op::Ffloor { dst: d, src: s });
        }),
        MathFunction::Ceil => unary_float_op_vec(ctx, arg, |fb, d, s| {
            fb.push(Op::Fceil { dst: d, src: s });
        }),
        MathFunction::Round => std_math_unary_vec(ctx, "round", arg),
        MathFunction::Trunc => unary_float_op_vec(ctx, arg, |fb, d, s| {
            fb.push(Op::Ftrunc { dst: d, src: s });
        }),
        MathFunction::Min | MathFunction::Max => {
            let is_min = matches!(fun, MathFunction::Min);
            let b = arg1.ok_or_else(|| LowerError::Internal(String::from("min/max")))?;
            let a = ctx.ensure_expr_vec(arg)?;
            let b = ctx.ensure_expr_vec(b)?;
            let n = a.len().max(b.len());
            match k0 {
                ScalarKind::Float => {
                    let mut o = VRegVec::new();
                    for i in 0..n {
                        let lhs = vat(&a, i);
                        let rhs = vat(&b, i);
                        let d = ctx.fb.alloc_vreg(IrType::F32);
                        if is_min {
                            ctx.fb.push(Op::Fmin { dst: d, lhs, rhs });
                        } else {
                            ctx.fb.push(Op::Fmax { dst: d, lhs, rhs });
                        }
                        o.push(d);
                    }
                    Ok(o)
                }
                ScalarKind::Sint => minmax_int_vec(ctx, &a, &b, n, is_min, true),
                ScalarKind::Uint => minmax_int_vec(ctx, &a, &b, n, is_min, false),
                _ => Err(LowerError::UnsupportedExpression(String::from(
                    "min/max vector non-numeric",
                ))),
            }
        }
        MathFunction::Mix => {
            if k0 != ScalarKind::Float {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "mix non-float vector",
                )));
            }
            let y = arg1.ok_or_else(|| LowerError::Internal(String::from("mix")))?;
            let t = arg2.ok_or_else(|| LowerError::Internal(String::from("mix")))?;
            let x = ctx.ensure_expr_vec(arg)?;
            let yv = ctx.ensure_expr_vec(y)?;
            let tv = ctx.ensure_expr_vec(t)?;
            let n = x.len().max(yv.len()).max(tv.len());
            let mut o = VRegVec::new();
            for i in 0..n {
                let xv = vat(&x, i);
                let y0 = vat(&yv, i);
                let t0 = vat(&tv, i);
                let d = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(Op::Fsub {
                    dst: d,
                    lhs: y0,
                    rhs: xv,
                });
                let m = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(Op::Fmul {
                    dst: m,
                    lhs: d,
                    rhs: t0,
                });
                let r = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(Op::Fadd {
                    dst: r,
                    lhs: xv,
                    rhs: m,
                });
                o.push(r);
            }
            Ok(o)
        }
        MathFunction::SmoothStep => smoothstep_vec(ctx, arg, arg1, arg2),
        MathFunction::Step => step_vec(ctx, arg, arg1),
        MathFunction::Fma => fma_vec(ctx, arg, arg1, arg2),
        MathFunction::Clamp => clamp_vec(ctx, arg, arg1, arg2, k0),
        MathFunction::Sign => sign_vec(ctx, arg, k0),
        MathFunction::Fract => unary_float_op_vec(ctx, arg, |fb, d, s| {
            let fl = fb.alloc_vreg(IrType::F32);
            fb.push(Op::Ffloor { dst: fl, src: s });
            fb.push(Op::Fsub {
                dst: d,
                lhs: s,
                rhs: fl,
            });
        }),
        MathFunction::InverseSqrt => inverse_sqrt_vec(ctx, arg),
        MathFunction::Saturate => saturate_vec(ctx, arg),
        MathFunction::Radians => scale_vec_f32(ctx, arg, core::f32::consts::PI / 180.0),
        MathFunction::Degrees => scale_vec_f32(ctx, arg, 180.0 / core::f32::consts::PI),
        MathFunction::Sin => std_math_unary_vec(ctx, "sin", arg),
        MathFunction::Cos => std_math_unary_vec(ctx, "cos", arg),
        MathFunction::Tan => std_math_unary_vec(ctx, "tan", arg),
        MathFunction::Asin => std_math_unary_vec(ctx, "asin", arg),
        MathFunction::Acos => std_math_unary_vec(ctx, "acos", arg),
        MathFunction::Atan => std_math_unary_vec(ctx, "atan", arg),
        MathFunction::Atan2 => std_math_binary_vec(ctx, "atan2", arg, arg1),
        MathFunction::Sinh => std_math_unary_vec(ctx, "sinh", arg),
        MathFunction::Cosh => std_math_unary_vec(ctx, "cosh", arg),
        MathFunction::Tanh => std_math_unary_vec(ctx, "tanh", arg),
        MathFunction::Asinh => std_math_unary_vec(ctx, "asinh", arg),
        MathFunction::Acosh => std_math_unary_vec(ctx, "acosh", arg),
        MathFunction::Atanh => std_math_unary_vec(ctx, "atanh", arg),
        MathFunction::Exp => std_math_unary_vec(ctx, "exp", arg),
        MathFunction::Exp2 => std_math_unary_vec(ctx, "exp2", arg),
        MathFunction::Log => std_math_unary_vec(ctx, "log", arg),
        MathFunction::Log2 => std_math_unary_vec(ctx, "log2", arg),
        MathFunction::Pow => std_math_binary_vec(ctx, "pow", arg, arg1),
        MathFunction::Ldexp => ldexp_vec(ctx, arg, arg1),
        _ => Err(LowerError::UnsupportedExpression(format!(
            "Math::{fun:?} (vector)"
        ))),
    }
}

fn unary_float_op_vec(
    ctx: &mut LowerCtx<'_>,
    arg: Handle<naga::Expression>,
    mut emit: impl FnMut(&mut lpir::FunctionBuilder, VReg, VReg),
) -> Result<VRegVec, LowerError> {
    let vs = ctx.ensure_expr_vec(arg)?;
    let mut o = VRegVec::new();
    for s in vs {
        let d = ctx.fb.alloc_vreg(IrType::F32);
        emit(&mut ctx.fb, d, s);
        o.push(d);
    }
    Ok(o)
}

fn std_math_unary_vec(
    ctx: &mut LowerCtx<'_>,
    name: &'static str,
    arg: Handle<naga::Expression>,
) -> Result<VRegVec, LowerError> {
    let vs = ctx.ensure_expr_vec(arg)?;
    let mut o = VRegVec::new();
    for s in vs {
        o.push(push_std_math(ctx, name, &[s])?);
    }
    Ok(o)
}

fn std_math_binary_vec(
    ctx: &mut LowerCtx<'_>,
    name: &'static str,
    a: Handle<naga::Expression>,
    b: Option<Handle<naga::Expression>>,
) -> Result<VRegVec, LowerError> {
    let b = b.ok_or_else(|| LowerError::Internal(format!("{name} missing arg")))?;
    let av = ctx.ensure_expr_vec(a)?;
    let bv = ctx.ensure_expr_vec(b)?;
    let n = av.len().max(bv.len());
    let mut o = VRegVec::new();
    for i in 0..n {
        o.push(push_std_math(ctx, name, &[vat(&av, i), vat(&bv, i)])?);
    }
    Ok(o)
}

fn minmax_int_vec(
    ctx: &mut LowerCtx<'_>,
    a: &[VReg],
    b: &[VReg],
    n: usize,
    is_min: bool,
    signed: bool,
) -> Result<VRegVec, LowerError> {
    let mut o = VRegVec::new();
    for i in 0..n {
        let lhs = vat(a, i);
        let rhs = vat(b, i);
        let cmp = ctx.fb.alloc_vreg(IrType::I32);
        if signed {
            if is_min {
                ctx.fb.push(Op::IltS { dst: cmp, lhs, rhs });
            } else {
                ctx.fb.push(Op::IgtS { dst: cmp, lhs, rhs });
            }
        } else if is_min {
            ctx.fb.push(Op::IltU { dst: cmp, lhs, rhs });
        } else {
            ctx.fb.push(Op::IgtU { dst: cmp, lhs, rhs });
        }
        let d = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(Op::Select {
            dst: d,
            cond: cmp,
            if_true: lhs,
            if_false: rhs,
        });
        o.push(d);
    }
    Ok(o)
}

fn smoothstep_vec(
    ctx: &mut LowerCtx<'_>,
    e0: Handle<naga::Expression>,
    e1: Option<Handle<naga::Expression>>,
    x: Option<Handle<naga::Expression>>,
) -> Result<VRegVec, LowerError> {
    let e1 = e1.ok_or_else(|| LowerError::Internal(String::from("smoothstep")))?;
    let x = x.ok_or_else(|| LowerError::Internal(String::from("smoothstep")))?;
    let e0v = ctx.ensure_expr_vec(e0)?;
    let e1v = ctx.ensure_expr_vec(e1)?;
    let xv = ctx.ensure_expr_vec(x)?;
    let n = e0v.len().max(e1v.len()).max(xv.len());
    let mut o = VRegVec::new();
    for i in 0..n {
        let r = lower_smoothstep_vregs(ctx, vat(&e0v, i), vat(&e1v, i), vat(&xv, i))?;
        o.push(r);
    }
    Ok(o)
}

fn lower_smoothstep_vregs(
    ctx: &mut LowerCtx<'_>,
    e0v: VReg,
    e1v: VReg,
    xv: VReg,
) -> Result<VReg, LowerError> {
    let range = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fsub {
        dst: range,
        lhs: e1v,
        rhs: e0v,
    });
    let raw = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fsub {
        dst: raw,
        lhs: xv,
        rhs: e0v,
    });
    let div = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fdiv {
        dst: div,
        lhs: raw,
        rhs: range,
    });
    let z = fconst(ctx, 0.0);
    let lo = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fmax {
        dst: lo,
        lhs: div,
        rhs: z,
    });
    let one = fconst(ctx, 1.0);
    let t = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fmin {
        dst: t,
        lhs: lo,
        rhs: one,
    });
    let two = fconst(ctx, 2.0);
    let three = fconst(ctx, 3.0);
    let t2 = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fmul {
        dst: t2,
        lhs: t,
        rhs: t,
    });
    let twot = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fmul {
        dst: twot,
        lhs: two,
        rhs: t,
    });
    let diff = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fsub {
        dst: diff,
        lhs: three,
        rhs: twot,
    });
    let r = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fmul {
        dst: r,
        lhs: t2,
        rhs: diff,
    });
    Ok(r)
}

fn step_vec(
    ctx: &mut LowerCtx<'_>,
    edge: Handle<naga::Expression>,
    x: Option<Handle<naga::Expression>>,
) -> Result<VRegVec, LowerError> {
    let x = x.ok_or_else(|| LowerError::Internal(String::from("step")))?;
    let ev = ctx.ensure_expr_vec(edge)?;
    let xv = ctx.ensure_expr_vec(x)?;
    let n = ev.len().max(xv.len());
    let mut o = VRegVec::new();
    for i in 0..n {
        let cmp = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(Op::Fge {
            dst: cmp,
            lhs: vat(&xv, i),
            rhs: vat(&ev, i),
        });
        let one = fconst(ctx, 1.0);
        let zero = fconst(ctx, 0.0);
        let r = ctx.fb.alloc_vreg(IrType::F32);
        ctx.fb.push(Op::Select {
            dst: r,
            cond: cmp,
            if_true: one,
            if_false: zero,
        });
        o.push(r);
    }
    Ok(o)
}

fn fma_vec(
    ctx: &mut LowerCtx<'_>,
    a: Handle<naga::Expression>,
    b: Option<Handle<naga::Expression>>,
    c: Option<Handle<naga::Expression>>,
) -> Result<VRegVec, LowerError> {
    let b = b.ok_or_else(|| LowerError::Internal(String::from("fma")))?;
    let c = c.ok_or_else(|| LowerError::Internal(String::from("fma")))?;
    let av = ctx.ensure_expr_vec(a)?;
    let bv = ctx.ensure_expr_vec(b)?;
    let cv = ctx.ensure_expr_vec(c)?;
    let n = av.len().max(bv.len()).max(cv.len());
    let mut o = VRegVec::new();
    for i in 0..n {
        let m = ctx.fb.alloc_vreg(IrType::F32);
        ctx.fb.push(Op::Fmul {
            dst: m,
            lhs: vat(&av, i),
            rhs: vat(&bv, i),
        });
        let r = ctx.fb.alloc_vreg(IrType::F32);
        ctx.fb.push(Op::Fadd {
            dst: r,
            lhs: m,
            rhs: vat(&cv, i),
        });
        o.push(r);
    }
    Ok(o)
}

fn clamp_vec(
    ctx: &mut LowerCtx<'_>,
    x: Handle<naga::Expression>,
    lo: Option<Handle<naga::Expression>>,
    hi: Option<Handle<naga::Expression>>,
    k: ScalarKind,
) -> Result<VRegVec, LowerError> {
    let lo = lo.ok_or_else(|| LowerError::Internal(String::from("clamp")))?;
    let hi = hi.ok_or_else(|| LowerError::Internal(String::from("clamp")))?;
    let xv = ctx.ensure_expr_vec(x)?;
    let lov = ctx.ensure_expr_vec(lo)?;
    let hiv = ctx.ensure_expr_vec(hi)?;
    let n = xv.len().max(lov.len()).max(hiv.len());
    match k {
        ScalarKind::Float => {
            let mut o = VRegVec::new();
            for i in 0..n {
                let t = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(Op::Fmax {
                    dst: t,
                    lhs: vat(&xv, i),
                    rhs: vat(&lov, i),
                });
                let r = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(Op::Fmin {
                    dst: r,
                    lhs: t,
                    rhs: vat(&hiv, i),
                });
                o.push(r);
            }
            Ok(o)
        }
        ScalarKind::Sint => clamp_int_vec(ctx, &xv, &lov, &hiv, n, true),
        ScalarKind::Uint => clamp_int_vec(ctx, &xv, &lov, &hiv, n, false),
        _ => Err(LowerError::UnsupportedExpression(String::from(
            "clamp vector non-numeric",
        ))),
    }
}

fn clamp_int_vec(
    ctx: &mut LowerCtx<'_>,
    xv: &[VReg],
    lov: &[VReg],
    hiv: &[VReg],
    n: usize,
    signed: bool,
) -> Result<VRegVec, LowerError> {
    let mut o = VRegVec::new();
    for i in 0..n {
        let x0 = vat(xv, i);
        let lo0 = vat(lov, i);
        let hi0 = vat(hiv, i);
        let lt = ctx.fb.alloc_vreg(IrType::I32);
        if signed {
            ctx.fb.push(Op::IltS {
                dst: lt,
                lhs: x0,
                rhs: lo0,
            });
        } else {
            ctx.fb.push(Op::IltU {
                dst: lt,
                lhs: x0,
                rhs: lo0,
            });
        }
        let t = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(Op::Select {
            dst: t,
            cond: lt,
            if_true: lo0,
            if_false: x0,
        });
        let gt = ctx.fb.alloc_vreg(IrType::I32);
        if signed {
            ctx.fb.push(Op::IgtS {
                dst: gt,
                lhs: t,
                rhs: hi0,
            });
        } else {
            ctx.fb.push(Op::IgtU {
                dst: gt,
                lhs: t,
                rhs: hi0,
            });
        }
        let r = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(Op::Select {
            dst: r,
            cond: gt,
            if_true: hi0,
            if_false: t,
        });
        o.push(r);
    }
    Ok(o)
}

fn sign_vec(
    ctx: &mut LowerCtx<'_>,
    arg: Handle<naga::Expression>,
    k: ScalarKind,
) -> Result<VRegVec, LowerError> {
    let vs = ctx.ensure_expr_vec(arg)?;
    let mut o = VRegVec::new();
    match k {
        ScalarKind::Float => {
            for x in vs {
                let zero = fconst(ctx, 0.0);
                let one = fconst(ctx, 1.0);
                let neg1 = fconst(ctx, -1.0);
                let gt = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(Op::Fgt {
                    dst: gt,
                    lhs: x,
                    rhs: zero,
                });
                let lt = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(Op::Flt {
                    dst: lt,
                    lhs: x,
                    rhs: zero,
                });
                let z = fconst(ctx, 0.0);
                let r1 = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(Op::Select {
                    dst: r1,
                    cond: gt,
                    if_true: one,
                    if_false: z,
                });
                let r = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(Op::Select {
                    dst: r,
                    cond: lt,
                    if_true: neg1,
                    if_false: r1,
                });
                o.push(r);
            }
        }
        ScalarKind::Sint => {
            for x in vs {
                let z = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(Op::IconstI32 { dst: z, value: 0 });
                let gt = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(Op::IgtS {
                    dst: gt,
                    lhs: x,
                    rhs: z,
                });
                let lt = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(Op::IltS {
                    dst: lt,
                    lhs: x,
                    rhs: z,
                });
                let one = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(Op::IconstI32 { dst: one, value: 1 });
                let neg1 = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(Op::IconstI32 {
                    dst: neg1,
                    value: -1,
                });
                let r1 = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(Op::Select {
                    dst: r1,
                    cond: gt,
                    if_true: one,
                    if_false: z,
                });
                let r = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(Op::Select {
                    dst: r,
                    cond: lt,
                    if_true: neg1,
                    if_false: r1,
                });
                o.push(r);
            }
        }
        ScalarKind::Uint => {
            for x in vs {
                let z = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(Op::IconstI32 { dst: z, value: 0 });
                let gt = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(Op::IgtU {
                    dst: gt,
                    lhs: x,
                    rhs: z,
                });
                let one = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(Op::IconstI32 { dst: one, value: 1 });
                let r = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(Op::Select {
                    dst: r,
                    cond: gt,
                    if_true: one,
                    if_false: z,
                });
                o.push(r);
            }
        }
        _ => {
            return Err(LowerError::UnsupportedExpression(String::from(
                "sign vector non-numeric",
            )));
        }
    }
    Ok(o)
}

fn inverse_sqrt_vec(
    ctx: &mut LowerCtx<'_>,
    arg: Handle<naga::Expression>,
) -> Result<VRegVec, LowerError> {
    let vs = ctx.ensure_expr_vec(arg)?;
    let mut o = VRegVec::new();
    for x in vs {
        let sq = push_std_math(ctx, "sqrt", &[x])?;
        let one = fconst(ctx, 1.0);
        let r = ctx.fb.alloc_vreg(IrType::F32);
        ctx.fb.push(Op::Fdiv {
            dst: r,
            lhs: one,
            rhs: sq,
        });
        o.push(r);
    }
    Ok(o)
}

fn saturate_vec(
    ctx: &mut LowerCtx<'_>,
    arg: Handle<naga::Expression>,
) -> Result<VRegVec, LowerError> {
    let vs = ctx.ensure_expr_vec(arg)?;
    let mut o = VRegVec::new();
    for x in vs {
        let z = fconst(ctx, 0.0);
        let one = fconst(ctx, 1.0);
        let t = ctx.fb.alloc_vreg(IrType::F32);
        ctx.fb.push(Op::Fmax {
            dst: t,
            lhs: x,
            rhs: z,
        });
        let r = ctx.fb.alloc_vreg(IrType::F32);
        ctx.fb.push(Op::Fmin {
            dst: r,
            lhs: t,
            rhs: one,
        });
        o.push(r);
    }
    Ok(o)
}

fn scale_vec_f32(
    ctx: &mut LowerCtx<'_>,
    arg: Handle<naga::Expression>,
    factor: f32,
) -> Result<VRegVec, LowerError> {
    let vs = ctx.ensure_expr_vec(arg)?;
    let f = fconst(ctx, factor);
    let mut o = VRegVec::new();
    for x in vs {
        let r = ctx.fb.alloc_vreg(IrType::F32);
        ctx.fb.push(Op::Fmul {
            dst: r,
            lhs: x,
            rhs: f,
        });
        o.push(r);
    }
    Ok(o)
}

fn ldexp_vec(
    ctx: &mut LowerCtx<'_>,
    x: Handle<naga::Expression>,
    e: Option<Handle<naga::Expression>>,
) -> Result<VRegVec, LowerError> {
    let e = e.ok_or_else(|| LowerError::Internal(String::from("ldexp")))?;
    let xv = ctx.ensure_expr_vec(x)?;
    let ev = ctx.ensure_expr_vec(e)?;
    let n = xv.len().max(ev.len());
    let mut o = VRegVec::new();
    for i in 0..n {
        o.push(push_std_math(ctx, "ldexp", &[vat(&xv, i), vat(&ev, i)])?);
    }
    Ok(o)
}

fn fconst(ctx: &mut LowerCtx<'_>, value: f32) -> VReg {
    let v = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::FconstF32 { dst: v, value });
    v
}

fn binary_op_maxes_dispatch_width(op: BinaryOperator) -> bool {
    matches!(
        op,
        BinaryOperator::Add
            | BinaryOperator::Subtract
            | BinaryOperator::Multiply
            | BinaryOperator::Divide
            | BinaryOperator::Modulo
    )
}

/// Component width for scalar vs vector math dispatch (see `lower_math_vec` default arm).
fn math_dispatch_width_expr(
    module: &Module,
    func: &Function,
    expr: Handle<naga::Expression>,
) -> Result<usize, LowerError> {
    match &func.expressions[expr] {
        Expression::Binary { op, left, right } if binary_op_maxes_dispatch_width(*op) => {
            Ok(math_dispatch_width_expr(module, func, *left)?
                .max(math_dispatch_width_expr(module, func, *right)?))
        }
        _ => Ok(naga_type_width(&expr_type_inner(module, func, expr)?)),
    }
}
