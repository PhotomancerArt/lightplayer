//! Shared helpers for math lowering (imports, constants, dispatch width).

use alloc::format;

use lpir::{IrType, Op, VReg};
use naga::{BinaryOperator, Expression, Function, Handle, Module};

use crate::lower_ctx::{LowerCtx, naga_type_width};
use crate::lower_error::LowerError;
use crate::naga_util::expr_type_inner;

pub(crate) fn push_import_call(
    ctx: &mut LowerCtx<'_>,
    module: &'static str,
    name: &'static str,
    args: &[VReg],
) -> Result<VReg, LowerError> {
    let key = format!("{module}::{name}");
    let callee = ctx
        .import_map
        .get(&key)
        .copied()
        .ok_or_else(|| LowerError::Internal(format!("missing import {key}")))?;
    let r = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push_call(callee, args, &[r]);
    Ok(r)
}

pub(crate) fn fconst(ctx: &mut LowerCtx<'_>, value: f32) -> VReg {
    let v = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::FconstF32 { dst: v, value });
    v
}

pub(crate) fn vat(v: &[VReg], i: usize) -> VReg {
    v[i.min(v.len().saturating_sub(1))]
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
pub(crate) fn math_dispatch_width_expr(
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
