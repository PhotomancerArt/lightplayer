//! Type parsing utilities for converting GLSL AST types to our Type enum

use crate::error::{GlslError, source_span_to_location};
use crate::frontend::semantic::types;
use alloc::{boxed::Box, format, vec::Vec};

/// Parse array dimensions from an ArraySpecifier
/// Returns a vector of dimension sizes (outermost-first)
fn parse_array_dimensions(
    array_spec: &glsl::syntax::ArraySpecifier,
    span: Option<glsl::syntax::SourceSpan>,
) -> Result<Vec<usize>, GlslError> {
    use glsl::syntax::ArraySpecifierDimension;

    let mut dimensions = Vec::new();

    // Process dimensions from outermost to innermost
    for dimension in &array_spec.dimensions.0 {
        let size = match dimension {
            ArraySpecifierDimension::ExplicitlySized(expr) => {
                // Extract literal integer constant
                if let glsl::syntax::Expr::IntConst(n, _) = expr.as_ref() {
                    *n as usize
                } else {
                    let mut error = GlslError::new(
                        crate::error::ErrorCode::E0400,
                        "array size must be a compile-time constant integer",
                    );
                    if let Some(s) = span {
                        error = error.with_location(source_span_to_location(&s));
                    }
                    return Err(error);
                }
            }
            ArraySpecifierDimension::Unsized => {
                // Unsized arrays require an initializer to infer the size
                // Since this function doesn't have access to initializer context,
                // we return an error
                let mut error = GlslError::new(
                    crate::error::ErrorCode::E0400,
                    "unsized array requires an initializer",
                );
                if let Some(s) = span {
                    error = error.with_location(source_span_to_location(&s));
                }
                return Err(error);
            }
        };

        // Reject size 0 (explicitly sized arrays cannot have zero size)
        if size == 0 {
            let mut error = GlslError::new(
                crate::error::ErrorCode::E0400,
                "array size must be positive",
            );
            if let Some(s) = span {
                error = error.with_location(source_span_to_location(&s));
            }
            return Err(error);
        }

        dimensions.push(size);
    }

    Ok(dimensions)
}

/// Parse GLSL type specifier into our Type enum
pub fn parse_type_specifier(
    ty: &glsl::syntax::TypeSpecifier,
    span: Option<glsl::syntax::SourceSpan>,
) -> Result<types::Type, GlslError> {
    use glsl::syntax::TypeSpecifierNonArray;

    // Parse base type
    let base_type = match &ty.ty {
        TypeSpecifierNonArray::Void => types::Type::Void,
        TypeSpecifierNonArray::Bool => types::Type::Bool,
        TypeSpecifierNonArray::Int => types::Type::Int,
        TypeSpecifierNonArray::UInt => types::Type::UInt,
        TypeSpecifierNonArray::Float => types::Type::Float,
        TypeSpecifierNonArray::Vec2 => types::Type::Vec2,
        TypeSpecifierNonArray::Vec3 => types::Type::Vec3,
        TypeSpecifierNonArray::Vec4 => types::Type::Vec4,
        TypeSpecifierNonArray::IVec2 => types::Type::IVec2,
        TypeSpecifierNonArray::IVec3 => types::Type::IVec3,
        TypeSpecifierNonArray::IVec4 => types::Type::IVec4,
        TypeSpecifierNonArray::UVec2 => types::Type::UVec2,
        TypeSpecifierNonArray::UVec3 => types::Type::UVec3,
        TypeSpecifierNonArray::UVec4 => types::Type::UVec4,
        TypeSpecifierNonArray::BVec2 => types::Type::BVec2,
        TypeSpecifierNonArray::BVec3 => types::Type::BVec3,
        TypeSpecifierNonArray::BVec4 => types::Type::BVec4,
        TypeSpecifierNonArray::Mat2 => types::Type::Mat2,
        TypeSpecifierNonArray::Mat3 => types::Type::Mat3,
        TypeSpecifierNonArray::Mat4 => types::Type::Mat4,
        _ => {
            let mut error = GlslError::unsupported_type(format!("{:?}", ty.ty));
            if let Some(s) = span {
                error = error.with_location(source_span_to_location(&s));
            }
            return Err(error);
        }
    };

    // Parse array dimensions (outermost-first)
    // Example: float[5][3] -> Array(Box<Array(Box<Float>, 3)>, 5)
    if let Some(array_spec) = &ty.array_specifier {
        let dimensions = parse_array_dimensions(array_spec, span)?;
        let mut current_type = base_type;

        // Process dimensions from outermost to innermost
        for size in dimensions {
            // Wrap current type in Array
            current_type = types::Type::Array(Box::new(current_type), size);
        }

        Ok(current_type)
    } else {
        Ok(base_type)
    }
}

/// Parse return type from fully specified type
pub fn parse_return_type(
    ty: &glsl::syntax::FullySpecifiedType,
    span: Option<glsl::syntax::SourceSpan>,
) -> Result<types::Type, GlslError> {
    parse_type_specifier(&ty.ty, span)
}

/// Apply array specifier to a base type
/// Used when array dimensions are in the declarator (e.g., "int arr[5];")
pub fn apply_array_specifier(
    base_ty: &types::Type,
    array_spec: &glsl::syntax::ArraySpecifier,
    span: Option<glsl::syntax::SourceSpan>,
) -> Result<types::Type, GlslError> {
    let dimensions = parse_array_dimensions(array_spec, span)?;
    let mut current_type = base_ty.clone();

    // Process dimensions from outermost to innermost
    for size in dimensions {
        // Wrap current type in Array
        current_type = types::Type::Array(Box::new(current_type), size);
    }

    Ok(current_type)
}

/// Parse a declaration type by combining a base type with an optional array specifier
/// This is the unified function for parsing types from declarations where array
/// specifiers may be in the declarator rather than the type specifier.
pub fn parse_declaration_type(
    base_ty: &types::Type,
    array_spec: Option<&glsl::syntax::ArraySpecifier>,
    span: Option<glsl::syntax::SourceSpan>,
) -> Result<types::Type, GlslError> {
    if let Some(array_spec) = array_spec {
        apply_array_specifier(base_ty, array_spec, span)
    } else {
        Ok(base_ty.clone())
    }
}

/// Parse the type for a head declarator in an InitDeclaratorList
/// Combines the base type from the list with the array specifier from SingleDeclaration
pub fn parse_head_declarator_type(
    list: &glsl::syntax::InitDeclaratorList,
    name_span: &glsl::syntax::SourceSpan,
) -> Result<types::Type, GlslError> {
    // Get base type from type specifier
    let base_ty = parse_return_type(&list.head.ty, None)?;

    // Combine with array specifier from SingleDeclaration if present
    parse_declaration_type(
        &base_ty,
        list.head.array_specifier.as_ref(),
        Some(name_span.clone()),
    )
}

/// Parse the type for a tail declarator in an InitDeclaratorList
/// Combines the base type from the list with the array specifier from ArrayedIdentifier
pub fn parse_tail_declarator_type(
    base_ty: &types::Type,
    declarator: &glsl::syntax::SingleDeclarationNoType,
) -> Result<types::Type, GlslError> {
    let name_span = declarator.ident.ident.span.clone();

    // Combine with array specifier from ArrayedIdentifier if present
    parse_declaration_type(
        base_ty,
        declarator.ident.array_spec.as_ref(),
        Some(name_span),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use glsl::parser::Parse;
    use glsl::syntax::*;

    fn parse_type_specifier_str(s: &str) -> Result<TypeSpecifier, glsl::parser::ParseError> {
        TypeSpecifier::parse(s)
    }

    #[test]
    fn test_parse_type_specifier_int() {
        let ty = parse_type_specifier_str("int").unwrap();
        let result = parse_type_specifier(&ty, None).unwrap();
        assert_eq!(result, types::Type::Int);
    }

    #[test]
    fn test_parse_type_specifier_float() {
        let ty = parse_type_specifier_str("float").unwrap();
        let result = parse_type_specifier(&ty, None).unwrap();
        assert_eq!(result, types::Type::Float);
    }

    #[test]
    fn test_parse_type_specifier_array_1d() {
        let ty = parse_type_specifier_str("int[5]").unwrap();
        let result = parse_type_specifier(&ty, None).unwrap();
        assert_eq!(result, types::Type::Array(Box::new(types::Type::Int), 5));
    }

    #[test]
    fn test_parse_type_specifier_array_2d() {
        let ty = parse_type_specifier_str("float[3][5]").unwrap();
        let result = parse_type_specifier(&ty, None).unwrap();

        let expected = types::Type::Array(
            Box::new(types::Type::Array(Box::new(types::Type::Float), 3)),
            5,
        );
        assert_eq!(result, expected);
    }

    #[test]
    fn test_apply_array_specifier_single_dimension() {
        let base_ty = types::Type::Int;
        let array_spec = parse_type_specifier_str("int[5]")
            .unwrap()
            .array_specifier
            .unwrap();
        let result = apply_array_specifier(&base_ty, &array_spec, None).unwrap();
        assert_eq!(result, types::Type::Array(Box::new(types::Type::Int), 5));
    }

    #[test]
    fn test_apply_array_specifier_multiple_dimensions() {
        let base_ty = types::Type::Float;
        let array_spec = parse_type_specifier_str("float[3][5]")
            .unwrap()
            .array_specifier
            .unwrap();
        let result = apply_array_specifier(&base_ty, &array_spec, None).unwrap();
        // Dimensions come as [3, 5] and we process left-to-right
        // So we wrap Float with 3 first, then wrap that with 5
        let expected = types::Type::Array(
            Box::new(types::Type::Array(Box::new(types::Type::Float), 3)),
            5,
        );
        assert_eq!(result, expected);
    }

    #[test]
    fn test_parse_declaration_type_without_array() {
        let base_ty = types::Type::Int;
        let result = parse_declaration_type(&base_ty, None, None).unwrap();
        assert_eq!(result, types::Type::Int);
    }

    #[test]
    fn test_parse_declaration_type_with_array() {
        let base_ty = types::Type::Int;
        let array_spec = parse_type_specifier_str("int[5]")
            .unwrap()
            .array_specifier
            .unwrap();
        let result = parse_declaration_type(&base_ty, Some(&array_spec), None).unwrap();
        assert_eq!(result, types::Type::Array(Box::new(types::Type::Int), 5));
    }

    #[test]
    fn test_parse_array_dimensions_zero_size() {
        // Create an array specifier with zero size (should fail)
        let ty = parse_type_specifier_str("int[0]").unwrap();
        let array_spec = ty.array_specifier.unwrap();
        let result = parse_array_dimensions(&array_spec, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("positive"));
    }

    #[test]
    fn test_parse_array_dimensions_unsized() {
        // Create an unsized array specifier (should fail)
        let ty = TypeSpecifier::parse("int[]").unwrap();
        let array_spec = ty.array_specifier.unwrap();
        let result = parse_array_dimensions(&array_spec, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("initializer"));
    }

    #[test]
    fn test_parse_head_declarator_type_with_array() {
        let decl = Declaration::parse("int arr[5];").unwrap();
        if let Declaration::InitDeclaratorList(list) = decl {
            let name_span = list.head.name.as_ref().unwrap().span.clone();
            let result = parse_head_declarator_type(&list, &name_span).unwrap();
            assert_eq!(result, types::Type::Array(Box::new(types::Type::Int), 5));
        } else {
            panic!("Expected InitDeclaratorList");
        }
    }

    #[test]
    fn test_parse_head_declarator_type_without_array() {
        let decl = Declaration::parse("int x;").unwrap();
        if let Declaration::InitDeclaratorList(list) = decl {
            let name_span = list.head.name.as_ref().unwrap().span.clone();
            let result = parse_head_declarator_type(&list, &name_span).unwrap();
            assert_eq!(result, types::Type::Int);
        } else {
            panic!("Expected InitDeclaratorList");
        }
    }

    #[test]
    fn test_parse_tail_declarator_type_with_array() {
        let decl = Declaration::parse("int x, arr[5];").unwrap();
        if let Declaration::InitDeclaratorList(list) = decl {
            let base_ty = parse_return_type(&list.head.ty, None).unwrap();
            let result = parse_tail_declarator_type(&base_ty, &list.tail[0]).unwrap();
            assert_eq!(result, types::Type::Array(Box::new(types::Type::Int), 5));
        } else {
            panic!("Expected InitDeclaratorList");
        }
    }
}
