use alloc::vec::Vec;

use lpir::{IrType, LpirOp};
use lps_shared::LpsType;

use crate::{Diagnostic, Span};

use super::super::{LowerCtx, LowerValue};

pub(in crate::lower::ops) fn lower_matrix_multiply(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    lhs: LowerValue,
    rhs: LowerValue,
    result_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let Some((cols, rows)) = result_ty.matrix_dims() else {
        return Err(Diagnostic::error(
            span,
            "matrix multiply result must be matrix",
        ));
    };
    if cols != rows || lhs.lanes.len() != cols * rows || rhs.lanes.len() != cols * rows {
        return Err(Diagnostic::error(span, "unsupported matrix multiply shape"));
    }
    let mut lanes = Vec::new();
    for col in 0..cols {
        for row in 0..rows {
            let mut acc = None;
            for k in 0..cols {
                let product = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(LpirOp::Fmul {
                    dst: product,
                    lhs: lhs.lanes[k * rows + row],
                    rhs: rhs.lanes[col * rows + k],
                });
                acc = Some(if let Some(prev) = acc {
                    let sum = ctx.fb.alloc_vreg(IrType::F32);
                    ctx.fb.push(LpirOp::Fadd {
                        dst: sum,
                        lhs: prev,
                        rhs: product,
                    });
                    sum
                } else {
                    product
                });
            }
            lanes.push(acc.ok_or_else(|| Diagnostic::error(span, "empty matrix multiply"))?);
        }
    }
    Ok(LowerValue {
        ty: result_ty.clone(),
        lanes,
    })
}

pub(in crate::lower::ops) fn lower_matrix_vector_multiply(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    lhs: LowerValue,
    rhs: LowerValue,
    result_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    if lhs.ty.is_matrix() {
        return lower_matrix_times_vector(ctx, span, lhs, rhs, result_ty);
    }
    if rhs.ty.is_matrix() {
        return lower_vector_times_matrix(ctx, span, lhs, rhs, result_ty);
    }
    Err(Diagnostic::error(
        span,
        "matrix-vector multiply requires a matrix operand",
    ))
}

pub(in crate::lower::ops) fn lower_matrix_transpose(
    _ctx: &mut LowerCtx<'_>,
    span: Span,
    value: LowerValue,
    result_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let Some((cols, rows)) = value.ty.matrix_dims() else {
        return Err(Diagnostic::error(span, "transpose expects matrix"));
    };
    if cols != rows || result_ty.matrix_dims() != Some((cols, rows)) {
        return Err(Diagnostic::error(span, "unsupported transpose shape"));
    }
    let mut lanes = Vec::new();
    for col in 0..cols {
        for row in 0..rows {
            lanes.push(value.lanes[row * rows + col]);
        }
    }
    Ok(LowerValue {
        ty: result_ty.clone(),
        lanes,
    })
}

pub(in crate::lower::ops) fn lower_matrix_determinant(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    value: LowerValue,
    result_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let Some((cols, rows)) = value.ty.matrix_dims() else {
        return Err(Diagnostic::error(span, "determinant expects matrix"));
    };
    if cols != rows || *result_ty != LpsType::Float {
        return Err(Diagnostic::error(span, "unsupported determinant shape"));
    }
    let det = determinant_lanes(ctx, rows, &value.lanes)?;
    Ok(LowerValue {
        ty: LpsType::Float,
        lanes: alloc::vec![det],
    })
}

pub(in crate::lower::ops) fn lower_matrix_inverse(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    value: LowerValue,
    result_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let Some((cols, rows)) = value.ty.matrix_dims() else {
        return Err(Diagnostic::error(span, "inverse expects matrix"));
    };
    if cols != rows || value.ty != *result_ty {
        return Err(Diagnostic::error(span, "unsupported inverse shape"));
    }
    let det = determinant_lanes(ctx, rows, &value.lanes)?;
    let lanes = inverse_lanes(ctx, span, rows, &value.lanes, det)?;
    Ok(LowerValue {
        ty: result_ty.clone(),
        lanes,
    })
}

fn lower_matrix_times_vector(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    matrix: LowerValue,
    vector: LowerValue,
    result_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let Some((cols, rows)) = matrix.ty.matrix_dims() else {
        return Err(Diagnostic::error(span, "left operand must be matrix"));
    };
    if matrix.lanes.len() != cols * rows || vector.lanes.len() != cols {
        return Err(Diagnostic::error(
            span,
            "unsupported matrix-vector multiply shape",
        ));
    }
    let mut lanes = Vec::new();
    for row in 0..rows {
        let mut acc = None;
        for col in 0..cols {
            let product = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(LpirOp::Fmul {
                dst: product,
                lhs: matrix.lanes[col * rows + row],
                rhs: vector.lanes[col],
            });
            acc = Some(sum_product(ctx, acc, product));
        }
        lanes.push(acc.ok_or_else(|| Diagnostic::error(span, "empty matrix-vector multiply"))?);
    }
    Ok(LowerValue {
        ty: result_ty.clone(),
        lanes,
    })
}

fn determinant_lanes(
    ctx: &mut LowerCtx<'_>,
    size: usize,
    lanes: &[lpir::VReg],
) -> Result<lpir::VReg, Diagnostic> {
    if size == 1 {
        return Ok(lanes[0]);
    }
    if size == 2 {
        let ad = ctx.fb.alloc_vreg(IrType::F32);
        let bc = ctx.fb.alloc_vreg(IrType::F32);
        let dst = ctx.fb.alloc_vreg(IrType::F32);
        ctx.fb.push(LpirOp::Fmul {
            dst: ad,
            lhs: lanes[0],
            rhs: lanes[3],
        });
        ctx.fb.push(LpirOp::Fmul {
            dst: bc,
            lhs: lanes[2],
            rhs: lanes[1],
        });
        ctx.fb.push(LpirOp::Fsub {
            dst,
            lhs: ad,
            rhs: bc,
        });
        return Ok(dst);
    }

    let mut acc = None;
    for col in 0..size {
        let mut minor = Vec::new();
        for minor_col in 0..size {
            if minor_col == col {
                continue;
            }
            for row in 1..size {
                minor.push(lanes[minor_col * size + row]);
            }
        }
        let sub_det = determinant_lanes(ctx, size - 1, &minor)?;
        let term = ctx.fb.alloc_vreg(IrType::F32);
        ctx.fb.push(LpirOp::Fmul {
            dst: term,
            lhs: lanes[col * size],
            rhs: sub_det,
        });
        acc = Some(if let Some(prev) = acc {
            let next = ctx.fb.alloc_vreg(IrType::F32);
            if col % 2 == 0 {
                ctx.fb.push(LpirOp::Fadd {
                    dst: next,
                    lhs: prev,
                    rhs: term,
                });
            } else {
                ctx.fb.push(LpirOp::Fsub {
                    dst: next,
                    lhs: prev,
                    rhs: term,
                });
            }
            next
        } else if col % 2 == 0 {
            term
        } else {
            let zero = ctx.fb.alloc_vreg(IrType::F32);
            let next = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(LpirOp::FconstF32 {
                dst: zero,
                value: 0.0,
            });
            ctx.fb.push(LpirOp::Fsub {
                dst: next,
                lhs: zero,
                rhs: term,
            });
            next
        });
    }
    acc.ok_or_else(|| Diagnostic::error(Span::new(0, 0), "empty determinant"))
}

fn inverse_lanes(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    size: usize,
    lanes: &[lpir::VReg],
    det: lpir::VReg,
) -> Result<Vec<lpir::VReg>, Diagnostic> {
    match size {
        2 => inverse2(ctx, lanes, det),
        3 | 4 => inverse_by_cofactors(ctx, span, size, lanes, det),
        _ => Err(Diagnostic::error(span, "unsupported inverse size")),
    }
}

fn inverse2(
    ctx: &mut LowerCtx<'_>,
    lanes: &[lpir::VReg],
    det: lpir::VReg,
) -> Result<Vec<lpir::VReg>, Diagnostic> {
    if lanes.len() != 4 {
        return Err(Diagnostic::error(Span::new(0, 0), "invalid mat2 inverse"));
    }
    let inv_det = fdiv_one(ctx, det);
    let neg_b = fneg(ctx, lanes[1]);
    let neg_c = fneg(ctx, lanes[2]);
    Ok(alloc::vec![
        fmul(ctx, lanes[3], inv_det),
        fmul(ctx, neg_b, inv_det),
        fmul(ctx, neg_c, inv_det),
        fmul(ctx, lanes[0], inv_det),
    ])
}

fn inverse_by_cofactors(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    size: usize,
    lanes: &[lpir::VReg],
    det: lpir::VReg,
) -> Result<Vec<lpir::VReg>, Diagnostic> {
    let inv_det = fdiv_one(ctx, det);
    let mut out = Vec::new();
    for col in 0..size {
        for row in 0..size {
            let cofactor = cofactor(ctx, span, lanes, size, col, row)?;
            out.push(fmul(ctx, cofactor, inv_det));
        }
    }
    Ok(out)
}

fn cofactor(
    ctx: &mut LowerCtx<'_>,
    _span: Span,
    lanes: &[lpir::VReg],
    size: usize,
    skip_col: usize,
    skip_row: usize,
) -> Result<lpir::VReg, Diagnostic> {
    let mut minor = Vec::new();
    for col in 0..size {
        for row in 0..size {
            if col == skip_col || row == skip_row {
                continue;
            }
            minor.push(lanes[col * size + row]);
        }
    }
    let det = determinant_lanes(ctx, size - 1, &minor)?;
    if (skip_col + skip_row).is_multiple_of(2) {
        Ok(det)
    } else {
        Ok(fneg(ctx, det))
    }
}

fn lower_vector_times_matrix(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    vector: LowerValue,
    matrix: LowerValue,
    result_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let Some((cols, rows)) = matrix.ty.matrix_dims() else {
        return Err(Diagnostic::error(span, "right operand must be matrix"));
    };
    if matrix.lanes.len() != cols * rows || vector.lanes.len() != rows {
        return Err(Diagnostic::error(
            span,
            "unsupported vector-matrix multiply shape",
        ));
    }
    let mut lanes = Vec::new();
    for col in 0..cols {
        let mut acc = None;
        for row in 0..rows {
            let product = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(LpirOp::Fmul {
                dst: product,
                lhs: vector.lanes[row],
                rhs: matrix.lanes[col * rows + row],
            });
            acc = Some(sum_product(ctx, acc, product));
        }
        lanes.push(acc.ok_or_else(|| Diagnostic::error(span, "empty vector-matrix multiply"))?);
    }
    Ok(LowerValue {
        ty: result_ty.clone(),
        lanes,
    })
}

fn sum_product(ctx: &mut LowerCtx<'_>, acc: Option<lpir::VReg>, product: lpir::VReg) -> lpir::VReg {
    if let Some(prev) = acc {
        let sum = ctx.fb.alloc_vreg(IrType::F32);
        ctx.fb.push(LpirOp::Fadd {
            dst: sum,
            lhs: prev,
            rhs: product,
        });
        sum
    } else {
        product
    }
}

fn fmul(ctx: &mut LowerCtx<'_>, lhs: lpir::VReg, rhs: lpir::VReg) -> lpir::VReg {
    let dst = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fmul { dst, lhs, rhs });
    dst
}

fn fneg(ctx: &mut LowerCtx<'_>, src: lpir::VReg) -> lpir::VReg {
    let dst = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fneg { dst, src });
    dst
}

fn fdiv_one(ctx: &mut LowerCtx<'_>, rhs: lpir::VReg) -> lpir::VReg {
    let one = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::FconstF32 {
        dst: one,
        value: 1.0,
    });
    let dst = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fdiv { dst, lhs: one, rhs });
    dst
}
