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
