//! [`ScalarKind`] inference and expression type shape for Naga IR.

use alloc::format;
use alloc::string::String;

use naga::{
    BinaryOperator, Expression, Function, Handle, Literal, MathFunction, Module,
    RelationalFunction, Scalar, ScalarKind, TypeInner, VectorSize,
};

use crate::lower_ctx::vector_size_usize;
use crate::lower_error::LowerError;

pub(crate) fn type_handle_scalar_kind(
    module: &Module,
    ty: Handle<naga::Type>,
) -> Result<ScalarKind, LowerError> {
    match &module.types[ty].inner {
        TypeInner::Scalar(s) => Ok(s.kind),
        TypeInner::Vector { scalar, .. } | TypeInner::Matrix { scalar, .. } => Ok(scalar.kind),
        _ => Err(LowerError::UnsupportedType(String::from(
            "expected scalar, vector, or matrix type",
        ))),
    }
}

/// Result type of an expression as [`TypeInner`] (by value; synthesized for comparisons, `As`, etc.).
pub(crate) fn expr_type_inner(
    module: &Module,
    func: &Function,
    expr: Handle<Expression>,
) -> Result<TypeInner, LowerError> {
    match &func.expressions[expr] {
        Expression::Literal(lit) => Ok(literal_type_inner(lit)),
        Expression::Constant(h) => Ok(module.types[module.constants[*h].ty].inner.clone()),
        Expression::ZeroValue(ty_h) => Ok(module.types[*ty_h].inner.clone()),
        Expression::Compose { ty, .. } => Ok(module.types[*ty].inner.clone()),
        Expression::FunctionArgument(i) => {
            let arg = func
                .arguments
                .get(*i as usize)
                .ok_or_else(|| LowerError::Internal(String::from("bad argument index")))?;
            // Keep Pointer type for `inout`/`out` so `Load`/`expr_type_inner` see a pointer.
            Ok(module.types[arg.ty].inner.clone())
        }
        Expression::LocalVariable(lv) => Ok(TypeInner::Pointer {
            base: func.local_variables[*lv].ty,
            space: naga::AddressSpace::Function,
        }),
        Expression::Load { pointer } => match expr_type_inner(module, func, *pointer)? {
            TypeInner::Pointer { base, space: _ } => {
                if let TypeInner::Atomic(scalar) = module.types[base].inner {
                    Ok(TypeInner::Scalar(scalar))
                } else {
                    Ok(module.types[base].inner.clone())
                }
            }
            TypeInner::ValuePointer {
                size,
                scalar,
                space: _,
            } => Ok(match size {
                Some(size) => TypeInner::Vector { size, scalar },
                None => TypeInner::Scalar(scalar),
            }),
            _ => Err(LowerError::UnsupportedExpression(String::from(
                "Load from non-pointer",
            ))),
        },
        Expression::Splat { size, value } => match expr_type_inner(module, func, *value)? {
            TypeInner::Scalar(scalar) => Ok(TypeInner::Vector {
                size: *size,
                scalar,
            }),
            _ => Err(LowerError::UnsupportedExpression(String::from(
                "Splat of non-scalar",
            ))),
        },
        Expression::Swizzle { size, vector, .. } => match expr_type_inner(module, func, *vector)? {
            TypeInner::Vector { scalar, .. } => Ok(TypeInner::Vector {
                size: *size,
                scalar,
            }),
            _ => Err(LowerError::UnsupportedExpression(String::from(
                "Swizzle of non-vector",
            ))),
        },
        Expression::AccessIndex { base, index } => {
            let base_ty = expr_type_inner(module, func, *base)?;
            match base_ty {
                TypeInner::Vector { size, scalar } => {
                    if *index >= vector_size_usize(size) as u32 {
                        return Err(LowerError::UnsupportedExpression(format!(
                            "AccessIndex {index} out of bounds for vector"
                        )));
                    }
                    Ok(TypeInner::Scalar(scalar))
                }
                TypeInner::Matrix {
                    columns,
                    rows,
                    scalar,
                } => {
                    if *index >= vector_size_usize(columns) as u32 {
                        return Err(LowerError::UnsupportedExpression(format!(
                            "AccessIndex {index} out of bounds for matrix columns"
                        )));
                    }
                    Ok(TypeInner::Vector { size: rows, scalar })
                }
                TypeInner::Pointer { base, space } => match &module.types[base].inner {
                    TypeInner::Vector { size, scalar } => {
                        if *index >= vector_size_usize(*size) as u32 {
                            return Err(LowerError::UnsupportedExpression(format!(
                                "AccessIndex {index} out of bounds"
                            )));
                        }
                        Ok(TypeInner::ValuePointer {
                            size: None,
                            scalar: *scalar,
                            space,
                        })
                    }
                    TypeInner::Matrix {
                        columns,
                        rows,
                        scalar,
                    } => {
                        if *index >= vector_size_usize(*columns) as u32 {
                            return Err(LowerError::UnsupportedExpression(format!(
                                "AccessIndex {index} out of bounds for matrix"
                            )));
                        }
                        Ok(TypeInner::ValuePointer {
                            size: Some(*rows),
                            scalar: *scalar,
                            space,
                        })
                    }
                    TypeInner::Array { base: elt, .. } => Ok(module.types[*elt].inner.clone()),
                    _ => Err(LowerError::UnsupportedExpression(String::from(
                        "AccessIndex base not vector/matrix/array",
                    ))),
                },
                // e.g. `t[0][1]`: `t[0]` is a column pointer (`ValuePointer` to vecN).
                TypeInner::ValuePointer {
                    size: Some(vec_size),
                    scalar,
                    space,
                } => {
                    if *index >= vector_size_usize(vec_size) as u32 {
                        return Err(LowerError::UnsupportedExpression(format!(
                            "AccessIndex {index} out of bounds for value pointer vector"
                        )));
                    }
                    Ok(TypeInner::ValuePointer {
                        size: None,
                        scalar,
                        space,
                    })
                }
                other => Err(LowerError::UnsupportedExpression(format!(
                    "AccessIndex on unsupported base {other:?}"
                ))),
            }
        }
        Expression::Binary { op, left, .. } => match op {
            BinaryOperator::Equal
            | BinaryOperator::NotEqual
            | BinaryOperator::Less
            | BinaryOperator::LessEqual
            | BinaryOperator::Greater
            | BinaryOperator::GreaterEqual
            | BinaryOperator::LogicalAnd
            | BinaryOperator::LogicalOr => {
                let li = expr_type_inner(module, func, *left)?;
                let bool_scalar = Scalar {
                    kind: ScalarKind::Bool,
                    width: 4,
                };
                match li {
                    TypeInner::Vector { size, .. } => Ok(TypeInner::Vector {
                        size,
                        scalar: bool_scalar,
                    }),
                    _ => Ok(TypeInner::Scalar(bool_scalar)),
                }
            }
            _ => expr_type_inner(module, func, *left),
        },
        Expression::Unary { expr: inner, .. } => expr_type_inner(module, func, *inner),
        Expression::Select { accept, .. } => expr_type_inner(module, func, *accept),
        Expression::As { kind, .. } => Ok(TypeInner::Scalar(Scalar {
            kind: *kind,
            width: 4,
        })),
        Expression::CallResult(fh) => {
            let ret = module.functions[*fh].result.as_ref().ok_or_else(|| {
                LowerError::UnsupportedExpression(String::from("CallResult for void function"))
            })?;
            Ok(module.types[ret.ty].inner.clone())
        }
        Expression::Math { fun, arg, arg1, .. } => {
            math_result_type_inner(module, func, *fun, *arg, *arg1)
        }
        _ => Err(LowerError::UnsupportedExpression(format!(
            "expr_type_inner unsupported {:?}",
            func.expressions[expr]
        ))),
    }
}

fn literal_type_inner(lit: &Literal) -> TypeInner {
    let (kind, width) = match lit {
        Literal::F32(_) | Literal::F64(_) | Literal::F16(_) | Literal::AbstractFloat(_) => {
            (ScalarKind::Float, 4)
        }
        Literal::I32(_) | Literal::I64(_) | Literal::AbstractInt(_) => (ScalarKind::Sint, 4),
        Literal::U32(_) | Literal::U64(_) => (ScalarKind::Uint, 4),
        Literal::Bool(_) => (ScalarKind::Bool, 4),
    };
    TypeInner::Scalar(Scalar { kind, width })
}

fn math_result_type_inner(
    module: &Module,
    func: &Function,
    fun: MathFunction,
    arg: Handle<Expression>,
    arg1: Option<Handle<Expression>>,
) -> Result<TypeInner, LowerError> {
    let arg_ty = expr_type_inner(module, func, arg)?;
    match fun {
        MathFunction::Dot | MathFunction::Length | MathFunction::Distance => {
            Ok(TypeInner::Scalar(Scalar {
                kind: ScalarKind::Float,
                width: 4,
            }))
        }
        MathFunction::Cross => Ok(TypeInner::Vector {
            size: VectorSize::Tri,
            scalar: Scalar {
                kind: ScalarKind::Float,
                width: 4,
            },
        }),
        MathFunction::Transpose => {
            let TypeInner::Matrix {
                columns,
                rows,
                scalar,
            } = arg_ty
            else {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "transpose non-matrix",
                )));
            };
            Ok(TypeInner::Matrix {
                columns: rows,
                rows: columns,
                scalar,
            })
        }
        MathFunction::Determinant => Ok(TypeInner::Scalar(Scalar {
            kind: ScalarKind::Float,
            width: 4,
        })),
        MathFunction::Inverse => Ok(arg_ty),
        MathFunction::Outer => {
            let Some(a1) = arg1 else {
                return Err(LowerError::Internal(String::from("outer missing arg")));
            };
            let t0 = expr_type_inner(module, func, arg)?;
            let t1 = expr_type_inner(module, func, a1)?;
            let (s0, k0) = vector_kind_rows(&t0)?;
            let (s1, k1) = vector_kind_rows(&t1)?;
            if k0.kind != ScalarKind::Float || k1.kind != ScalarKind::Float {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "outer non-float",
                )));
            }
            Ok(TypeInner::Matrix {
                columns: s1,
                rows: s0,
                scalar: k0,
            })
        }
        _ => Ok(arg_ty),
    }
}

fn vector_kind_rows(inner: &TypeInner) -> Result<(VectorSize, Scalar), LowerError> {
    match *inner {
        TypeInner::Vector { size, scalar } => Ok((size, scalar)),
        _ => Err(LowerError::UnsupportedExpression(String::from(
            "expected vector for outer",
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
            match &module.types[arg.ty].inner {
                TypeInner::Pointer { base, .. } => type_handle_scalar_kind(module, *base),
                _ => type_handle_scalar_kind(module, arg.ty),
            }
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
        Expression::Compose { ty, .. } => type_handle_scalar_kind(module, *ty),
        Expression::Splat { value, .. } => expr_scalar_kind(module, func, *value),
        Expression::Swizzle { vector, .. } => expr_scalar_kind(module, func, *vector),
        Expression::AccessIndex { base, .. } | Expression::Access { base, .. } => {
            expr_scalar_kind(module, func, *base)
        }
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
        Expression::ZeroValue(ty_h) => type_handle_scalar_kind(module, *ty_h),
        Expression::Math { fun, arg, .. } => match fun {
            MathFunction::Dot
            | MathFunction::Length
            | MathFunction::Distance
            | MathFunction::Determinant => Ok(ScalarKind::Float),
            MathFunction::Cross => Ok(ScalarKind::Float),
            MathFunction::Transpose | MathFunction::Inverse => expr_scalar_kind(module, func, *arg),
            _ => expr_scalar_kind(module, func, *arg),
        },
        Expression::Relational { fun, argument } => match fun {
            RelationalFunction::All | RelationalFunction::Any => {
                expr_scalar_kind(module, func, *argument)
            }
            RelationalFunction::IsNan | RelationalFunction::IsInf => {
                expr_scalar_kind(module, func, *argument)
            }
        },
        _ => Err(LowerError::UnsupportedExpression(format!(
            "cannot infer scalar kind for {:?}",
            func.expressions[expr]
        ))),
    }
}
