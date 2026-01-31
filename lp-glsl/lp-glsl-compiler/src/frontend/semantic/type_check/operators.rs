//! Operator type inference for binary and unary operations
//! Implements GLSL spec: operators.adoc:775-855

use crate::error::{ErrorCode, GlslError, source_span_to_location};
use crate::frontend::semantic::types::Type;
use glsl::syntax::{BinaryOp, SourceSpan, UnaryOp};

use super::conversion::promote_numeric;
use super::matrix;

use alloc::format;

/// Infer result type of binary operation (with implicit conversion)
/// Implements GLSL spec: operators.adoc:775-855, operators.adoc:1019-1098 (matrix ops)
pub fn infer_binary_result_type(
    op: &BinaryOp,
    lhs_ty: &Type,
    rhs_ty: &Type,
    span: SourceSpan,
) -> Result<Type, GlslError> {
    use BinaryOp::*;

    match op {
        // Arithmetic operators
        Add | Sub | Mult | Div => {
            // Matrix operations
            if lhs_ty.is_matrix() || rhs_ty.is_matrix() {
                return matrix::infer_matrix_binary_result_type(op, lhs_ty, rhs_ty, span);
            }

            // Vector operations
            if lhs_ty.is_vector() || rhs_ty.is_vector() {
                // Vector + Vector: component-wise, types must match
                if lhs_ty.is_vector() && rhs_ty.is_vector() {
                    if lhs_ty != rhs_ty {
                        return Err(GlslError::new(
                            ErrorCode::E0106,
                            format!(
                                "vector operation requires matching types, got {lhs_ty:?} and {rhs_ty:?}"
                            ),
                        )
                        .with_location(source_span_to_location(&span)));
                    }
                    return Ok(lhs_ty.clone());
                }

                // Vector + Scalar or Scalar + Vector: result is vector type
                if lhs_ty.is_vector() {
                    let vec_base = lhs_ty.vector_base_type().unwrap();
                    if !rhs_ty.is_numeric() || !vec_base.is_numeric() {
                        return Err(GlslError::new(
                            ErrorCode::E0106,
                            format!("cannot use {rhs_ty:?} with {lhs_ty:?}"),
                        )
                        .with_location(source_span_to_location(&span)));
                    }
                    return Ok(lhs_ty.clone());
                }

                if rhs_ty.is_vector() {
                    let vec_base = rhs_ty.vector_base_type().unwrap();
                    if !lhs_ty.is_numeric() || !vec_base.is_numeric() {
                        return Err(GlslError::new(
                            ErrorCode::E0106,
                            format!("cannot use {lhs_ty:?} with {rhs_ty:?}"),
                        )
                        .with_location(source_span_to_location(&span)));
                    }
                    return Ok(rhs_ty.clone());
                }
            }

            // Scalar operations
            if !lhs_ty.is_numeric() || !rhs_ty.is_numeric() {
                return Err(GlslError::new(
                    ErrorCode::E0106,
                    format!("arithmetic operator {op:?} requires numeric operands"),
                )
                .with_location(source_span_to_location(&span))
                .with_note(format!(
                    "left operand has type `{lhs_ty:?}`, right operand has type `{rhs_ty:?}`"
                )));
            }
            // Result type is the promoted type
            Ok(promote_numeric(lhs_ty, rhs_ty))
        }

        // Modulo operator: integer types only (int, uint, ivec2, ivec3, ivec4, uvec2, uvec3, uvec4)
        Mod => {
            // Modulo is only valid for integer types, not floats
            let lhs_is_int = matches!(
                lhs_ty,
                Type::Int
                    | Type::UInt
                    | Type::IVec2
                    | Type::IVec3
                    | Type::IVec4
                    | Type::UVec2
                    | Type::UVec3
                    | Type::UVec4
            );
            let rhs_is_int = matches!(
                rhs_ty,
                Type::Int
                    | Type::UInt
                    | Type::IVec2
                    | Type::IVec3
                    | Type::IVec4
                    | Type::UVec2
                    | Type::UVec3
                    | Type::UVec4
            );

            if !lhs_is_int || !rhs_is_int {
                return Err(GlslError::new(
                    ErrorCode::E0106,
                    format!(
                        "modulo operator requires integer operands, got {lhs_ty:?} and {rhs_ty:?}"
                    ),
                )
                .with_location(source_span_to_location(&span)));
            }

            // Matrix operations (not valid for modulo)
            if lhs_ty.is_matrix() || rhs_ty.is_matrix() {
                return Err(GlslError::new(
                    ErrorCode::E0106,
                    "modulo operator not valid for matrix types",
                )
                .with_location(source_span_to_location(&span)));
            }

            // Vector operations
            if lhs_ty.is_vector() || rhs_ty.is_vector() {
                // Vector % Vector: component-wise, types must match
                if lhs_ty.is_vector() && rhs_ty.is_vector() {
                    if lhs_ty != rhs_ty {
                        return Err(GlslError::new(
                            ErrorCode::E0106,
                            format!(
                                "vector modulo requires matching types, got {lhs_ty:?} and {rhs_ty:?}"
                            ),
                        )
                        .with_location(source_span_to_location(&span)));
                    }
                    return Ok(lhs_ty.clone());
                }

                // Vector % Scalar or Scalar % Vector: result is vector type
                if lhs_ty.is_vector() {
                    let vec_base = lhs_ty.vector_base_type().unwrap();
                    if !matches!(vec_base, Type::Int | Type::UInt)
                        || !matches!(rhs_ty, Type::Int | Type::UInt)
                    {
                        return Err(GlslError::new(
                            ErrorCode::E0106,
                            format!("modulo requires integer types, got {lhs_ty:?} and {rhs_ty:?}"),
                        )
                        .with_location(source_span_to_location(&span)));
                    }
                    return Ok(lhs_ty.clone());
                }

                if rhs_ty.is_vector() {
                    let vec_base = rhs_ty.vector_base_type().unwrap();
                    if !matches!(vec_base, Type::Int | Type::UInt)
                        || !matches!(lhs_ty, Type::Int | Type::UInt)
                    {
                        return Err(GlslError::new(
                            ErrorCode::E0106,
                            format!("modulo requires integer types, got {lhs_ty:?} and {rhs_ty:?}"),
                        )
                        .with_location(source_span_to_location(&span)));
                    }
                    return Ok(rhs_ty.clone());
                }
            }

            // Scalar operations: both must be Int or UInt
            match (lhs_ty, rhs_ty) {
                (Type::Int, Type::Int) => Ok(Type::Int),
                (Type::UInt, Type::UInt) => Ok(Type::UInt),
                (Type::Int, Type::UInt) | (Type::UInt, Type::Int) => {
                    // Mixed int/uint: promote to uint per GLSL spec
                    Ok(Type::UInt)
                }
                _ => {
                    return Err(GlslError::new(
                        ErrorCode::E0106,
                        format!(
                            "modulo operator requires integer operands, got {lhs_ty:?} and {rhs_ty:?}"
                        ),
                    )
                    .with_location(source_span_to_location(&span)));
                }
            }
        }

        // Comparison operators: operands must be compatible, result is bool
        // Note: The == operator does aggregate comparison (all components must match)
        // For component-wise comparison, use the equal() builtin function
        Equal | NonEqual => {
            // Equality operators work on all types (including bool)
            if lhs_ty != rhs_ty {
                return Err(GlslError::new(
                    ErrorCode::E0106,
                    format!("equality operator {op:?} requires matching types"),
                )
                .with_location(source_span_to_location(&span))
                .with_note(format!(
                    "left operand has type `{lhs_ty:?}`, right operand has type `{rhs_ty:?}`"
                )));
            }
            Ok(Type::Bool)
        }
        LT | GT | LTE | GTE => {
            // Relational operators require numeric operands
            if !lhs_ty.is_numeric() || !rhs_ty.is_numeric() {
                return Err(GlslError::new(
                    ErrorCode::E0106,
                    format!("comparison operator {op:?} requires numeric operands"),
                )
                .with_location(source_span_to_location(&span))
                .with_note(format!(
                    "left operand has type `{lhs_ty:?}`, right operand has type `{rhs_ty:?}`"
                )));
            }
            Ok(Type::Bool)
        }

        // Logical operators: must be bool
        And | Or | Xor => {
            if lhs_ty != &Type::Bool || rhs_ty != &Type::Bool {
                return Err(GlslError::new(
                    ErrorCode::E0106,
                    format!("logical operator {op:?} requires bool operands"),
                )
                .with_location(source_span_to_location(&span))
                .with_note(format!(
                    "left operand has type `{lhs_ty:?}`, right operand has type `{rhs_ty:?}`"
                )));
            }
            Ok(Type::Bool)
        }

        _ => Err(GlslError::new(
            ErrorCode::E0112,
            format!("unsupported binary operator: {op:?}"),
        )
        .with_location(source_span_to_location(&span))),
    }
}

/// Infer result type of unary operation
pub fn infer_unary_result_type(
    op: &UnaryOp,
    operand_ty: &Type,
    span: SourceSpan,
) -> Result<Type, GlslError> {
    use UnaryOp::*;

    match op {
        Minus => {
            if !operand_ty.is_numeric() {
                return Err(GlslError::new(
                    ErrorCode::E0112,
                    "unary minus requires numeric operand",
                )
                .with_location(source_span_to_location(&span))
                .with_note(format!("operand has type `{operand_ty:?}`")));
            }
            Ok(operand_ty.clone())
        }

        Not => {
            if operand_ty != &Type::Bool {
                return Err(
                    GlslError::new(ErrorCode::E0112, "logical NOT requires bool operand")
                        .with_location(source_span_to_location(&span))
                        .with_note(format!("operand has type `{operand_ty:?}`")),
                );
            }
            Ok(Type::Bool)
        }

        Inc | Dec => {
            if !operand_ty.is_numeric() {
                return Err(GlslError::new(
                    ErrorCode::E0112,
                    "increment/decrement requires numeric operand (scalar, vector, or matrix)",
                )
                .with_location(source_span_to_location(&span))
                .with_note(format!("operand has type `{operand_ty:?}`")));
            }
            Ok(operand_ty.clone())
        }

        _ => Err(GlslError::new(
            ErrorCode::E0112,
            format!("unsupported unary operator: {op:?}"),
        )
        .with_location(source_span_to_location(&span))),
    }
}

/// Validate condition expression type (must be bool)
pub fn check_condition(cond_ty: &Type) -> Result<(), GlslError> {
    if cond_ty != &Type::Bool {
        return Err(
            GlslError::new(ErrorCode::E0107, "condition must be bool type")
                .with_note(format!("condition has type `{cond_ty:?}`, expected `Bool`")),
        );
    }
    Ok(())
}

/// Infer result type of post-increment operation
/// Implements GLSL spec: operators.adoc:856-869
pub fn infer_postinc_result_type(operand_ty: &Type, span: SourceSpan) -> Result<Type, GlslError> {
    // Post-increment requires numeric operand (int, float, or vector/matrix of these)
    if !operand_ty.is_numeric() {
        return Err(GlslError::new(
            ErrorCode::E0112,
            "post-increment requires numeric operand (scalar, vector, or matrix)",
        )
        .with_location(source_span_to_location(&span))
        .with_note(format!("operand has type `{operand_ty:?}`")));
    }

    // Return same type as operand
    Ok(operand_ty.clone())
}

/// Infer result type of pre-increment operation
/// Implements GLSL spec: operators.adoc:856-869
pub fn infer_preinc_result_type(operand_ty: &Type, span: SourceSpan) -> Result<Type, GlslError> {
    // Pre-increment requires numeric operand (int, float, or vector/matrix of these)
    if !operand_ty.is_numeric() {
        return Err(GlslError::new(
            ErrorCode::E0112,
            "pre-increment requires numeric operand (scalar, vector, or matrix)",
        )
        .with_location(source_span_to_location(&span))
        .with_note(format!("operand has type `{operand_ty:?}`")));
    }

    // Return same type as operand
    Ok(operand_ty.clone())
}

/// Infer result type of pre-decrement operation
/// Implements GLSL spec: operators.adoc:856-869
pub fn infer_predec_result_type(operand_ty: &Type, span: SourceSpan) -> Result<Type, GlslError> {
    // Pre-decrement requires numeric operand (int, float, or vector/matrix of these)
    if !operand_ty.is_numeric() {
        return Err(GlslError::new(
            ErrorCode::E0112,
            "pre-decrement requires numeric operand (scalar, vector, or matrix)",
        )
        .with_location(source_span_to_location(&span))
        .with_note(format!("operand has type `{operand_ty:?}`")));
    }

    // Return same type as operand
    Ok(operand_ty.clone())
}

/// Infer result type of post-decrement operation
/// Implements GLSL spec: operators.adoc:856-869
pub fn infer_postdec_result_type(operand_ty: &Type, span: SourceSpan) -> Result<Type, GlslError> {
    // Post-decrement requires numeric operand (int, float, or vector/matrix of these)
    if !operand_ty.is_numeric() {
        return Err(GlslError::new(
            ErrorCode::E0112,
            "post-decrement requires numeric operand (scalar, vector, or matrix)",
        )
        .with_location(source_span_to_location(&span))
        .with_note(format!("operand has type `{operand_ty:?}`")));
    }

    // Return same type as operand
    Ok(operand_ty.clone())
}
