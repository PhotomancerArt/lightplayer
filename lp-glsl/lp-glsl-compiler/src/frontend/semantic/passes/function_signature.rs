//! Shared utilities for extracting function signatures from AST

use crate::error::GlslError;
use crate::frontend::semantic::functions::{FunctionSignature, ParamQualifier, Parameter};
use crate::frontend::semantic::type_resolver;

use alloc::vec::Vec;

use alloc::string::String;

/// Extract a function signature from a function prototype
pub fn extract_function_signature(
    prototype: &glsl::syntax::FunctionPrototype,
) -> Result<FunctionSignature, GlslError> {
    let name = prototype.name.name.clone();
    // Extract span from function name for error reporting (fallback to type location)
    let type_span = prototype.name.span.clone();
    let return_type = type_resolver::parse_return_type(&prototype.ty, Some(type_span))?;

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

/// Extract a parameter from a function parameter declaration
pub fn extract_parameter(
    param_decl: &glsl::syntax::FunctionParameterDeclaration,
) -> Result<Parameter, GlslError> {
    use glsl::syntax::FunctionParameterDeclaration;

    match param_decl {
        FunctionParameterDeclaration::Named(qualifier, decl) => {
            let param_span = decl.ident.ident.span.clone();
            // Parse base type from TypeSpecifier
            let base_ty = type_resolver::parse_type_specifier(&decl.ty, Some(param_span.clone()))?;
            // Combine with array specifier from declarator if present
            // For "int arr[5]", the [5] is in decl.ident.array_spec
            let ty = type_resolver::parse_declaration_type(
                &base_ty,
                decl.ident.array_spec.as_ref(),
                Some(param_span),
            )?;
            let name = decl.ident.ident.name.clone();

            let param_qualifier = extract_param_qualifier(qualifier);

            Ok(Parameter {
                name,
                ty,
                qualifier: param_qualifier,
            })
        }
        FunctionParameterDeclaration::Unnamed(qualifier, ty) => {
            // Unnamed parameters (allowed in prototypes)
            // For unnamed params, we don't have a good span, so pass None
            let param_ty = type_resolver::parse_type_specifier(ty, None)?;
            let param_qualifier = extract_param_qualifier(qualifier);

            Ok(Parameter {
                name: String::new(), // Empty name for unnamed params
                ty: param_ty,
                qualifier: param_qualifier,
            })
        }
    }
}

/// Extract parameter qualifier from type qualifier
pub fn extract_param_qualifier(qualifier: &Option<glsl::syntax::TypeQualifier>) -> ParamQualifier {
    use glsl::syntax::{StorageQualifier, TypeQualifierSpec};

    if let Some(type_qual) = qualifier {
        for spec in &type_qual.qualifiers.0 {
            if let TypeQualifierSpec::Storage(storage) = spec {
                return match storage {
                    StorageQualifier::Out => ParamQualifier::Out,
                    StorageQualifier::InOut => ParamQualifier::InOut,
                    StorageQualifier::In => ParamQualifier::In,
                    _ => ParamQualifier::In, // Default for other storage qualifiers
                };
            }
        }
    }

    // Default is 'in'
    ParamQualifier::In
}
