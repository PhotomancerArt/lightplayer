//! Constructor validation for vector and matrix types
//! Implements GLSL spec: variables.adoc:72-97

use crate::error::{ErrorCode, GlslError, source_span_to_location};
use crate::frontend::semantic::types::Type;
use glsl::syntax::SourceSpan;

use super::conversion::can_implicitly_convert;

use alloc::format;

/// Check vector constructor arguments and infer result type
pub fn check_vector_constructor(type_name: &str, args: &[Type]) -> Result<Type, GlslError> {
    check_vector_constructor_with_span(type_name, args, None)
}

/// Check vector constructor arguments and infer result type with optional span
pub fn check_vector_constructor_with_span(
    type_name: &str,
    args: &[Type],
    span: Option<SourceSpan>,
) -> Result<Type, GlslError> {
    let result_type = parse_vector_type_name(type_name)?;
    let component_count = result_type.component_count().ok_or_else(|| {
        GlslError::new(
            ErrorCode::E0112,
            format!("`{type_name}` is not a vector type"),
        )
    })?;
    let base_type = result_type.vector_base_type().unwrap();

    // Helper to add location to error if span is available
    let span_clone = span.clone();
    let add_location = move |mut error: GlslError| -> GlslError {
        if let Some(ref s) = span_clone {
            if error.location.is_none() {
                error = error.with_location(source_span_to_location(s));
            }
        }
        error
    };

    // Case 1: Single scalar - broadcast to all components
    if args.len() == 1 && args[0].is_scalar() {
        // Check implicit conversion is allowed
        if !can_implicitly_convert(&args[0], &base_type) {
            return Err(add_location(
                GlslError::new(
                    ErrorCode::E0103,
                    format!("cannot construct `{}` from `{:?}`", type_name, args[0]),
                )
                .with_note("type cannot be implicitly converted"),
            ));
        }
        return Ok(result_type);
    }

    // Case 2: Single vector - type conversion or shortening
    if args.len() == 1 && args[0].is_vector() {
        let src_component_count = args[0].component_count().unwrap();
        // Allow shortening: source components >= target components
        if src_component_count < component_count {
            return Err(add_location(
                GlslError::new(
                    ErrorCode::E0115,
                    format!("cannot construct `{}` from `{:?}`", type_name, args[0]),
                )
                .with_note(format!(
                    "expected at least {component_count} components, found {src_component_count}"
                )),
            ));
        }
        // Check base type conversion is allowed
        let src_base = args[0].vector_base_type().unwrap();
        if !can_implicitly_convert(&src_base, &base_type) {
            return Err(add_location(
                GlslError::new(
                    ErrorCode::E0103,
                    format!("cannot construct `{}` from `{:?}`", type_name, args[0]),
                )
                .with_note("component type cannot be implicitly converted"),
            ));
        }
        return Ok(result_type);
    }

    // Case 3: Multiple arguments - concatenation
    let total_components = count_total_components(args)?;
    if total_components != component_count {
        return Err(add_location(
            GlslError::new(
                ErrorCode::E0115,
                format!("`{type_name}` constructor has wrong number of components"),
            )
            .with_note(format!(
                "expected {component_count} components, found {total_components}"
            )),
        ));
    }

    // Validate each argument can convert to base type
    for arg in args {
        let arg_base = if arg.is_vector() {
            arg.vector_base_type().unwrap()
        } else {
            arg.clone()
        };

        if !can_implicitly_convert(&arg_base, &base_type) {
            return Err(add_location(
                GlslError::new(
                    ErrorCode::E0103,
                    format!("cannot use `{arg:?}` in `{type_name}` constructor"),
                )
                .with_note("component type cannot be implicitly converted"),
            ));
        }
    }

    Ok(result_type)
}

fn parse_vector_type_name(name: &str) -> Result<Type, GlslError> {
    match name {
        "vec2" => Ok(Type::Vec2),
        "vec3" => Ok(Type::Vec3),
        "vec4" => Ok(Type::Vec4),
        "ivec2" => Ok(Type::IVec2),
        "ivec3" => Ok(Type::IVec3),
        "ivec4" => Ok(Type::IVec4),
        "uvec2" => Ok(Type::UVec2),
        "uvec3" => Ok(Type::UVec3),
        "uvec4" => Ok(Type::UVec4),
        "bvec2" => Ok(Type::BVec2),
        "bvec3" => Ok(Type::BVec3),
        "bvec4" => Ok(Type::BVec4),
        _ => Err(GlslError::unsupported_type(name)),
    }
}

fn count_total_components(args: &[Type]) -> Result<usize, GlslError> {
    let mut total = 0;
    for arg in args {
        if let Some(count) = arg.component_count() {
            total += count;
        } else if arg.is_scalar() {
            total += 1;
        } else {
            return Err(GlslError::new(
                ErrorCode::E0112,
                format!("invalid constructor argument: `{arg:?}`"),
            ));
        }
    }
    Ok(total)
}

/// Check if a name is a vector type constructor
pub fn is_vector_type_name(name: &str) -> bool {
    matches!(
        name,
        "vec2"
            | "vec3"
            | "vec4"
            | "ivec2"
            | "ivec3"
            | "ivec4"
            | "uvec2"
            | "uvec3"
            | "uvec4"
            | "bvec2"
            | "bvec3"
            | "bvec4"
    )
}

/// Check if a name is a matrix type constructor
pub fn is_matrix_type_name(name: &str) -> bool {
    matches!(name, "mat2" | "mat3" | "mat4")
}

/// Check if a name is a scalar type constructor
pub fn is_scalar_type_name(name: &str) -> bool {
    matches!(name, "bool" | "int" | "uint" | "float")
}

/// Check scalar constructor arguments and infer result type
pub fn check_scalar_constructor(type_name: &str, args: &[Type]) -> Result<Type, GlslError> {
    check_scalar_constructor_with_span(type_name, args, None)
}

/// Check scalar constructor arguments and infer result type with optional span
pub fn check_scalar_constructor_with_span(
    type_name: &str,
    args: &[Type],
    span: Option<SourceSpan>,
) -> Result<Type, GlslError> {
    // Scalar constructors take exactly one argument
    if args.len() != 1 {
        let mut error = GlslError::new(
            ErrorCode::E0115,
            format!("`{type_name}` constructor requires exactly one argument"),
        );
        if let Some(ref s) = span {
            error = error.with_location(source_span_to_location(s));
        }
        return Err(error);
    }

    // Argument can be scalar or vector (extracts first component)
    // This is allowed per GLSL spec: scalar constructors can take vectors

    // Determine result type
    let result_type = match type_name {
        "bool" => Type::Bool,
        "int" => Type::Int,
        "uint" => Type::UInt,
        "float" => Type::Float,
        _ => {
            let mut error = GlslError::new(
                ErrorCode::E0112,
                format!("`{type_name}` is not a scalar type"),
            );
            if let Some(ref s) = span {
                error = error.with_location(source_span_to_location(s));
            }
            return Err(error);
        }
    };

    Ok(result_type)
}

/// Parse matrix type name to Type
fn parse_matrix_type_name(name: &str) -> Result<Type, GlslError> {
    match name {
        "mat2" => Ok(Type::Mat2),
        "mat3" => Ok(Type::Mat3),
        "mat4" => Ok(Type::Mat4),
        _ => Err(GlslError::unsupported_type(name)),
    }
}

/// Check matrix constructor arguments and infer result type
/// Implements GLSL spec: variables.adoc:72-97
pub fn check_matrix_constructor(type_name: &str, args: &[Type]) -> Result<Type, GlslError> {
    let result_type = parse_matrix_type_name(type_name)?;
    let (rows, cols) = result_type.matrix_dims().ok_or_else(|| {
        GlslError::new(
            ErrorCode::E0112,
            format!("`{type_name}` is not a matrix type"),
        )
    })?;
    let element_count = rows * cols;

    // Case 1: Single scalar - identity matrix (diagonal = scalar, rest = 0.0)
    if args.len() == 1 && args[0].is_scalar() {
        if !can_implicitly_convert(&args[0], &Type::Float) {
            return Err(GlslError::new(
                ErrorCode::E0103,
                format!("cannot construct `{}` from `{:?}`", type_name, args[0]),
            )
            .with_note("matrix constructor requires float scalar for identity"));
        }
        return Ok(result_type);
    }

    // Case 2: Column vectors - one vector per column
    if args.len() == cols {
        // Check all args are vectors of correct type
        let mut all_vectors = true;
        for arg in args {
            if !arg.is_vector() {
                all_vectors = false;
                break;
            }
        }
        if all_vectors {
            for (i, arg) in args.iter().enumerate() {
                if arg.component_count() != Some(rows) {
                    return Err(GlslError::new(
                        ErrorCode::E0115,
                        format!(
                            "matrix column {} has wrong size: expected {} components, got {}",
                            i,
                            rows,
                            arg.component_count().unwrap_or(0)
                        ),
                    ));
                }
                // Check base type can convert to float
                let arg_base = arg.vector_base_type().unwrap();
                if !can_implicitly_convert(&arg_base, &Type::Float) {
                    return Err(GlslError::new(
                        ErrorCode::E0103,
                        format!("matrix column {i} has incompatible base type: `{arg_base:?}`"),
                    ));
                }
            }
            return Ok(result_type);
        }
        // If not all vectors, fall through to check mixed case
    }

    // Case 3: Single matrix - conversion between matrix sizes
    // Check this before counting components, since count_total_components doesn't handle matrices
    if args.len() == 1 && args[0].is_matrix() {
        // Matrix conversion is allowed - smaller matrices are padded with identity,
        // larger matrices are truncated
        return Ok(result_type);
    }

    // Case 4: Mixed scalars - column-major order
    if args.len() == element_count {
        // Check all args are scalars that can convert to float
        let mut all_scalars = true;
        for arg in args {
            if !arg.is_scalar() {
                all_scalars = false;
                break;
            }
        }
        if all_scalars {
            for (i, arg) in args.iter().enumerate() {
                if !can_implicitly_convert(arg, &Type::Float) {
                    return Err(GlslError::new(
                        ErrorCode::E0103,
                        format!("matrix element {i} cannot be converted to float: `{arg:?}`"),
                    ));
                }
            }
            return Ok(result_type);
        }
    }

    // Case 5: Mixed scalars and vectors - column-major order
    // Count total elements from all arguments
    let total_elements = count_total_components(args)?;
    if total_elements == element_count {
        // Validate each argument can convert to float
        for (i, arg) in args.iter().enumerate() {
            let arg_base = if arg.is_vector() {
                arg.vector_base_type().unwrap()
            } else if arg.is_scalar() {
                arg.clone()
            } else {
                return Err(GlslError::new(
                    ErrorCode::E0103,
                    format!(
                        "matrix constructor argument {i} must be a scalar or vector, got `{arg:?}`"
                    ),
                ));
            };

            if !can_implicitly_convert(&arg_base, &Type::Float) {
                return Err(GlslError::new(
                    ErrorCode::E0103,
                    format!(
                        "matrix constructor argument {i} cannot be converted to float: `{arg_base:?}`"
                    ),
                ));
            }
        }
        return Ok(result_type);
    }

    // Wrong number of arguments
    Err(GlslError::new(
        ErrorCode::E0115,
        format!("`{type_name}` constructor has wrong number of arguments")
    )
    .with_note(format!("expected 1 (identity/matrix), {} (columns), {} (scalars), or {} total elements (mixed), found {}",
        cols, element_count, element_count, args.len())))
}
