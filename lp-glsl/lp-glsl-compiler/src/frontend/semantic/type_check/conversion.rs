//! Type promotion and conversion rules for GLSL
//! Implements GLSL spec: variables.adoc:1182-1229

use crate::error::{ErrorCode, GlslError, source_span_to_location};
use crate::frontend::semantic::types::Type;
use glsl::syntax::SourceSpan;

use alloc::format;

/// Promote numeric types (GLSL spec implicit conversion rules)
/// Implements GLSL spec: variables.adoc:1182-1229
pub fn promote_numeric(lhs: &Type, rhs: &Type) -> Type {
    match (lhs, rhs) {
        (Type::Int, Type::Int) => Type::Int,
        (Type::UInt, Type::UInt) => Type::UInt,
        (Type::Float, Type::Float) => Type::Float,
        (Type::Int, Type::Float) | (Type::Float, Type::Int) => Type::Float,
        // int + uint: both can convert to uint/float/double, promote to uint
        (Type::Int, Type::UInt) | (Type::UInt, Type::Int) => Type::UInt,
        // uint + float: promote to float
        (Type::UInt, Type::Float) | (Type::Float, Type::UInt) => Type::Float,
        // int → float implicit conversion per GLSL spec
        _ => Type::Int, // Fallback (shouldn't reach here after validation)
    }
}

/// Check if implicit conversion is allowed (GLSL spec: variables.adoc:1182-1229)
pub fn can_implicitly_convert(from: &Type, to: &Type) -> bool {
    // Exact match always allowed
    if from == to {
        return true;
    }

    // Scalar conversions
    if matches!((from, to), (Type::Int, Type::Float)) {
        return true;
    }
    // Float to int conversion (for constructors: truncates toward zero)
    if matches!((from, to), (Type::Float, Type::Int)) {
        return true;
    }
    // int ↔ uint conversions (bit pattern preserved)
    if matches!(
        (from, to),
        (Type::Int, Type::UInt) | (Type::UInt, Type::Int)
    ) {
        return true;
    }
    // uint ↔ float conversions
    if matches!(
        (from, to),
        (Type::UInt, Type::Float) | (Type::Float, Type::UInt)
    ) {
        return true;
    }
    // Numeric to bool conversions (for constructors: 0/0.0 → false, non-zero → true)
    if matches!(
        (from, to),
        (Type::Int, Type::Bool) | (Type::UInt, Type::Bool) | (Type::Float, Type::Bool)
    ) {
        return true;
    }
    // Bool to numeric conversions (for constructors: false → 0/0.0, true → 1/1.0)
    if matches!(
        (from, to),
        (Type::Bool, Type::Int) | (Type::Bool, Type::UInt) | (Type::Bool, Type::Float)
    ) {
        return true;
    }

    // Matrix conversions: same dimensions, exact type match
    if from.is_matrix() && to.is_matrix() {
        if let (Some((from_rows, from_cols)), Some((to_rows, to_cols))) =
            (from.matrix_dims(), to.matrix_dims())
        {
            // Matrices can only be converted if dimensions match exactly
            return from_rows == to_rows && from_cols == to_cols;
        }
    }

    // Vector conversions: same size, compatible base types
    if let (Some(from_base), Some(to_base), Some(from_count), Some(to_count)) = (
        from.vector_base_type(),
        to.vector_base_type(),
        from.component_count(),
        to.component_count(),
    ) {
        if from_count == to_count {
            return can_implicitly_convert(&from_base, &to_base);
        }
    }

    false
}

/// Validate assignment types
pub fn check_assignment(lhs_ty: &Type, rhs_ty: &Type) -> Result<(), GlslError> {
    check_assignment_with_span(lhs_ty, rhs_ty, None)
}

/// Validate assignment types with optional span for error location
pub fn check_assignment_with_span(
    lhs_ty: &Type,
    rhs_ty: &Type,
    span: Option<SourceSpan>,
) -> Result<(), GlslError> {
    if !can_implicitly_convert(rhs_ty, lhs_ty) {
        let mut error = GlslError::new(ErrorCode::E0102, "type mismatch in assignment")
            .with_note(format!(
                "cannot assign value of type `{rhs_ty:?}` to variable of type `{lhs_ty:?}`"
            ))
            .with_note("help: consider using an explicit type conversion");

        if let Some(span) = span {
            error = error.with_location(source_span_to_location(&span));
        }

        return Err(error);
    }
    Ok(())
}
