//! Scalar [`ScalarKind`] inference for Naga expressions (scalar-only).

use alloc::format;
use alloc::string::String;

use naga::{BinaryOperator, Expression, Function, Handle, Literal, Module, ScalarKind, TypeInner};

use crate::lower_error::LowerError;

pub(crate) fn type_handle_scalar_kind(
    module: &Module,
    ty: Handle<naga::Type>,
) -> Result<ScalarKind, LowerError> {
    match &module.types[ty].inner {
        TypeInner::Scalar(s) => Ok(s.kind),
        _ => Err(LowerError::UnsupportedType(String::from(
            "expected scalar type",
        ))),
    }
}

pub(crate) fn expr_scalar_kind(
    module: &Module,
    func: &Function,
    expr: Handle<Expression>,
) -> Result<ScalarKind, LowerError> {
    match &func.expressions[expr] {
        Expression::Constant(h) => type_handle_scalar_kind(module, module.constants[*h].ty),
        Expression::Literal(l) => match l {
            Literal::F32(_) | Literal::F64(_) | Literal::F16(_) | Literal::AbstractFloat(_) => {
                Ok(ScalarKind::Float)
            }
            Literal::I32(_) | Literal::I64(_) | Literal::AbstractInt(_) => Ok(ScalarKind::Sint),
            Literal::U32(_) | Literal::U64(_) => Ok(ScalarKind::Uint),
            Literal::Bool(_) => Ok(ScalarKind::Bool),
        },
        Expression::FunctionArgument(i) => {
            let arg = func
                .arguments
                .get(*i as usize)
                .ok_or_else(|| LowerError::Internal(String::from("bad argument index")))?;
            type_handle_scalar_kind(module, arg.ty)
        }
        Expression::LocalVariable(lv) => {
            let lv_ty = func.local_variables[*lv].ty;
            type_handle_scalar_kind(module, lv_ty)
        }
        Expression::Load { pointer } => match &func.expressions[*pointer] {
            Expression::LocalVariable(lv) => {
                let lv_ty = func.local_variables[*lv].ty;
                type_handle_scalar_kind(module, lv_ty)
            }
            _ => expr_scalar_kind(module, func, *pointer),
        },
        Expression::Binary { op, left, .. } => match op {
            BinaryOperator::Equal
            | BinaryOperator::NotEqual
            | BinaryOperator::Less
            | BinaryOperator::LessEqual
            | BinaryOperator::Greater
            | BinaryOperator::GreaterEqual
            | BinaryOperator::LogicalAnd
            | BinaryOperator::LogicalOr => Ok(ScalarKind::Bool),
            _ => expr_scalar_kind(module, func, *left),
        },
        Expression::Unary { expr: inner, .. } => expr_scalar_kind(module, func, *inner),
        Expression::Select { accept, .. } => expr_scalar_kind(module, func, *accept),
        Expression::As { kind, .. } => Ok(*kind),
        Expression::CallResult(fh) => {
            let ret = module.functions[*fh].result.as_ref().ok_or_else(|| {
                LowerError::UnsupportedExpression(String::from("CallResult for void function"))
            })?;
            type_handle_scalar_kind(module, ret.ty)
        }
        Expression::ZeroValue(ty_h) => match &module.types[*ty_h].inner {
            TypeInner::Scalar(s) => Ok(s.kind),
            _ => Err(LowerError::UnsupportedType(String::from(
                "ZeroValue non-scalar",
            ))),
        },
        Expression::Math { arg, .. } => expr_scalar_kind(module, func, *arg),
        _ => Err(LowerError::UnsupportedExpression(format!(
            "cannot infer scalar kind for {:?}",
            func.expressions[expr]
        ))),
    }
}
