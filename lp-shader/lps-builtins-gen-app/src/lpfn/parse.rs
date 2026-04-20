//! Parse #[lpfn_impl] attributes

use crate::lpfn::errors::{LpfnCodegenError, Variant};
use proc_macro2::TokenStream;
use syn::{Attribute, LitStr, parse2};

/// Parsed LPFX implementation attribute
#[derive(Debug, Clone)]
pub struct LpfnAttribute {
    /// Variant type (None for non-decimal, Some for decimal functions)
    pub variant: Option<Variant>,
    /// GLSL signature string
    pub glsl_signature: String,
}

/// Parse a #[lpfn_impl(...)] attribute
pub fn parse_lpfn_attribute(
    attr: &Attribute,
    function_name: &str,
    file_path: &str,
) -> Result<LpfnAttribute, LpfnCodegenError> {
    // Check that this is an lpfn_impl attribute (can be lpfn_impl or lpfn_impl_macro::lpfn_impl)
    let path = attr.path();
    let is_lpfn_impl = path.is_ident("lpfn_impl")
        || path
            .segments
            .last()
            .map(|s| s.ident == "lpfn_impl")
            .unwrap_or(false);
    if !is_lpfn_impl {
        return Err(LpfnCodegenError::AttributeParseError {
            function_name: function_name.to_string(),
            file_path: file_path.to_string(),
            error: "Not an lpfn_impl attribute".to_string(),
        });
    }

    // Parse the attribute arguments as a token stream
    let tokens = attr
        .meta
        .require_list()
        .map_err(|e| LpfnCodegenError::AttributeParseError {
            function_name: function_name.to_string(),
            file_path: file_path.to_string(),
            error: format!("Failed to parse attribute list: {}", e),
        })?;

    parse_lpfn_attribute_tokens(&tokens.tokens, function_name, file_path)
}

/// Parse tokens from #[lpfn_impl(...)]
fn parse_lpfn_attribute_tokens(
    tokens: &TokenStream,
    function_name: &str,
    file_path: &str,
) -> Result<LpfnAttribute, LpfnCodegenError> {
    // Try to parse as: variant, "signature" or just "signature"
    // First, try parsing as an identifier followed by comma and string
    let mut iter = tokens.clone().into_iter();

    // Check first token
    if let Some(proc_macro2::TokenTree::Ident(ident)) = iter.next() {
        let ident_str = ident.to_string();

        // Check if it's f32 or q32
        if ident_str == "f32" || ident_str == "q32" {
            let variant = if ident_str == "f32" {
                Variant::F32
            } else {
                Variant::Q32
            };

            // Expect comma
            if let Some(proc_macro2::TokenTree::Punct(punct)) = iter.next() {
                if punct.as_char() != ',' {
                    return Err(LpfnCodegenError::AttributeParseError {
                        function_name: function_name.to_string(),
                        file_path: file_path.to_string(),
                        error: "Expected comma after variant identifier".to_string(),
                    });
                }
            } else {
                return Err(LpfnCodegenError::AttributeParseError {
                    function_name: function_name.to_string(),
                    file_path: file_path.to_string(),
                    error: "Expected comma after variant identifier".to_string(),
                });
            }

            // Parse remaining tokens as string literal
            let remaining: TokenStream = iter.collect();
            let glsl_signature = parse_string_literal(&remaining, function_name, file_path)?;

            return Ok(LpfnAttribute {
                variant: Some(variant),
                glsl_signature,
            });
        }
    }

    // Not a variant, try parsing entire token stream as string literal
    let glsl_signature = parse_string_literal(tokens, function_name, file_path)?;

    Ok(LpfnAttribute {
        variant: None,
        glsl_signature,
    })
}

/// Parse a string literal from token stream
fn parse_string_literal(
    tokens: &TokenStream,
    function_name: &str,
    file_path: &str,
) -> Result<String, LpfnCodegenError> {
    match parse2::<LitStr>(tokens.clone()) {
        Ok(lit_str) => Ok(lit_str.value()),
        Err(e) => Err(LpfnCodegenError::AttributeParseError {
            function_name: function_name.to_string(),
            file_path: file_path.to_string(),
            error: format!("Failed to parse string literal: {}", e),
        }),
    }
}
