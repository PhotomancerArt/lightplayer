//! Validate LPFX function definitions

use crate::discovery::LpfxFunctionInfo;
use crate::lpfx::errors::LpfxCodegenError;
use crate::lpfx::errors::Variant;
use crate::lpfx::parse::LpfxAttribute;
use lp_glsl_compiler::frontend::semantic::functions::FunctionSignature;
use std::collections::HashMap;

/// Complete information about a parsed LPFX function
#[derive(Debug, Clone)]
pub struct ParsedLpfxFunction {
    /// Original discovery info
    pub info: LpfxFunctionInfo,
    /// Parsed attribute
    pub attribute: LpfxAttribute,
    /// Parsed GLSL signature
    pub glsl_sig: FunctionSignature,
}

/// Validate all discovered LPFX functions
pub fn validate_lpfx_functions(
    parsed_functions: &[ParsedLpfxFunction],
) -> Result<(), LpfxCodegenError> {
    // Check for missing attributes (should have been caught earlier, but double-check)
    for func in parsed_functions {
        if !func.info.has_lpfx_impl_attr {
            return Err(LpfxCodegenError::MissingAttribute {
                function_name: func.info.rust_fn_name.clone(),
                file_path: func.info.file_path.display().to_string(),
            });
        }
    }

    // Validate decimal pairs
    validate_decimal_pairs(parsed_functions)?;

    // Validate signature consistency
    validate_signature_consistency(parsed_functions)?;

    // Validate overloads have distinct parameter signatures
    validate_overload_distinct_signatures(parsed_functions)?;

    Ok(())
}

/// Validate that all decimal functions have both f32 and q32 variants
fn validate_decimal_pairs(parsed_functions: &[ParsedLpfxFunction]) -> Result<(), LpfxCodegenError> {
    // Group functions by full signature (name + types), not just name
    // This allows overloaded functions (e.g., vec3 and vec4 variants)
    let mut by_signature: HashMap<String, Vec<&ParsedLpfxFunction>> = HashMap::new();

    for func in parsed_functions {
        let key = signature_key(&func.glsl_sig);
        by_signature.entry(key).or_default().push(func);
    }

    // Check each group
    for functions in by_signature.values() {
        // Check if any function has a variant (decimal function)
        let has_variant = functions.iter().any(|f| f.attribute.variant.is_some());

        if has_variant {
            // This is a decimal function - must have both f32 and q32
            let mut has_f32 = false;
            let mut has_q32 = false;
            let mut found_variants = Vec::new();

            for func in functions {
                if let Some(variant) = func.attribute.variant {
                    match variant {
                        Variant::F32 => has_f32 = true,
                        Variant::Q32 => has_q32 = true,
                    }
                    found_variants.push(variant);
                }
            }

            if !has_f32 {
                let glsl_name = functions[0].glsl_sig.name.clone();
                return Err(LpfxCodegenError::MissingPair {
                    function_name: glsl_name,
                    missing_variant: Variant::F32,
                    found_variants,
                });
            }

            if !has_q32 {
                let glsl_name = functions[0].glsl_sig.name.clone();
                return Err(LpfxCodegenError::MissingPair {
                    function_name: glsl_name,
                    missing_variant: Variant::Q32,
                    found_variants,
                });
            }
        }
    }

    Ok(())
}

/// Create a signature key for grouping functions (name + signature, ignoring variant)
fn signature_key(sig: &FunctionSignature) -> String {
    // Create a key from function name + return type + parameter types
    let mut key = format!("{}:", sig.name);
    key.push_str(&format!("{:?}", sig.return_type));
    for param in &sig.parameters {
        key.push_str(&format!("{:?}{:?}", param.ty, param.qualifier));
    }
    key
}

/// Validate that f32 and q32 variants have matching signatures
fn validate_signature_consistency(
    parsed_functions: &[ParsedLpfxFunction],
) -> Result<(), LpfxCodegenError> {
    // Group functions by full signature (name + types), not just name
    // This allows overloaded functions (e.g., vec3 and vec4 variants)
    let mut by_signature: HashMap<String, Vec<&ParsedLpfxFunction>> = HashMap::new();

    for func in parsed_functions {
        let key = signature_key(&func.glsl_sig);
        by_signature.entry(key).or_default().push(func);
    }

    // Check each group for signature consistency
    for functions in by_signature.values() {
        // Find f32 and q32 variants
        let mut f32_func: Option<&ParsedLpfxFunction> = None;
        let mut q32_func: Option<&ParsedLpfxFunction> = None;

        for func in functions {
            if let Some(variant) = func.attribute.variant {
                match variant {
                    Variant::F32 => {
                        if f32_func.is_some() {
                            return Err(LpfxCodegenError::DuplicateFunctionName {
                                function_name: func.glsl_sig.name.clone(),
                                conflicting_files: vec![
                                    f32_func.unwrap().info.file_path.display().to_string(),
                                    func.info.file_path.display().to_string(),
                                ],
                            });
                        }
                        f32_func = Some(func);
                    }
                    Variant::Q32 => {
                        if q32_func.is_some() {
                            return Err(LpfxCodegenError::DuplicateFunctionName {
                                function_name: func.glsl_sig.name.clone(),
                                conflicting_files: vec![
                                    q32_func.unwrap().info.file_path.display().to_string(),
                                    func.info.file_path.display().to_string(),
                                ],
                            });
                        }
                        q32_func = Some(func);
                    }
                }
            }
        }

        // If both exist, compare signatures (should already match since we grouped by signature)
        if let (Some(f32), Some(q32)) = (f32_func, q32_func)
            && !signatures_match(&f32.glsl_sig, &q32.glsl_sig)
        {
            return Err(LpfxCodegenError::SignatureMismatch {
                function_name: f32.glsl_sig.name.clone(),
                f32_signature: format!("{:?}", f32.glsl_sig),
                q32_signature: format!("{:?}", q32.glsl_sig),
            });
        }
    }

    Ok(())
}

/// Check if two function signatures match (ignoring function name)
fn signatures_match(sig1: &FunctionSignature, sig2: &FunctionSignature) -> bool {
    // Compare return types
    if sig1.return_type != sig2.return_type {
        return false;
    }

    // Compare parameter count
    if sig1.parameters.len() != sig2.parameters.len() {
        return false;
    }

    // Compare each parameter (type and qualifier, ignore name)
    for (p1, p2) in sig1.parameters.iter().zip(sig2.parameters.iter()) {
        if p1.ty != p2.ty || p1.qualifier != p2.qualifier {
            return false;
        }
    }

    true
}

/// Validate that overloaded functions have distinct parameter signatures
///
/// For functions with the same GLSL name, ensures that parameter signatures are distinct.
/// Note: f32 and q32 variants of the same signature are allowed (they're implementations, not overloads).
/// This validation checks that we don't have multiple functions with the same parameter signature
/// and the same variant (e.g., two f32 implementations of the same signature).
fn validate_overload_distinct_signatures(
    parsed_functions: &[ParsedLpfxFunction],
) -> Result<(), LpfxCodegenError> {
    // Group functions by GLSL name
    let mut by_name: HashMap<String, Vec<&ParsedLpfxFunction>> = HashMap::new();

    for func in parsed_functions {
        let glsl_name = func.glsl_sig.name.clone();
        by_name.entry(glsl_name).or_default().push(func);
    }

    // Check each group for distinct parameter signatures
    for (name, functions) in &by_name {
        if functions.len() <= 1 {
            continue; // No overloads
        }

        // Group by (parameter signature, variant) to check for duplicates
        // f32 and q32 variants of the same signature are allowed
        let mut by_sig_and_variant: HashMap<
            (String, Option<crate::lpfx::errors::Variant>),
            Vec<&ParsedLpfxFunction>,
        > = HashMap::new();

        for func in functions {
            let param_key = parameter_signature_key(&func.glsl_sig);
            let key = (param_key, func.attribute.variant);
            by_sig_and_variant.entry(key).or_default().push(func);
        }

        // Check for duplicate (parameter signature, variant) combinations
        for funcs in by_sig_and_variant.values() {
            if funcs.len() > 1 {
                let conflicting_files: Vec<String> = funcs
                    .iter()
                    .map(|f| f.info.file_path.display().to_string())
                    .collect();
                return Err(LpfxCodegenError::DuplicateFunctionName {
                    function_name: name.clone(),
                    conflicting_files,
                });
            }
        }
    }

    Ok(())
}

/// Create a key from parameter signature (name + parameters, ignoring return type)
fn parameter_signature_key(sig: &FunctionSignature) -> String {
    let mut key = format!("{}:", sig.name);
    for param in &sig.parameters {
        key.push_str(&format!("{:?}{:?}", param.ty, param.qualifier));
    }
    key
}
