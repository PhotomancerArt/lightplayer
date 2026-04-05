//! Parse `glsl::syntax` function prototypes into [`FunctionSignature`](crate::lpfx::types::FunctionSignature).
//!
//! Function prototype → signature parsing for the builtins codegen (string errors for the binary).

use crate::lpfx::types::{FunctionSignature, ParamQualifier, Parameter, Type};

use std::string::String;
use std::vec::Vec;

fn parse_array_dimensions(array_spec: &glsl::syntax::ArraySpecifier) -> Result<Vec<usize>, String> {
    use glsl::syntax::ArraySpecifierDimension;

    let mut dimensions = Vec::new();

    for dimension in &array_spec.dimensions.0 {
        let size = match dimension {
            ArraySpecifierDimension::ExplicitlySized(expr) => {
                if let glsl::syntax::Expr::IntConst(n, _) = expr.as_ref() {
                    *n as usize
                } else {
                    return Err("array size must be a compile-time constant integer".to_string());
                }
            }
            ArraySpecifierDimension::Unsized => {
                return Err("unsized array requires an initializer".to_string());
            }
        };

        if size == 0 {
            return Err("array size must be positive".to_string());
        }

        dimensions.push(size);
    }

    Ok(dimensions)
}

fn parse_type_specifier(ty: &glsl::syntax::TypeSpecifier) -> Result<Type, String> {
    use glsl::syntax::TypeSpecifierNonArray;

    let base_type = match &ty.ty {
        TypeSpecifierNonArray::Void => Type::Void,
        TypeSpecifierNonArray::Bool => Type::Bool,
        TypeSpecifierNonArray::Int => Type::Int,
        TypeSpecifierNonArray::UInt => Type::UInt,
        TypeSpecifierNonArray::Float => Type::Float,
        TypeSpecifierNonArray::Vec2 => Type::Vec2,
        TypeSpecifierNonArray::Vec3 => Type::Vec3,
        TypeSpecifierNonArray::Vec4 => Type::Vec4,
        TypeSpecifierNonArray::IVec2 => Type::IVec2,
        TypeSpecifierNonArray::IVec3 => Type::IVec3,
        TypeSpecifierNonArray::IVec4 => Type::IVec4,
        TypeSpecifierNonArray::UVec2 => Type::UVec2,
        TypeSpecifierNonArray::UVec3 => Type::UVec3,
        TypeSpecifierNonArray::UVec4 => Type::UVec4,
        TypeSpecifierNonArray::BVec2 => Type::BVec2,
        TypeSpecifierNonArray::BVec3 => Type::BVec3,
        TypeSpecifierNonArray::BVec4 => Type::BVec4,
        TypeSpecifierNonArray::Mat2 => Type::Mat2,
        TypeSpecifierNonArray::Mat3 => Type::Mat3,
        TypeSpecifierNonArray::Mat4 => Type::Mat4,
        _ => {
            return Err(format!(
                "unsupported type in builtin signature: {:?}",
                ty.ty
            ));
        }
    };

    if let Some(array_spec) = &ty.array_specifier {
        let dimensions = parse_array_dimensions(array_spec)?;
        let mut current_type = base_type;
        for size in dimensions {
            current_type = Type::Array(Box::new(current_type), size);
        }
        Ok(current_type)
    } else {
        Ok(base_type)
    }
}

fn parse_return_type(ty: &glsl::syntax::FullySpecifiedType) -> Result<Type, String> {
    parse_type_specifier(&ty.ty)
}

fn apply_array_specifier(
    base_ty: &Type,
    array_spec: &glsl::syntax::ArraySpecifier,
) -> Result<Type, String> {
    let dimensions = parse_array_dimensions(array_spec)?;
    let mut current_type = base_ty.clone();
    for size in dimensions {
        current_type = Type::Array(Box::new(current_type), size);
    }
    Ok(current_type)
}

fn parse_declaration_type(
    base_ty: &Type,
    array_spec: Option<&glsl::syntax::ArraySpecifier>,
) -> Result<Type, String> {
    if let Some(array_spec) = array_spec {
        apply_array_specifier(base_ty, array_spec)
    } else {
        Ok(base_ty.clone())
    }
}

fn extract_param_qualifier(qualifier: &Option<glsl::syntax::TypeQualifier>) -> ParamQualifier {
    use glsl::syntax::{StorageQualifier, TypeQualifierSpec};

    if let Some(type_qual) = qualifier {
        for spec in &type_qual.qualifiers.0 {
            if let TypeQualifierSpec::Storage(storage) = spec {
                return match storage {
                    StorageQualifier::Out => ParamQualifier::Out,
                    StorageQualifier::InOut => ParamQualifier::InOut,
                    StorageQualifier::In => ParamQualifier::In,
                    _ => ParamQualifier::In,
                };
            }
        }
    }

    ParamQualifier::In
}

fn extract_parameter(
    param_decl: &glsl::syntax::FunctionParameterDeclaration,
) -> Result<Parameter, String> {
    use glsl::syntax::FunctionParameterDeclaration;

    match param_decl {
        FunctionParameterDeclaration::Named(qualifier, decl) => {
            let base_ty = parse_type_specifier(&decl.ty)?;
            let ty = parse_declaration_type(&base_ty, decl.ident.array_spec.as_ref())?;
            let name = decl.ident.ident.name.clone();
            let param_qualifier = extract_param_qualifier(qualifier);

            Ok(Parameter {
                name,
                ty,
                qualifier: param_qualifier,
            })
        }
        FunctionParameterDeclaration::Unnamed(qualifier, ty) => {
            let param_ty = parse_type_specifier(ty)?;
            let param_qualifier = extract_param_qualifier(qualifier);

            Ok(Parameter {
                name: String::new(),
                ty: param_ty,
                qualifier: param_qualifier,
            })
        }
    }
}

/// Extract a function signature from a function prototype (LPFX / glsl-parser AST).
pub fn extract_function_signature(
    prototype: &glsl::syntax::FunctionPrototype,
) -> Result<FunctionSignature, String> {
    let name = prototype.name.name.clone();
    let return_type = parse_return_type(&prototype.ty)?;

    let mut parameters = Vec::new();
    for param_decl in &prototype.parameters {
        let param = extract_parameter(param_decl)?;
        parameters.push(param);
    }

    Ok(FunctionSignature {
        name,
        return_type,
        parameters,
    })
}
