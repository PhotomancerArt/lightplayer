//! Generate lpfx_fns.rs code

use crate::lpfx::validate::ParsedLpfxFunction;
use lp_glsl_compiler::frontend::semantic::functions::{
    FunctionSignature, ParamQualifier, Parameter,
};
use lp_glsl_compiler::frontend::semantic::types::Type;
use std::collections::HashMap;

/// Generate the complete lpfx_fns.rs source code
pub fn generate_lpfx_fns(parsed_functions: &[ParsedLpfxFunction]) -> String {
    let mut output = String::new();

    // Header comment
    output.push_str("//! LPFX function registry\n");
    output.push_str("//!\n");
    output.push_str("//! This module contains the array of all LPFX functions.\n");
    output.push_str("//! This file is AUTO-GENERATED. Do not edit manually.\n");
    output.push_str("//!\n");
    output.push_str("//! To regenerate this file, run:\n");
    output.push_str("//!     cargo run --bin lp-builtin-gen --manifest-path lp-glsl/apps/lp-builtin-gen/Cargo.toml\n");
    output.push_str("//!\n");
    output.push_str("//! Or use the build script:\n");
    output.push_str("//!     scripts/build-builtins.sh\n\n");

    // Imports
    output.push_str("use super::lpfx_fn::{LpfxFn, LpfxFnImpl};\n");
    output.push_str("use crate::backend::builtins::registry::BuiltinId;\n");
    output.push_str(
        "use crate::semantic::functions::{FunctionSignature, ParamQualifier, Parameter};\n",
    );
    output.push_str("use crate::semantic::types::Type;\n");
    output.push_str("use alloc::{boxed::Box, string::String, vec, vec::Vec};\n\n");

    // lpfx_fns() function
    output.push_str("/// Registry of all LPFX functions\n");
    output.push_str("///\n");
    output.push_str("/// This is the single source of truth for all LPFX function definitions.\n");
    output.push_str("/// Functions are looked up by name from this array.\n");
    output.push_str("///\n");
    output.push_str("/// Returns a static reference to avoid recreating the Vec on every call.\n");
    output.push_str("pub fn lpfx_fns() -> &'static [LpfxFn] {\n");
    output.push_str("    #[cfg(feature = \"std\")]\n");
    output.push_str("    {\n");
    output.push_str("        static FUNCTIONS: std::sync::OnceLock<&'static [LpfxFn]> = std::sync::OnceLock::new();\n");
    output.push_str("        *FUNCTIONS.get_or_init(|| init_functions())\n");
    output.push_str("    }\n");
    output.push_str("    #[cfg(not(feature = \"std\"))]\n");
    output.push_str("    {\n");
    output.push_str("        // In no_std, use a static initialized on first access\n");
    output.push_str("        // This is safe because the data is immutable after initialization\n");
    output.push_str("        static mut FUNCTIONS: Option<&'static [LpfxFn]> = None;\n");
    output.push_str("        unsafe {\n");
    output.push_str("            let ptr = core::ptr::addr_of_mut!(FUNCTIONS);\n");
    output.push_str("            if (*ptr).is_none() {\n");
    output.push_str("                *ptr = Some(init_functions());\n");
    output.push_str("            }\n");
    output.push_str("            (*ptr).unwrap_unchecked()\n");
    output.push_str("        }\n");
    output.push_str("    }\n");
    output.push_str("}\n\n");

    // init_functions() function
    output.push_str("fn init_functions() -> &'static [LpfxFn] {\n");
    output.push_str("    let vec: Vec<LpfxFn> = vec![\n");

    // Group functions by GLSL name for overload support
    let grouped = group_functions_by_name(parsed_functions);

    // Sort function names for deterministic ordering
    let mut sorted_names: Vec<&String> = grouped.keys().collect();
    sorted_names.sort();

    // Generate LpfxFn structures - one per unique signature
    for name in sorted_names {
        let functions = &grouped[name];
        // Group by unique signature (name + return type + parameters)
        let mut signatures = group_by_signature(functions);

        // Sort signatures for deterministic ordering within each function name group
        // Sort by return type first, then by parameter count, then by parameter types
        signatures.sort_by(|(_, sig_a), (_, sig_b)| {
            // Compare return types
            let ret_cmp =
                format!("{:?}", sig_a.return_type).cmp(&format!("{:?}", sig_b.return_type));
            if ret_cmp != std::cmp::Ordering::Equal {
                return ret_cmp;
            }
            // Compare parameter counts
            let param_count_cmp = sig_a.parameters.len().cmp(&sig_b.parameters.len());
            if param_count_cmp != std::cmp::Ordering::Equal {
                return param_count_cmp;
            }
            // Compare parameter types
            for (param_a, param_b) in sig_a.parameters.iter().zip(sig_b.parameters.iter()) {
                let param_cmp = format!("{:?}{:?}", param_a.ty, param_a.qualifier)
                    .cmp(&format!("{:?}{:?}", param_b.ty, param_b.qualifier));
                if param_cmp != std::cmp::Ordering::Equal {
                    return param_cmp;
                }
            }
            std::cmp::Ordering::Equal
        });

        // Generate one LpfxFn entry per unique signature
        for (signature_funcs, sig) in signatures {
            output.push_str("        LpfxFn {\n");
            output.push_str("            glsl_sig: ");
            output.push_str(&format_function_signature(sig));
            output.push_str(",\n");
            output.push_str("            impls: ");
            output.push_str(&format_lpfx_fn_impl_for_signature(&signature_funcs));
            output.push_str(",\n");
            output.push_str("        },\n");
        }
    }

    output.push_str("    ];\n");
    output.push_str("    Box::leak(vec.into_boxed_slice())\n");
    output.push_str("}\n");

    output
}

/// Group functions by GLSL function name
fn group_functions_by_name(
    parsed_functions: &[ParsedLpfxFunction],
) -> HashMap<String, Vec<&ParsedLpfxFunction>> {
    let mut grouped: HashMap<String, Vec<&ParsedLpfxFunction>> = HashMap::new();

    for func in parsed_functions {
        let glsl_name = func.glsl_sig.name.clone();
        grouped.entry(glsl_name).or_default().push(func);
    }

    grouped
}

/// Format a FunctionSignature as Rust code
fn format_function_signature(sig: &FunctionSignature) -> String {
    let mut output = String::new();
    output.push_str("FunctionSignature {\n");
    output.push_str(&format!(
        "                name: String::from(\"{}\"),\n",
        sig.name
    ));
    output.push_str("                return_type: ");
    output.push_str(&format_type(&sig.return_type));
    output.push_str(",\n");
    output.push_str("                parameters: vec![\n");

    for param in &sig.parameters {
        output.push_str("                    ");
        output.push_str(&format_parameter(param));
        output.push_str(",\n");
    }

    output.push_str("                ],\n");
    output.push_str("            }");
    output
}

/// Format a Parameter as Rust code
fn format_parameter(param: &Parameter) -> String {
    format!(
        "Parameter {{\n                        name: String::from(\"{}\"),\n                        ty: {},\n                        qualifier: {},\n                    }}",
        param.name,
        format_type(&param.ty),
        format_param_qualifier(&param.qualifier)
    )
}

/// Format a Type as Rust code
fn format_type(ty: &Type) -> String {
    match ty {
        Type::Void => "Type::Void".to_string(),
        Type::Bool => "Type::Bool".to_string(),
        Type::Int => "Type::Int".to_string(),
        Type::UInt => "Type::UInt".to_string(),
        Type::Float => "Type::Float".to_string(),
        Type::Vec2 => "Type::Vec2".to_string(),
        Type::Vec3 => "Type::Vec3".to_string(),
        Type::Vec4 => "Type::Vec4".to_string(),
        Type::IVec2 => "Type::IVec2".to_string(),
        Type::IVec3 => "Type::IVec3".to_string(),
        Type::IVec4 => "Type::IVec4".to_string(),
        Type::UVec2 => "Type::UVec2".to_string(),
        Type::UVec3 => "Type::UVec3".to_string(),
        Type::UVec4 => "Type::UVec4".to_string(),
        Type::BVec2 => "Type::BVec2".to_string(),
        Type::BVec3 => "Type::BVec3".to_string(),
        Type::BVec4 => "Type::BVec4".to_string(),
        Type::Mat2 => "Type::Mat2".to_string(),
        Type::Mat3 => "Type::Mat3".to_string(),
        Type::Mat4 => "Type::Mat4".to_string(),
        Type::Sampler2D => "Type::Sampler2D".to_string(),
        Type::Struct(id) => format!("Type::Struct({})", id),
        Type::Array(inner, size) => {
            format!("Type::Array(Box::new({}), {})", format_type(inner), size)
        }
    }
}

/// Format a ParamQualifier as Rust code
fn format_param_qualifier(qualifier: &ParamQualifier) -> String {
    match qualifier {
        ParamQualifier::In => "ParamQualifier::In".to_string(),
        ParamQualifier::Out => "ParamQualifier::Out".to_string(),
        ParamQualifier::InOut => "ParamQualifier::InOut".to_string(),
    }
}

/// Group functions by unique signature (name + return type + parameters)
fn group_by_signature<'a>(
    functions: &'a [&'a ParsedLpfxFunction],
) -> Vec<(Vec<&'a ParsedLpfxFunction>, &'a FunctionSignature)> {
    use std::collections::HashMap;

    let mut by_sig: HashMap<String, Vec<&ParsedLpfxFunction>> = HashMap::new();

    for func in functions {
        let key = signature_key(&func.glsl_sig);
        by_sig.entry(key).or_default().push(func);
    }

    // Convert to vec of (functions, signature) pairs
    by_sig
        .into_values()
        .map(|funcs| {
            let sig = &funcs[0].glsl_sig;
            (funcs, sig)
        })
        .collect()
}

/// Create a signature key for grouping functions (name + return type + parameters)
fn signature_key(sig: &FunctionSignature) -> String {
    // Create a key from function name + return type + parameter types
    let mut key = format!("{}:", sig.name);
    key.push_str(&format!("{:?}", sig.return_type));
    for param in &sig.parameters {
        key.push_str(&format!("{:?}{:?}", param.ty, param.qualifier));
    }
    key
}

/// Format an LpfxFnImpl as Rust code for a specific signature
fn format_lpfx_fn_impl_for_signature(functions: &[&ParsedLpfxFunction]) -> String {
    // Check if any function has a variant (decimal function)
    let has_variant = functions.iter().any(|f| f.attribute.variant.is_some());

    if has_variant {
        // Decimal function - find f32 and q32 variants
        let mut f32_builtin = None;
        let mut q32_builtin = None;

        for f in functions {
            if let Some(v) = f.attribute.variant {
                match v {
                    crate::lpfx::errors::Variant::F32 => {
                        f32_builtin = Some(&f.info.builtin_id_variant);
                    }
                    crate::lpfx::errors::Variant::Q32 => {
                        q32_builtin = Some(&f.info.builtin_id_variant);
                    }
                }
            }
        }

        format!(
            "LpfxFnImpl::Decimal {{\n                float_impl: BuiltinId::{},\n                q32_impl: BuiltinId::{},\n            }}",
            f32_builtin.expect("f32 variant not found"),
            q32_builtin.expect("q32 variant not found")
        )
    } else {
        // Non-decimal function - should have exactly one
        if functions.len() != 1 {
            panic!("Non-decimal function should have exactly one implementation");
        }
        format!(
            "LpfxFnImpl::NonDecimal(BuiltinId::{})",
            functions[0].info.builtin_id_variant
        )
    }
}
