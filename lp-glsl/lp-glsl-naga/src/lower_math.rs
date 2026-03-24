//! Naga [`Expression::Math`] → LPIR ops, inline decomposition, and `@std.math` import calls.

use alloc::format;
use alloc::string::String;

use lpir::{IrType, Op, VReg};
use naga::{Handle, MathFunction, ScalarKind};

use crate::expr_scalar::expr_scalar_kind;
use crate::lower_ctx::LowerCtx;
use crate::lower_error::LowerError;

pub(crate) fn lower_math(
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
            let d = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(Op::Fsqrt { dst: d, src: s });
            Ok(d)
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
            let d = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(Op::Fnearest { dst: d, src: s });
            Ok(d)
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
    let sq = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fsqrt { dst: sq, src: x });
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

fn fconst(ctx: &mut LowerCtx<'_>, value: f32) -> VReg {
    let v = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::FconstF32 { dst: v, value });
    v
}
