//! Vector/matrix/geometry [`MathFunction`] lowering (not per-component smear builtins).

use alloc::string::String;

use lpir::{IrType, LpirOp, VReg};
use naga::{Handle, MathFunction, TypeInner};

use crate::lower_ctx::{LowerCtx, VRegVec, vector_size_usize};
use crate::lower_error::LowerError;
use crate::lower_math_helpers::{fconst, push_import_call};
use crate::lower_matrix;
use crate::naga_util::expr_type_inner;

pub(crate) fn try_lower_special(
    ctx: &mut LowerCtx<'_>,
    fun: MathFunction,
    arg: Handle<naga::Expression>,
    arg1: Option<Handle<naga::Expression>>,
    arg2: Option<Handle<naga::Expression>>,
    _arg3: Option<Handle<naga::Expression>>,
) -> Result<Option<VRegVec>, LowerError> {
    Ok(Some(match fun {
        MathFunction::Dot => {
            let a = ctx.ensure_expr_vec(arg)?;
            let b = ctx
                .ensure_expr_vec(arg1.ok_or_else(|| LowerError::Internal(String::from("dot")))?)?;
            let d = lower_matrix::emit_dot_product(ctx, &a, &b)?;
            smallvec::smallvec![d]
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
            emit_cross(ctx, &a, &b)?
        }
        MathFunction::Length => {
            let v = ctx.ensure_expr_vec(arg)?;
            let d = lower_matrix::emit_dot_product(ctx, &v, &v)?;
            let r = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(LpirOp::Fsqrt { dst: r, src: d });
            smallvec::smallvec![r]
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
                ctx.fb.push(LpirOp::Fsub {
                    dst: d,
                    lhs: a[i],
                    rhs: b[i],
                });
                diffs.push(d);
            }
            let d = lower_matrix::emit_dot_product(ctx, &diffs, &diffs)?;
            let r = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(LpirOp::Fsqrt { dst: r, src: d });
            smallvec::smallvec![r]
        }
        MathFunction::Normalize => {
            let v = ctx.ensure_expr_vec(arg)?;
            let len = {
                let d = lower_matrix::emit_dot_product(ctx, &v, &v)?;
                let r = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(LpirOp::Fsqrt { dst: r, src: d });
                r
            };
            let mut out = VRegVec::new();
            for &c in &v {
                let d = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(LpirOp::Fdiv {
                    dst: d,
                    lhs: c,
                    rhs: len,
                });
                out.push(d);
            }
            out
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
            ctx.fb.push(LpirOp::Flt {
                dst: cmp,
                lhs: d,
                rhs: z,
            });
            let mut out = VRegVec::new();
            for j in 0..n.len() {
                let neg = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(LpirOp::Fneg {
                    dst: neg,
                    src: n[j],
                });
                let dst = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(LpirOp::Select {
                    dst,
                    cond: cmp,
                    if_true: n[j],
                    if_false: neg,
                });
                out.push(dst);
            }
            out
        }
        MathFunction::Reflect => {
            let i = ctx.ensure_expr_vec(arg)?;
            let n = ctx.ensure_expr_vec(
                arg1.ok_or_else(|| LowerError::Internal(String::from("reflect")))?,
            )?;
            let two = fconst(ctx, 2.0);
            let ndi = lower_matrix::emit_dot_product(ctx, &n, &i)?;
            let scale = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(LpirOp::Fmul {
                dst: scale,
                lhs: two,
                rhs: ndi,
            });
            let mut out = VRegVec::new();
            for j in 0..i.len() {
                let pn = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(LpirOp::Fmul {
                    dst: pn,
                    lhs: scale,
                    rhs: n[j],
                });
                let dst = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(LpirOp::Fsub {
                    dst,
                    lhs: i[j],
                    rhs: pn,
                });
                out.push(dst);
            }
            out
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
            ctx.fb.push(LpirOp::Fmul {
                dst: ndi2,
                lhs: ndi,
                rhs: ndi,
            });
            let t1 = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(LpirOp::Fsub {
                dst: t1,
                lhs: one,
                rhs: ndi2,
            });
            let eta2 = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(LpirOp::Fmul {
                dst: eta2,
                lhs: eta_v,
                rhs: eta_v,
            });
            let k_inner = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(LpirOp::Fmul {
                dst: k_inner,
                lhs: eta2,
                rhs: t1,
            });
            let k = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(LpirOp::Fsub {
                dst: k,
                lhs: one,
                rhs: k_inner,
            });
            let z = fconst(ctx, 0.0);
            let k_neg = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::Flt {
                dst: k_neg,
                lhs: k,
                rhs: z,
            });
            let mut out = VRegVec::new();
            for j in 0..i.len() {
                let etai = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(LpirOp::Fmul {
                    dst: etai,
                    lhs: eta_v,
                    rhs: i[j],
                });
                let root = push_import_call(ctx, "lpir", "sqrt", &[k])?;
                let eta_ndi = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(LpirOp::Fmul {
                    dst: eta_ndi,
                    lhs: eta_v,
                    rhs: ndi,
                });
                let sum = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(LpirOp::Fadd {
                    dst: sum,
                    lhs: eta_ndi,
                    rhs: root,
                });
                let pn = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(LpirOp::Fmul {
                    dst: pn,
                    lhs: sum,
                    rhs: n[j],
                });
                let refr = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(LpirOp::Fsub {
                    dst: refr,
                    lhs: etai,
                    rhs: pn,
                });
                let zero = fconst(ctx, 0.0);
                let dst = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(LpirOp::Select {
                    dst,
                    cond: k_neg,
                    if_true: zero,
                    if_false: refr,
                });
                out.push(dst);
            }
            out
        }
        MathFunction::Transpose => {
            let inner = expr_type_inner(ctx.module, ctx.func, arg)?;
            let TypeInner::Matrix { columns, rows, .. } = inner else {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "transpose non-matrix",
                )));
            };
            let lc = vector_size_usize(columns);
            let lr = vector_size_usize(rows);
            let v = ctx.ensure_expr_vec(arg)?;
            lower_matrix::lower_transpose(&v, lc, lr)
        }
        MathFunction::Determinant => {
            let inner = expr_type_inner(ctx.module, ctx.func, arg)?;
            let TypeInner::Matrix { columns, rows, .. } = inner else {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "determinant non-matrix",
                )));
            };
            let lc = vector_size_usize(columns);
            let lr = vector_size_usize(rows);
            if lc != lr {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "determinant non-square",
                )));
            }
            let v = ctx.ensure_expr_vec(arg)?;
            let d = lower_matrix::lower_determinant(ctx, &v, lc)?;
            smallvec::smallvec![d]
        }
        MathFunction::Inverse => {
            let inner = expr_type_inner(ctx.module, ctx.func, arg)?;
            let TypeInner::Matrix { columns, rows, .. } = inner else {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "inverse non-matrix",
                )));
            };
            let lc = vector_size_usize(columns);
            let lr = vector_size_usize(rows);
            if lc != lr {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "inverse non-square",
                )));
            }
            let v = ctx.ensure_expr_vec(arg)?;
            lower_matrix::lower_inverse(ctx, &v, lc)?
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
                    ctx.fb.push(LpirOp::Fmul {
                        dst: d,
                        lhs: a[r],
                        rhs: b[c],
                    });
                    out.push(d);
                }
            }
            out
        }
        _ => return Ok(None),
    }))
}

fn emit_fsub_fmul_pair(
    ctx: &mut LowerCtx<'_>,
    a1: VReg,
    b1: VReg,
    a2: VReg,
    b2: VReg,
) -> Result<VReg, LowerError> {
    let p1 = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fmul {
        dst: p1,
        lhs: a1,
        rhs: b1,
    });
    let p2 = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fmul {
        dst: p2,
        lhs: a2,
        rhs: b2,
    });
    let d = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fsub {
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
