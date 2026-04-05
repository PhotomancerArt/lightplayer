//! Matrix × vector, matrix × matrix, transpose, determinant, inverse → scalar LPIR ops.

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use lpir::{IrType, Op, VReg};
use smallvec::SmallVec;

use crate::lower_ctx::{LowerCtx, VRegVec};
use crate::lower_error::LowerError;

pub(crate) fn mat_elem(vregs: &[VReg], rows: usize, col: usize, row: usize) -> VReg {
    vregs[col * rows + row]
}

pub(crate) fn emit_dot_product(
    ctx: &mut LowerCtx<'_>,
    a: &[VReg],
    b: &[VReg],
) -> Result<VReg, LowerError> {
    if a.len() != b.len() || a.is_empty() {
        return Err(LowerError::Internal(String::from(
            "dot product length mismatch",
        )));
    }
    let mut sum = {
        let d = ctx.fb.alloc_vreg(IrType::F32);
        ctx.fb.push(Op::Fmul {
            dst: d,
            lhs: a[0],
            rhs: b[0],
        });
        d
    };
    for i in 1..a.len() {
        let prod = ctx.fb.alloc_vreg(IrType::F32);
        ctx.fb.push(Op::Fmul {
            dst: prod,
            lhs: a[i],
            rhs: b[i],
        });
        let next = ctx.fb.alloc_vreg(IrType::F32);
        ctx.fb.push(Op::Fadd {
            dst: next,
            lhs: sum,
            rhs: prod,
        });
        sum = next;
    }
    Ok(sum)
}

pub(crate) fn lower_mat_vec_mul(
    ctx: &mut LowerCtx<'_>,
    mat: &[VReg],
    vec: &[VReg],
    cols: usize,
    rows: usize,
) -> Result<VRegVec, LowerError> {
    let mut result = VRegVec::new();
    for r in 0..rows {
        let mut sum = {
            let d = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(Op::Fmul {
                dst: d,
                lhs: mat_elem(mat, rows, 0, r),
                rhs: vec[0],
            });
            d
        };
        for c in 1..cols {
            let prod = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(Op::Fmul {
                dst: prod,
                lhs: mat_elem(mat, rows, c, r),
                rhs: vec[c],
            });
            let next = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(Op::Fadd {
                dst: next,
                lhs: sum,
                rhs: prod,
            });
            sum = next;
        }
        result.push(sum);
    }
    Ok(result)
}

pub(crate) fn lower_vec_mat_mul(
    ctx: &mut LowerCtx<'_>,
    vec: &[VReg],
    mat: &[VReg],
    cols: usize,
    rows: usize,
) -> Result<VRegVec, LowerError> {
    let mut result = VRegVec::new();
    for c in 0..cols {
        let col_start = c * rows;
        let col = &mat[col_start..col_start + rows];
        let dot = emit_dot_product(ctx, vec, col)?;
        result.push(dot);
    }
    Ok(result)
}

pub(crate) fn lower_mat_mat_mul(
    ctx: &mut LowerCtx<'_>,
    left: &[VReg],
    right: &[VReg],
    left_cols: usize,
    left_rows: usize,
    right_cols: usize,
    right_rows: usize,
) -> Result<VRegVec, LowerError> {
    if left_cols != right_rows {
        return Err(LowerError::Internal(format!(
            "mat*mat inner mismatch {left_cols} vs {right_rows}"
        )));
    }
    let mut result = VRegVec::new();
    for c in 0..right_cols {
        let right_col_start = c * right_rows;
        let right_col = &right[right_col_start..right_col_start + right_rows];
        let out_col = lower_mat_vec_mul(ctx, left, right_col, left_cols, left_rows)?;
        result.extend_from_slice(&out_col);
    }
    Ok(result)
}

pub(crate) fn lower_transpose(mat: &[VReg], cols: usize, rows: usize) -> VRegVec {
    let mut result = VRegVec::new();
    for new_c in 0..rows {
        for new_r in 0..cols {
            result.push(mat[new_r * rows + new_c]);
        }
    }
    result
}

pub(crate) fn lower_determinant(
    ctx: &mut LowerCtx<'_>,
    mat: &[VReg],
    size: usize,
) -> Result<VReg, LowerError> {
    match size {
        2 => det2(ctx, mat),
        3 => det3(ctx, mat),
        4 => det4(ctx, mat),
        _ => Err(LowerError::UnsupportedExpression(format!(
            "determinant size {size}"
        ))),
    }
}

fn det2(ctx: &mut LowerCtx<'_>, m: &[VReg]) -> Result<VReg, LowerError> {
    let a = m[0];
    let b = m[1];
    let c = m[2];
    let d = m[3];
    let p1 = fmul(ctx, a, d);
    let p2 = fmul(ctx, b, c);
    fsub(ctx, p1, p2)
}

fn det3(ctx: &mut LowerCtx<'_>, m: &[VReg]) -> Result<VReg, LowerError> {
    let a = m[0];
    let b = m[1];
    let c = m[2];
    let d = m[3];
    let e = m[4];
    let f = m[5];
    let g = m[6];
    let h = m[7];
    let i = m[8];
    let ei = fmul(ctx, e, i);
    let fh = fmul(ctx, f, h);
    let m1 = fsub(ctx, ei, fh)?;
    let term1 = fmul(ctx, a, m1);
    let di = fmul(ctx, d, i);
    let fg = fmul(ctx, f, g);
    let m2 = fsub(ctx, di, fg)?;
    let term2 = fmul(ctx, b, m2);
    let dh = fmul(ctx, d, h);
    let eg = fmul(ctx, e, g);
    let m3 = fsub(ctx, dh, eg)?;
    let term3 = fmul(ctx, c, m3);
    let s12 = fsub(ctx, term1, term2)?;
    fadd(ctx, s12, term3)
}

fn det4(ctx: &mut LowerCtx<'_>, m: &[VReg]) -> Result<VReg, LowerError> {
    // Laplace expansion along column 0: m[col*4+row], col=0 → m[0..4].
    let c0 = minor3_det(ctx, m, 0, 0)?;
    let c1 = minor3_det(ctx, m, 0, 1)?;
    let c2 = minor3_det(ctx, m, 0, 2)?;
    let c3 = minor3_det(ctx, m, 0, 3)?;
    let t0 = fmul(ctx, m[0], c0);
    let t1 = fmul(ctx, m[1], c1);
    let t2 = fmul(ctx, m[2], c2);
    let t3 = fmul(ctx, m[3], c3);
    let s01 = fsub(ctx, t0, t1)?;
    let s23 = fsub(ctx, t2, t3)?;
    fadd(ctx, s01, s23)
}

/// Determinant of 3×3 minor after removing column `skip_col` and row `skip_row` (4×4 only).
fn minor3_det(
    ctx: &mut LowerCtx<'_>,
    m: &[VReg],
    skip_col: usize,
    skip_row: usize,
) -> Result<VReg, LowerError> {
    let mut buf = Vec::with_capacity(9);
    for col in 0..4 {
        for row in 0..4 {
            if col == skip_col || row == skip_row {
                continue;
            }
            buf.push(m[col * 4 + row]);
        }
    }
    debug_assert_eq!(buf.len(), 9);
    let sign = if (skip_col + skip_row) % 2 == 0 {
        1.0f32
    } else {
        -1.0f32
    };
    let d = det3(ctx, &buf)?;
    fmul_imm(ctx, d, sign)
}

pub(crate) fn lower_inverse(
    ctx: &mut LowerCtx<'_>,
    mat: &[VReg],
    size: usize,
) -> Result<VRegVec, LowerError> {
    let det = lower_determinant(ctx, mat, size)?;
    match size {
        2 => inv2(ctx, mat, det),
        3 => inv3(ctx, mat, det),
        4 => inv4(ctx, mat, det),
        _ => Err(LowerError::UnsupportedExpression(format!(
            "inverse size {size}"
        ))),
    }
}

fn inv2(ctx: &mut LowerCtx<'_>, m: &[VReg], det: VReg) -> Result<VRegVec, LowerError> {
    let a = m[0];
    let b = m[1];
    let c = m[2];
    let d = m[3];
    let inv_det = fdiv_one(ctx, det)?;
    let o0 = fmul(ctx, d, inv_det);
    let nb = fneg(ctx, b)?;
    let o1 = fmul(ctx, nb, inv_det);
    let nc = fneg(ctx, c)?;
    let o2 = fmul(ctx, nc, inv_det);
    let o3 = fmul(ctx, a, inv_det);
    Ok(SmallVec::from_vec(vec![o0, o1, o2, o3]))
}

/// `inv[col*3+row] = cofactor(row,col) / det` for column-major `m`.
fn inv3(ctx: &mut LowerCtx<'_>, m: &[VReg], det: VReg) -> Result<VRegVec, LowerError> {
    let inv_det = fdiv_one(ctx, det)?;
    let mut out = VRegVec::new();
    for col in 0..3 {
        for row in 0..3 {
            let cof = cofactor3(ctx, m, col, row)?;
            out.push(fmul(ctx, cof, inv_det));
        }
    }
    Ok(out)
}

fn cofactor3(
    ctx: &mut LowerCtx<'_>,
    m: &[VReg],
    skip_col: usize,
    skip_row: usize,
) -> Result<VReg, LowerError> {
    let mut buf = Vec::with_capacity(9);
    for col in 0..3 {
        for row in 0..3 {
            if col == skip_col || row == skip_row {
                continue;
            }
            buf.push(m[col * 3 + row]);
        }
    }
    debug_assert_eq!(buf.len(), 4);
    let a = buf[0];
    let b = buf[1];
    let c = buf[2];
    let d = buf[3];
    let ad = fmul(ctx, a, d);
    let bc = fmul(ctx, b, c);
    let minor = fsub(ctx, ad, bc)?;
    let sign = if (skip_col + skip_row) % 2 == 0 {
        1.0f32
    } else {
        -1.0f32
    };
    fmul_imm(ctx, minor, sign)
}

fn inv4(ctx: &mut LowerCtx<'_>, m: &[VReg], det: VReg) -> Result<VRegVec, LowerError> {
    let inv_det = fdiv_one(ctx, det)?;
    let mut out = VRegVec::new();
    for col in 0..4 {
        for row in 0..4 {
            let cof = cofactor4(ctx, m, col, row)?;
            out.push(fmul(ctx, cof, inv_det));
        }
    }
    Ok(out)
}

fn cofactor4(
    ctx: &mut LowerCtx<'_>,
    m: &[VReg],
    skip_col: usize,
    skip_row: usize,
) -> Result<VReg, LowerError> {
    let mut buf = Vec::with_capacity(9);
    for col in 0..4 {
        for row in 0..4 {
            if col == skip_col || row == skip_row {
                continue;
            }
            buf.push(m[col * 4 + row]);
        }
    }
    debug_assert_eq!(buf.len(), 9);
    let minor = det3(ctx, &buf)?;
    let sign = if (skip_col + skip_row) % 2 == 0 {
        1.0f32
    } else {
        -1.0f32
    };
    fmul_imm(ctx, minor, sign)
}

fn fmul(ctx: &mut LowerCtx<'_>, lhs: VReg, rhs: VReg) -> VReg {
    let d = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fmul { dst: d, lhs, rhs });
    d
}

fn fadd(ctx: &mut LowerCtx<'_>, lhs: VReg, rhs: VReg) -> Result<VReg, LowerError> {
    let d = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fadd { dst: d, lhs, rhs });
    Ok(d)
}

fn fsub(ctx: &mut LowerCtx<'_>, lhs: VReg, rhs: VReg) -> Result<VReg, LowerError> {
    let d = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fsub { dst: d, lhs, rhs });
    Ok(d)
}

fn fneg(ctx: &mut LowerCtx<'_>, src: VReg) -> Result<VReg, LowerError> {
    let d = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fneg { dst: d, src });
    Ok(d)
}

fn fmul_imm(ctx: &mut LowerCtx<'_>, src: VReg, imm: f32) -> Result<VReg, LowerError> {
    let c = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::FconstF32 { dst: c, value: imm });
    Ok(fmul(ctx, src, c))
}

fn fdiv_one(ctx: &mut LowerCtx<'_>, denom: VReg) -> Result<VReg, LowerError> {
    let one = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::FconstF32 {
        dst: one,
        value: 1.0,
    });
    let d = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fdiv {
        dst: d,
        lhs: one,
        rhs: denom,
    });
    Ok(d)
}
