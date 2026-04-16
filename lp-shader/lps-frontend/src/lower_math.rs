//! Naga [`Expression::Math`] → LPIR: inline ops, `@glsl` / `@lpir` imports, per-component vector math,
//! and geometry/matrix builtins (see [`crate::lower_math_geom`]).

use alloc::format;
use alloc::string::String;

use lpir::{IrType, LpirOp, VReg};
use naga::{Handle, MathFunction, ScalarKind};

use crate::lower_ctx::{LowerCtx, VRegVec};
use crate::lower_error::LowerError;
use crate::lower_math_geom::try_lower_special;
use crate::lower_math_helpers::{fconst, math_dispatch_width_expr, push_import_call, vat};
use crate::naga_util::expr_scalar_kind;

pub(crate) fn lower_math_vec(
    ctx: &mut LowerCtx<'_>,
    fun: MathFunction,
    arg: Handle<naga::Expression>,
    arg1: Option<Handle<naga::Expression>>,
    arg2: Option<Handle<naga::Expression>>,
    arg3: Option<Handle<naga::Expression>>,
) -> Result<VRegVec, LowerError> {
    if let Some(v) = try_lower_special(ctx, fun, arg, arg1, arg2, arg3)? {
        return Ok(v);
    }

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
    lower_math_vectorized(ctx, fun, arg, arg1, arg2, arg3)
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
        MathFunction::Abs => {
            let s = ctx.ensure_expr(arg)?;
            emit_abs(ctx, k0, s)
        }
        MathFunction::Sqrt => {
            let s = ctx.ensure_expr(arg)?;
            push_import_call(ctx, "lpir", "sqrt", &[s])
        }
        MathFunction::Floor => {
            let s = ctx.ensure_expr(arg)?;
            let d = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(LpirOp::Ffloor { dst: d, src: s });
            Ok(d)
        }
        MathFunction::Ceil => {
            let s = ctx.ensure_expr(arg)?;
            let d = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(LpirOp::Fceil { dst: d, src: s });
            Ok(d)
        }
        MathFunction::Round => {
            let s = ctx.ensure_expr(arg)?;
            push_import_call(ctx, "glsl", "round", &[s])
        }
        MathFunction::Trunc => {
            let s = ctx.ensure_expr(arg)?;
            let d = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(LpirOp::Ftrunc { dst: d, src: s });
            Ok(d)
        }
        MathFunction::Min => lower_min_max_scalar(ctx, arg, arg1, k0, true),
        MathFunction::Max => lower_min_max_scalar(ctx, arg, arg1, k0, false),
        MathFunction::Mix => lower_mix_scalar(ctx, arg, arg1, arg2, k0),
        MathFunction::SmoothStep => lower_smoothstep_scalar(ctx, arg, arg1, arg2),
        MathFunction::Step => lower_step_scalar(ctx, arg, arg1),
        MathFunction::Fma => lower_fma_scalar(ctx, arg, arg1, arg2),
        MathFunction::Clamp => lower_clamp_scalar(ctx, arg, arg1, arg2, k0),
        MathFunction::Sign => {
            let x = ctx.ensure_expr(arg)?;
            emit_sign(ctx, k0, x)
        }
        MathFunction::Fract => {
            let x = ctx.ensure_expr(arg)?;
            emit_fract_f32(ctx, x)
        }
        MathFunction::InverseSqrt => {
            let x = ctx.ensure_expr(arg)?;
            emit_inverse_sqrt_f32(ctx, x)
        }
        MathFunction::Saturate => {
            let x = ctx.ensure_expr(arg)?;
            emit_saturate_f32(ctx, x)
        }
        MathFunction::Radians => {
            let x = ctx.ensure_expr(arg)?;
            emit_scale_f32(ctx, x, core::f32::consts::PI / 180.0)
        }
        MathFunction::Degrees => {
            let x = ctx.ensure_expr(arg)?;
            emit_scale_f32(ctx, x, 180.0 / core::f32::consts::PI)
        }

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

fn lower_math_vectorized(
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
        MathFunction::Abs => map_unary_vregs(ctx, k0, arg, |ctx, k, s| emit_abs(ctx, k, s)),
        MathFunction::Sqrt => std_math_unary_vec(ctx, "lpir", "sqrt", arg),
        MathFunction::Floor => unary_float_op_vec(ctx, arg, |fb, d, s| {
            fb.push(LpirOp::Ffloor { dst: d, src: s });
        }),
        MathFunction::Ceil => unary_float_op_vec(ctx, arg, |fb, d, s| {
            fb.push(LpirOp::Fceil { dst: d, src: s });
        }),
        MathFunction::Round => std_math_unary_vec(ctx, "glsl", "round", arg),
        MathFunction::Trunc => unary_float_op_vec(ctx, arg, |fb, d, s| {
            fb.push(LpirOp::Ftrunc { dst: d, src: s });
        }),
        MathFunction::Min | MathFunction::Max => {
            let is_min = matches!(fun, MathFunction::Min);
            lower_min_max_vec(ctx, arg, arg1, k0, is_min)
        }
        MathFunction::Mix => lower_mix_vec(ctx, arg, arg1, arg2, k0),
        MathFunction::SmoothStep => smoothstep_vec(ctx, arg, arg1, arg2),
        MathFunction::Step => step_vec(ctx, arg, arg1),
        MathFunction::Fma => fma_vec(ctx, arg, arg1, arg2),
        MathFunction::Clamp => clamp_vec(ctx, arg, arg1, arg2, k0),
        MathFunction::Sign => sign_vec(ctx, arg, k0),
        MathFunction::Fract => unary_float_op_vec(ctx, arg, |fb, d, s| {
            let fl = fb.alloc_vreg(IrType::F32);
            fb.push(LpirOp::Ffloor { dst: fl, src: s });
            fb.push(LpirOp::Fsub {
                dst: d,
                lhs: s,
                rhs: fl,
            });
        }),
        MathFunction::InverseSqrt => inverse_sqrt_vec(ctx, arg),
        MathFunction::Saturate => saturate_vec(ctx, arg),
        MathFunction::Radians => scale_vec_f32(ctx, arg, core::f32::consts::PI / 180.0),
        MathFunction::Degrees => scale_vec_f32(ctx, arg, 180.0 / core::f32::consts::PI),
        MathFunction::Sin => std_math_unary_vec(ctx, "glsl", "sin", arg),
        MathFunction::Cos => std_math_unary_vec(ctx, "glsl", "cos", arg),
        MathFunction::Tan => std_math_unary_vec(ctx, "glsl", "tan", arg),
        MathFunction::Asin => std_math_unary_vec(ctx, "glsl", "asin", arg),
        MathFunction::Acos => std_math_unary_vec(ctx, "glsl", "acos", arg),
        MathFunction::Atan => std_math_unary_vec(ctx, "glsl", "atan", arg),
        MathFunction::Atan2 => std_math_binary_vec(ctx, "atan2", arg, arg1),
        MathFunction::Sinh => std_math_unary_vec(ctx, "glsl", "sinh", arg),
        MathFunction::Cosh => std_math_unary_vec(ctx, "glsl", "cosh", arg),
        MathFunction::Tanh => std_math_unary_vec(ctx, "glsl", "tanh", arg),
        MathFunction::Asinh => std_math_unary_vec(ctx, "glsl", "asinh", arg),
        MathFunction::Acosh => std_math_unary_vec(ctx, "glsl", "acosh", arg),
        MathFunction::Atanh => std_math_unary_vec(ctx, "glsl", "atanh", arg),
        MathFunction::Exp => std_math_unary_vec(ctx, "glsl", "exp", arg),
        MathFunction::Exp2 => std_math_unary_vec(ctx, "glsl", "exp2", arg),
        MathFunction::Log => std_math_unary_vec(ctx, "glsl", "log", arg),
        MathFunction::Log2 => std_math_unary_vec(ctx, "glsl", "log2", arg),
        MathFunction::Pow => std_math_binary_vec(ctx, "pow", arg, arg1),
        MathFunction::Ldexp => ldexp_vec(ctx, arg, arg1),
        _ => Err(LowerError::UnsupportedExpression(format!(
            "Math::{fun:?} (vector)"
        ))),
    }
}

fn map_unary_vregs(
    ctx: &mut LowerCtx<'_>,
    k: ScalarKind,
    arg: Handle<naga::Expression>,
    mut f: impl FnMut(&mut LowerCtx<'_>, ScalarKind, VReg) -> Result<VReg, LowerError>,
) -> Result<VRegVec, LowerError> {
    let vs = ctx.ensure_expr_vec(arg)?;
    let mut o = VRegVec::new();
    for s in vs {
        o.push(f(ctx, k, s)?);
    }
    Ok(o)
}

fn emit_abs(ctx: &mut LowerCtx<'_>, k: ScalarKind, s: VReg) -> Result<VReg, LowerError> {
    match k {
        ScalarKind::Float => {
            let d = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(LpirOp::Fabs { dst: d, src: s });
            Ok(d)
        }
        ScalarKind::Sint => {
            let z = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::IconstI32 { dst: z, value: 0 });
            let neg = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::Ineg { dst: neg, src: s });
            let lt = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::IltS {
                dst: lt,
                lhs: s,
                rhs: z,
            });
            let d = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::Select {
                dst: d,
                cond: lt,
                if_true: neg,
                if_false: s,
            });
            Ok(d)
        }
        ScalarKind::Uint => Ok(s),
        _ => Err(LowerError::UnsupportedExpression(String::from(
            "abs on non-numeric scalar",
        ))),
    }
}

fn emit_min_max_k(
    ctx: &mut LowerCtx<'_>,
    k: ScalarKind,
    lhs: VReg,
    rhs: VReg,
    is_min: bool,
) -> Result<VReg, LowerError> {
    match k {
        ScalarKind::Float => {
            let d = ctx.fb.alloc_vreg(IrType::F32);
            if is_min {
                ctx.fb.push(LpirOp::Fmin { dst: d, lhs, rhs });
            } else {
                ctx.fb.push(LpirOp::Fmax { dst: d, lhs, rhs });
            }
            Ok(d)
        }
        ScalarKind::Sint => {
            let cmp = ctx.fb.alloc_vreg(IrType::I32);
            if is_min {
                ctx.fb.push(LpirOp::IltS { dst: cmp, lhs, rhs });
            } else {
                ctx.fb.push(LpirOp::IgtS { dst: cmp, lhs, rhs });
            }
            let d = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::Select {
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
                ctx.fb.push(LpirOp::IltU { dst: cmp, lhs, rhs });
            } else {
                ctx.fb.push(LpirOp::IgtU { dst: cmp, lhs, rhs });
            }
            let d = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::Select {
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

fn lower_min_max_scalar(
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
    emit_min_max_k(ctx, k, lhs, rhs, is_min)
}

fn lower_min_max_vec(
    ctx: &mut LowerCtx<'_>,
    arg: Handle<naga::Expression>,
    arg1: Option<Handle<naga::Expression>>,
    k0: ScalarKind,
    is_min: bool,
) -> Result<VRegVec, LowerError> {
    let b = arg1.ok_or_else(|| LowerError::Internal(String::from("min/max")))?;
    let lk = expr_scalar_kind(ctx.module, ctx.func, arg)?;
    let rk = expr_scalar_kind(ctx.module, ctx.func, b)?;
    if lk != rk || lk != k0 {
        return Err(LowerError::UnsupportedExpression(String::from(
            "min/max operand mismatch",
        )));
    }
    let a = ctx.ensure_expr_vec(arg)?;
    let b = ctx.ensure_expr_vec(b)?;
    let n = a.len().max(b.len());
    match k0 {
        ScalarKind::Float | ScalarKind::Sint | ScalarKind::Uint => {
            let mut o = VRegVec::new();
            for i in 0..n {
                o.push(emit_min_max_k(ctx, k0, vat(&a, i), vat(&b, i), is_min)?);
            }
            Ok(o)
        }
        _ => Err(LowerError::UnsupportedExpression(String::from(
            "min/max vector non-numeric",
        ))),
    }
}

fn emit_mix_float(ctx: &mut LowerCtx<'_>, xv: VReg, yv: VReg, tv: VReg) -> VReg {
    let d = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fsub {
        dst: d,
        lhs: yv,
        rhs: xv,
    });
    let m = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fmul {
        dst: m,
        lhs: d,
        rhs: tv,
    });
    let r = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fadd {
        dst: r,
        lhs: xv,
        rhs: m,
    });
    r
}

fn lower_mix_scalar(
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
    Ok(emit_mix_float(ctx, xv, yv, tv))
}

fn lower_mix_vec(
    ctx: &mut LowerCtx<'_>,
    arg: Handle<naga::Expression>,
    arg1: Option<Handle<naga::Expression>>,
    arg2: Option<Handle<naga::Expression>>,
    k0: ScalarKind,
) -> Result<VRegVec, LowerError> {
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
        o.push(emit_mix_float(ctx, vat(&x, i), vat(&yv, i), vat(&tv, i)));
    }
    Ok(o)
}

fn lower_smoothstep_scalar(
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
    lower_smoothstep_vregs(ctx, e0v, e1v, xv)
}

fn lower_smoothstep_vregs(
    ctx: &mut LowerCtx<'_>,
    e0v: VReg,
    e1v: VReg,
    xv: VReg,
) -> Result<VReg, LowerError> {
    let range = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fsub {
        dst: range,
        lhs: e1v,
        rhs: e0v,
    });
    let raw = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fsub {
        dst: raw,
        lhs: xv,
        rhs: e0v,
    });
    let div = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fdiv {
        dst: div,
        lhs: raw,
        rhs: range,
    });
    let z = fconst(ctx, 0.0);
    let lo = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fmax {
        dst: lo,
        lhs: div,
        rhs: z,
    });
    let one = fconst(ctx, 1.0);
    let t = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fmin {
        dst: t,
        lhs: lo,
        rhs: one,
    });
    let two = fconst(ctx, 2.0);
    let three = fconst(ctx, 3.0);
    let t2 = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fmul {
        dst: t2,
        lhs: t,
        rhs: t,
    });
    let twot = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fmul {
        dst: twot,
        lhs: two,
        rhs: t,
    });
    let diff = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fsub {
        dst: diff,
        lhs: three,
        rhs: twot,
    });
    let r = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fmul {
        dst: r,
        lhs: t2,
        rhs: diff,
    });
    Ok(r)
}

fn lower_step_scalar(
    ctx: &mut LowerCtx<'_>,
    edge: Handle<naga::Expression>,
    x: Option<Handle<naga::Expression>>,
) -> Result<VReg, LowerError> {
    let x = x.ok_or_else(|| LowerError::Internal(String::from("step")))?;
    let ev = ctx.ensure_expr(edge)?;
    let xv = ctx.ensure_expr(x)?;
    Ok(emit_step_vregs(ctx, ev, xv))
}

fn emit_step_vregs(ctx: &mut LowerCtx<'_>, ev: VReg, xv: VReg) -> VReg {
    let cmp = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Fge {
        dst: cmp,
        lhs: xv,
        rhs: ev,
    });
    let one = fconst(ctx, 1.0);
    let zero = fconst(ctx, 0.0);
    let r = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Select {
        dst: r,
        cond: cmp,
        if_true: one,
        if_false: zero,
    });
    r
}

fn lower_fma_scalar(
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
    Ok(emit_fma_vregs(ctx, av, bv, cv))
}

fn emit_fma_vregs(ctx: &mut LowerCtx<'_>, av: VReg, bv: VReg, cv: VReg) -> VReg {
    let m = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fmul {
        dst: m,
        lhs: av,
        rhs: bv,
    });
    let r = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fadd {
        dst: r,
        lhs: m,
        rhs: cv,
    });
    r
}

fn lower_clamp_scalar(
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
            Ok(emit_clamp_float(ctx, xv, lov, hiv))
        }
        ScalarKind::Sint => {
            let xv = ctx.ensure_expr(x)?;
            let lov = ctx.ensure_expr(lo)?;
            let hiv = ctx.ensure_expr(hi)?;
            Ok(emit_clamp_int(ctx, xv, lov, hiv, true))
        }
        ScalarKind::Uint => {
            let xv = ctx.ensure_expr(x)?;
            let lov = ctx.ensure_expr(lo)?;
            let hiv = ctx.ensure_expr(hi)?;
            Ok(emit_clamp_int(ctx, xv, lov, hiv, false))
        }
        _ => Err(LowerError::UnsupportedExpression(String::from(
            "clamp non-numeric",
        ))),
    }
}

fn emit_clamp_float(ctx: &mut LowerCtx<'_>, xv: VReg, lov: VReg, hiv: VReg) -> VReg {
    let t = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fmax {
        dst: t,
        lhs: xv,
        rhs: lov,
    });
    let r = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fmin {
        dst: r,
        lhs: t,
        rhs: hiv,
    });
    r
}

fn emit_clamp_int(ctx: &mut LowerCtx<'_>, xv: VReg, lov: VReg, hiv: VReg, signed: bool) -> VReg {
    let lt = ctx.fb.alloc_vreg(IrType::I32);
    if signed {
        ctx.fb.push(LpirOp::IltS {
            dst: lt,
            lhs: xv,
            rhs: lov,
        });
    } else {
        ctx.fb.push(LpirOp::IltU {
            dst: lt,
            lhs: xv,
            rhs: lov,
        });
    }
    let t = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Select {
        dst: t,
        cond: lt,
        if_true: lov,
        if_false: xv,
    });
    let gt = ctx.fb.alloc_vreg(IrType::I32);
    if signed {
        ctx.fb.push(LpirOp::IgtS {
            dst: gt,
            lhs: t,
            rhs: hiv,
        });
    } else {
        ctx.fb.push(LpirOp::IgtU {
            dst: gt,
            lhs: t,
            rhs: hiv,
        });
    }
    let r = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Select {
        dst: r,
        cond: gt,
        if_true: hiv,
        if_false: t,
    });
    r
}

fn emit_sign(ctx: &mut LowerCtx<'_>, k: ScalarKind, x: VReg) -> Result<VReg, LowerError> {
    match k {
        ScalarKind::Float => {
            let zero = fconst(ctx, 0.0);
            let one = fconst(ctx, 1.0);
            let neg1 = fconst(ctx, -1.0);
            let gt = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::Fgt {
                dst: gt,
                lhs: x,
                rhs: zero,
            });
            let lt = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::Flt {
                dst: lt,
                lhs: x,
                rhs: zero,
            });
            let z = fconst(ctx, 0.0);
            let r1 = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(LpirOp::Select {
                dst: r1,
                cond: gt,
                if_true: one,
                if_false: z,
            });
            let r = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(LpirOp::Select {
                dst: r,
                cond: lt,
                if_true: neg1,
                if_false: r1,
            });
            Ok(r)
        }
        ScalarKind::Sint => {
            let z = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::IconstI32 { dst: z, value: 0 });
            let gt = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::IgtS {
                dst: gt,
                lhs: x,
                rhs: z,
            });
            let lt = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::IltS {
                dst: lt,
                lhs: x,
                rhs: z,
            });
            let one = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::IconstI32 { dst: one, value: 1 });
            let neg1 = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::IconstI32 {
                dst: neg1,
                value: -1,
            });
            let r1 = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::Select {
                dst: r1,
                cond: gt,
                if_true: one,
                if_false: z,
            });
            let r = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::Select {
                dst: r,
                cond: lt,
                if_true: neg1,
                if_false: r1,
            });
            Ok(r)
        }
        ScalarKind::Uint => {
            let z = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::IconstI32 { dst: z, value: 0 });
            let gt = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::IgtU {
                dst: gt,
                lhs: x,
                rhs: z,
            });
            let one = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::IconstI32 { dst: one, value: 1 });
            let r = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::Select {
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

fn emit_fract_f32(ctx: &mut LowerCtx<'_>, x: VReg) -> Result<VReg, LowerError> {
    let fl = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Ffloor { dst: fl, src: x });
    let r = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fsub {
        dst: r,
        lhs: x,
        rhs: fl,
    });
    Ok(r)
}

fn emit_inverse_sqrt_f32(ctx: &mut LowerCtx<'_>, x: VReg) -> Result<VReg, LowerError> {
    let sq = push_import_call(ctx, "lpir", "sqrt", &[x])?;
    let one = fconst(ctx, 1.0);
    let r = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fdiv {
        dst: r,
        lhs: one,
        rhs: sq,
    });
    Ok(r)
}

fn emit_saturate_f32(ctx: &mut LowerCtx<'_>, x: VReg) -> Result<VReg, LowerError> {
    let z = fconst(ctx, 0.0);
    let one = fconst(ctx, 1.0);
    let t = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fmax {
        dst: t,
        lhs: x,
        rhs: z,
    });
    let r = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fmin {
        dst: r,
        lhs: t,
        rhs: one,
    });
    Ok(r)
}

fn emit_scale_f32(ctx: &mut LowerCtx<'_>, x: VReg, factor: f32) -> Result<VReg, LowerError> {
    let fac = fconst(ctx, factor);
    let r = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fmul {
        dst: r,
        lhs: x,
        rhs: fac,
    });
    Ok(r)
}

fn std_math_unary(
    ctx: &mut LowerCtx<'_>,
    name: &'static str,
    arg: Handle<naga::Expression>,
) -> Result<VReg, LowerError> {
    let s = ctx.ensure_expr(arg)?;
    push_import_call(ctx, "glsl", name, &[s])
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
    push_import_call(ctx, "glsl", name, &[av, bv])
}

fn lower_ldexp_import(
    ctx: &mut LowerCtx<'_>,
    x: Handle<naga::Expression>,
    e: Option<Handle<naga::Expression>>,
) -> Result<VReg, LowerError> {
    let e = e.ok_or_else(|| LowerError::Internal(String::from("ldexp")))?;
    let xv = ctx.ensure_expr(x)?;
    let ev = ctx.ensure_expr(e)?;
    push_import_call(ctx, "glsl", "ldexp", &[xv, ev])
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
    module: &'static str,
    name: &'static str,
    arg: Handle<naga::Expression>,
) -> Result<VRegVec, LowerError> {
    let vs = ctx.ensure_expr_vec(arg)?;
    let mut o = VRegVec::new();
    for s in vs {
        o.push(push_import_call(ctx, module, name, &[s])?);
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
        o.push(push_import_call(
            ctx,
            "glsl",
            name,
            &[vat(&av, i), vat(&bv, i)],
        )?);
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
        o.push(emit_step_vregs(ctx, vat(&ev, i), vat(&xv, i)));
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
        o.push(emit_fma_vregs(ctx, vat(&av, i), vat(&bv, i), vat(&cv, i)));
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
                o.push(emit_clamp_float(
                    ctx,
                    vat(&xv, i),
                    vat(&lov, i),
                    vat(&hiv, i),
                ));
            }
            Ok(o)
        }
        ScalarKind::Sint | ScalarKind::Uint => {
            let mut o = VRegVec::new();
            for i in 0..n {
                o.push(emit_clamp_int(
                    ctx,
                    vat(&xv, i),
                    vat(&lov, i),
                    vat(&hiv, i),
                    k == ScalarKind::Sint,
                ));
            }
            Ok(o)
        }
        _ => Err(LowerError::UnsupportedExpression(String::from(
            "clamp vector non-numeric",
        ))),
    }
}

fn sign_vec(
    ctx: &mut LowerCtx<'_>,
    arg: Handle<naga::Expression>,
    k: ScalarKind,
) -> Result<VRegVec, LowerError> {
    let vs = ctx.ensure_expr_vec(arg)?;
    let mut o = VRegVec::new();
    for x in vs {
        o.push(emit_sign(ctx, k, x)?);
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
        o.push(emit_inverse_sqrt_f32(ctx, x)?);
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
        o.push(emit_saturate_f32(ctx, x)?);
    }
    Ok(o)
}

fn scale_vec_f32(
    ctx: &mut LowerCtx<'_>,
    arg: Handle<naga::Expression>,
    factor: f32,
) -> Result<VRegVec, LowerError> {
    let vs = ctx.ensure_expr_vec(arg)?;
    let mut o = VRegVec::new();
    for x in vs {
        o.push(emit_scale_f32(ctx, x, factor)?);
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
        o.push(push_import_call(
            ctx,
            "glsl",
            "ldexp",
            &[vat(&xv, i), vat(&ev, i)],
        )?);
    }
    Ok(o)
}
