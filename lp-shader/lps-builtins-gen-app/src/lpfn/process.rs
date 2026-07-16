//! Process discovered LPFX functions: parse attributes and GLSL signatures

use crate::discovery::LpfnFunctionInfo;
use crate::lpfn::errors::LpfnCodegenError;
use crate::lpfn::glsl_parse::parse_glsl_signature;
use crate::lpfn::parse::parse_lpfn_attribute;
use crate::lpfn::validate::ParsedLpfnFunction;
use std::fs;
use syn::{Item, ItemFn, parse_file};

/// Process discovered LPFX functions: parse attributes and GLSL signatures
pub fn process_lpfn_functions(
    discovered: &[LpfnFunctionInfo],
) -> Result<Vec<ParsedLpfnFunction>, LpfnCodegenError> {
    let mut parsed = Vec::new();

    for info in discovered {
        // Re-read the file to get the function with its attributes
        let content = fs::read_to_string(&info.file_path).map_err(|e| {
            LpfnCodegenError::AttributeParseError {
                function_name: info.rust_fn_name.clone(),
                file_path: info.file_path.display().to_string(),
                error: format!("Failed to read file: {}", e),
            }
        })?;

        let ast = parse_file(&content).map_err(|e| LpfnCodegenError::AttributeParseError {
            function_name: info.rust_fn_name.clone(),
            file_path: info.file_path.display().to_string(),
            error: format!("Failed to parse file: {}", e),
        })?;

        // Find the function in the AST
        let func = find_function_in_ast(&ast, &info.rust_fn_name).ok_or_else(|| {
            LpfnCodegenError::AttributeParseError {
                function_name: info.rust_fn_name.clone(),
                file_path: info.file_path.display().to_string(),
                error: "Function not found in parsed AST".to_string(),
            }
        })?;

        // Find and parse the #[lpfn_impl] or #[lpfn_impl_macro::lpfn_impl] attribute
        let attr = func
            .attrs
            .iter()
            .find(|a| {
                let path = a.path();
                if path.is_ident("lpfn_impl") {
                    return true;
                }
                // Check if last segment is "lpfn_impl"
                if let Some(last_seg) = path.segments.last() {
                    return last_seg.ident == "lpfn_impl";
                }
                false
            })
            .ok_or_else(|| LpfnCodegenError::MissingAttribute {
                function_name: info.rust_fn_name.clone(),
                file_path: info.file_path.display().to_string(),
            })?;

        let parsed_attr = parse_lpfn_attribute(
            attr,
            &info.rust_fn_name,
            &info.file_path.display().to_string(),
        )?;

        // Parse GLSL signature
        let glsl_sig = parse_glsl_signature(
            &parsed_attr.glsl_signature,
            &info.rust_fn_name,
            &info.file_path.display().to_string(),
        )?;

        parsed.push(ParsedLpfnFunction {
            info: info.clone(),
            attribute: parsed_attr,
            glsl_sig,
            doc_summary: extract_doc_summary(func),
        });
    }

    Ok(parsed)
}

/// First non-empty `///` doc-comment line on the function (used as completion description).
fn extract_doc_summary(func: &ItemFn) -> String {
    for attr in &func.attrs {
        if !attr.path().is_ident("doc") {
            continue;
        }
        if let syn::Meta::NameValue(name_value) = &attr.meta
            && let syn::Expr::Lit(expr_lit) = &name_value.value
            && let syn::Lit::Str(lit_str) = &expr_lit.lit
        {
            let line = lit_str.value().trim().to_string();
            if !line.is_empty() {
                return line;
            }
        }
    }
    String::new()
}

/// Find a function by name in the AST
fn find_function_in_ast<'a>(ast: &'a syn::File, name: &str) -> Option<&'a ItemFn> {
    for item in &ast.items {
        if let Item::Fn(func) = item
            && func.sig.ident == name
        {
            return Some(func);
        }
    }
    None
}
