use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use syn::{Item, ItemFn, parse_file};
use walkdir::WalkDir;

mod discovery;
mod lpfx;

use discovery::discover_lpfx_functions;
use lp_glsl_frontend::semantic::types::Type;
use lpfx::errors::Variant;
use lpfx::generate::{generate_lpfx_fns, group_by_signature, group_functions_by_name};
use lpfx::process::process_lpfx_functions;
use lpfx::validate::{ParsedLpfxFunction, validate_lpfx_functions};

#[derive(Debug, Clone)]
struct BuiltinInfo {
    enum_variant: String,
    symbol_name: String,
    function_name: String,
    param_count: usize,
    file_name: String,
    /// Rust function signature types as strings (e.g., "extern \"C\" fn(f32, u32) -> f32")
    rust_signature: String,
    /// Module path relative to builtins/ directory (e.g., "glsl::sin_q32", "lpir::fsqrt_q32", "lpfx::hash")
    module_path: String,
    /// `lpir`, `glsl`, or `lpfx`
    builtin_module: String,
    /// Function name within the module (e.g. `fadd`, `sin`, `fbm2`, `hash_1`)
    builtin_fn_name: String,
    /// `q32`, `f32`, or mode-independent
    builtin_mode: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let workspace_root = find_workspace_root().expect("Failed to find workspace root");
    let builtins_dir = workspace_root
        .join("lp-glsl-builtins")
        .join("src")
        .join("builtins");
    let glsl_dir = builtins_dir.join("glsl");
    let lpir_dir = builtins_dir.join("lpir");
    let lpfx_dir = builtins_dir.join("lpfx");

    let mut builtins =
        discover_builtins(&glsl_dir, &builtins_dir).expect("Failed to discover glsl builtins");
    builtins.extend(
        discover_builtins(&lpir_dir, &builtins_dir).expect("Failed to discover lpir builtins"),
    );
    builtins.extend(
        discover_builtins(&lpfx_dir, &builtins_dir).expect("Failed to discover lpfx builtins"),
    );

    let glsl_map_path = workspace_root
        .join("lp-glsl-builtin-ids")
        .join("src")
        .join("glsl_builtin_mapping.rs");
    generate_glsl_builtin_mapping(&glsl_map_path, &builtins, &lpfx_dir)?;

    // Generate builtin-ids lib.rs (after `glsl_builtin_mapping.rs` for consistent partial runs)
    let builtin_ids_path = workspace_root
        .join("lp-glsl-builtin-ids")
        .join("src")
        .join("lib.rs");
    generate_builtin_ids(&builtin_ids_path, &builtins);

    // Generate registry.rs
    let registry_path = workspace_root
        .join("lp-glsl-cranelift")
        .join("src")
        .join("backend")
        .join("builtins")
        .join("registry.rs");
    generate_registry(&registry_path, &builtins);

    // Generate builtin_refs.rs (RISC-V emu app)
    let builtin_refs_path = workspace_root
        .join("lp-glsl-builtins-emu-app")
        .join("src")
        .join("builtin_refs.rs");
    generate_builtin_refs(&builtin_refs_path, &builtins);

    // Generate builtin_refs.rs (wasm32 cdylib — same refs, different consumer)
    let builtin_refs_wasm_path = workspace_root
        .join("lp-glsl-builtins-wasm")
        .join("src")
        .join("builtin_refs.rs");
    generate_builtin_refs(&builtin_refs_wasm_path, &builtins);

    // Generate glsl/mod.rs and lpir/mod.rs (submodule lists only; lpfx keeps hand-written mod tree)
    let glsl_builtins: Vec<BuiltinInfo> = builtins
        .iter()
        .filter(|b| b.module_path.starts_with("glsl::"))
        .cloned()
        .collect();
    let lpir_builtins: Vec<BuiltinInfo> = builtins
        .iter()
        .filter(|b| b.module_path.starts_with("lpir::"))
        .cloned()
        .collect();
    let glsl_mod_rs_path = workspace_root
        .join("lp-glsl-builtins")
        .join("src")
        .join("builtins")
        .join("glsl")
        .join("mod.rs");
    let lpir_mod_rs_path = workspace_root
        .join("lp-glsl-builtins")
        .join("src")
        .join("builtins")
        .join("lpir")
        .join("mod.rs");
    generate_dir_mod_rs(
        &glsl_mod_rs_path,
        &glsl_builtins,
        "GLSL scalar math builtins (fixed-point Q32).",
    );
    generate_dir_mod_rs(
        &lpir_mod_rs_path,
        &lpir_builtins,
        "LPIR library operations (fixed-point Q32).",
    );

    // Generate testcase mapping in backend/builtins/mapping.rs
    let mapping_rs_path = workspace_root
        .join("lp-glsl-cranelift")
        .join("src")
        .join("backend")
        .join("builtins")
        .join("mapping.rs");
    generate_testcase_mapping(&mapping_rs_path, &builtins);

    // Generate lpfx_fns.rs
    let lpfx_fns_path = workspace_root
        .join("lp-glsl-frontend")
        .join("src")
        .join("semantic")
        .join("lpfx")
        .join("lpfx_fns.rs");
    generate_lpfx_fns_file(&lpfx_fns_path, &lpfx_dir)?;

    let wasm_import_types_path = workspace_root
        .join("lp-glsl-wasm")
        .join("src")
        .join("emit")
        .join("builtin_wasm_import_types.rs");
    generate_wasm_import_types(&wasm_import_types_path, &builtins);

    // Format generated files using cargo fmt
    // Need actual workspace root for cargo fmt, not lp-glsl directory
    let actual_workspace_root = workspace_root
        .parent()
        .ok_or("lp-glsl directory has no parent")?;
    format_generated_files(
        actual_workspace_root,
        &[
            &builtin_ids_path,
            &registry_path,
            &builtin_refs_path,
            &builtin_refs_wasm_path,
            &glsl_mod_rs_path,
            &lpir_mod_rs_path,
            &mapping_rs_path,
            &lpfx_fns_path,
            &glsl_map_path,
            &wasm_import_types_path,
        ],
    );

    println!("Generated all builtin boilerplate files");
    Ok(())
}

/// Generate lpfx_fns.rs file
fn generate_lpfx_fns_file(
    output_path: &Path,
    lpfx_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // Discover LPFX functions
    let discovered = discover_lpfx_functions(lpfx_dir)?;

    // Process: parse attributes and GLSL signatures
    let parsed = process_lpfx_functions(&discovered)?;

    // Validate
    validate_lpfx_functions(&parsed)?;

    // Generate code
    let code = generate_lpfx_fns(&parsed);

    // Write to file
    fs::write(output_path, code)?;

    Ok(())
}

/// GLSL / LPFX name → `BuiltinId` for WASM Q32 codegen (auto-generated).
fn generate_glsl_builtin_mapping(
    path: &Path,
    builtins: &[BuiltinInfo],
    lpfx_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let discovered = discover_lpfx_functions(lpfx_dir)?;
    let parsed = process_lpfx_functions(&discovered)?;
    validate_lpfx_functions(&parsed)?;

    let header = r#"//! GLSL / LPIR / LPFX name → `BuiltinId` for Q32 WASM imports.
//!
//! AUTO-GENERATED by lp-glsl-builtins-gen-app. Do not edit manually.
//!
//! - `glsl_q32_math_builtin_id`: `@glsl::*` scalar imports.
//! - `lpir_q32_builtin_id`: `@lpir::*` library ops (e.g. `sqrt`).
//! - `glsl_lpfx_q32_builtin_id`: `lpfx_*` overloads keyed by parameter types.
//!
//! Regenerate: `cargo run -p lp-glsl-builtins-gen-app` or `scripts/build-builtins.sh`

use super::BuiltinId;

/// Parameter types for LPFX overload resolution (matches GLSL, not flattened WASM).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GlslParamKind {
    Bool,
    Int,
    UInt,
    Float,
    Vec2,
    Vec3,
    Vec4,
    IVec2,
    IVec3,
    IVec4,
    UVec2,
    UVec3,
    UVec4,
    BVec2,
    BVec3,
    BVec4,
}

/// Map `@glsl::*` import name + **AST argument count** to a scalar builtin.
/// Returns `None` for builtins implemented inline in WASM (e.g. `clamp`) or unknown names.
pub fn glsl_q32_math_builtin_id(name: &str, arg_count: usize) -> Option<BuiltinId> {
    match (name, arg_count) {
"#;

    let mut out = String::from(header);

    for builtin in builtins {
        if builtin.builtin_module != "glsl" {
            continue;
        }
        for (glsl, count) in q32_glsl_import_keys(builtin) {
            if glsl.is_empty() {
                continue;
            }
            out.push_str(&format!(
                "        (\"{}\", {}) => Some(BuiltinId::{}),\n",
                glsl, count, builtin.enum_variant
            ));
        }
    }

    out.push_str(
        "        _ => None,\n    }\n}\n\n\
         /// Map `@lpir::*` import name + argument count to a Q32 builtin.\n\
         pub fn lpir_q32_builtin_id(name: &str, arg_count: usize) -> Option<BuiltinId> {\n\
         match (name, arg_count) {\n",
    );

    for builtin in builtins {
        if builtin.builtin_module != "lpir" {
            continue;
        }
        let import = match builtin.builtin_fn_name.as_str() {
            "fsqrt" => "sqrt",
            other => other,
        };
        out.push_str(&format!(
            "        (\"{}\", {}) => Some(BuiltinId::{}),\n",
            import, builtin.param_count, builtin.enum_variant
        ));
    }

    out.push_str(
        "        _ => None,\n    }\n}\n\n\
         /// Map `lpfx_*` name + parameter type list to the Q32 `BuiltinId`.\n\
         pub fn glsl_lpfx_q32_builtin_id(name: &str, params: &[GlslParamKind]) -> Option<BuiltinId> {\n\
         match (name, params) {\n",
    );

    let mut lpfx_arms: Vec<String> = Vec::new();
    let grouped = group_functions_by_name(&parsed);
    let mut sorted_names: Vec<&String> = grouped.keys().collect();
    sorted_names.sort();
    for glsl_name in sorted_names {
        let funcs = &grouped[glsl_name];
        let mut sigs = group_by_signature(funcs);
        sigs.sort_by(|(_, sig_a), (_, sig_b)| {
            let r = format!("{:?}", sig_a.return_type).cmp(&format!("{:?}", sig_b.return_type));
            if r != std::cmp::Ordering::Equal {
                return r;
            }
            let c = sig_a.parameters.len().cmp(&sig_b.parameters.len());
            if c != std::cmp::Ordering::Equal {
                return c;
            }
            for (pa, pb) in sig_a.parameters.iter().zip(sig_b.parameters.iter()) {
                let p = format!("{:?}{:?}", pa.ty, pa.qualifier)
                    .cmp(&format!("{:?}{:?}", pb.ty, pb.qualifier));
                if p != std::cmp::Ordering::Equal {
                    return p;
                }
            }
            std::cmp::Ordering::Equal
        });
        for (signature_funcs, sig) in sigs {
            let Some(variant) = lpfx_q32_builtin_variant(&signature_funcs) else {
                continue;
            };
            let kinds: Vec<String> = sig
                .parameters
                .iter()
                .map(|p| format!("GlslParamKind::{}", type_to_glsl_param_kind_variant(&p.ty)))
                .collect();
            let pat = kinds.join(", ");
            let escaped_name = sig.name.replace('\\', "\\\\").replace('"', "\\\"");
            lpfx_arms.push(format!(
                "        (\"{escaped_name}\", &[{pat}]) => Some(BuiltinId::{variant}),\n",
            ));
        }
    }
    lpfx_arms.sort();
    for arm in lpfx_arms {
        out.push_str(&arm);
    }

    out.push_str("        _ => None,\n    }\n}\n\n");

    let sin_v = builtins
        .iter()
        .find(|b| b.builtin_module == "glsl" && b.builtin_fn_name == "sin")
        .map(|b| b.enum_variant.as_str())
        .expect("glsl sin builtin");
    let atan2_v = builtins
        .iter()
        .find(|b| b.builtin_module == "glsl" && b.builtin_fn_name == "atan2")
        .map(|b| b.enum_variant.as_str())
        .expect("glsl atan2 builtin");
    let fbm_v = builtins
        .iter()
        .find(|b| b.symbol_name.contains("fbm2") && b.symbol_name.ends_with("_q32"))
        .map(|b| b.enum_variant.as_str())
        .expect("lpfx fbm2 q32");
    let sqrt_v = builtins
        .iter()
        .find(|b| b.builtin_module == "lpir" && b.builtin_fn_name == "fsqrt")
        .map(|b| b.enum_variant.as_str())
        .expect("lpir fsqrt builtin");

    out.push_str(&format!(
        "#[cfg(test)]\nmod glsl_builtin_mapping_tests {{\n    use crate::BuiltinId;\n    use super::{{glsl_lpfx_q32_builtin_id, glsl_q32_math_builtin_id, lpir_q32_builtin_id, GlslParamKind}};\n\n    #[test]\n    fn q32_sin() {{\n        assert_eq!(\n            glsl_q32_math_builtin_id(\"sin\", 1),\n            Some(BuiltinId::{sin_v})\n        );\n    }}\n\n    #[test]\n    fn q32_atan_two_args_is_atan2_import() {{\n        assert_eq!(\n            glsl_q32_math_builtin_id(\"atan\", 2),\n            Some(BuiltinId::{atan2_v})\n        );\n    }}\n\n    #[test]\n    fn lpir_sqrt() {{\n        assert_eq!(lpir_q32_builtin_id(\"sqrt\", 1), Some(BuiltinId::{sqrt_v}));\n    }}\n\n    #[test]\n    fn lpfx_fbm_vec2() {{\n        assert_eq!(\n            glsl_lpfx_q32_builtin_id(\n                \"lpfx_fbm\",\n                &[GlslParamKind::Vec2, GlslParamKind::Int, GlslParamKind::UInt],\n            ),\n            Some(BuiltinId::{fbm_v})\n        );\n    }}\n}}\n",
        sin_v = sin_v,
        atan2_v = atan2_v,
        sqrt_v = sqrt_v,
        fbm_v = fbm_v
    ));

    fs::write(path, out)?;
    Ok(())
}

/// `@glsl::*` import keys for Naga lowering and WASM (`name`, arg count).
fn q32_glsl_import_keys(builtin: &BuiltinInfo) -> Vec<(String, usize)> {
    if builtin.builtin_module != "glsl" {
        return Vec::new();
    }
    let s = builtin.builtin_fn_name.as_str();
    match s {
        "atan2" if builtin.param_count == 2 => {
            vec![("atan".to_string(), 2), ("atan2".to_string(), 2)]
        }
        s => vec![(s.to_string(), builtin.param_count)],
    }
}

fn type_to_glsl_param_kind_variant(ty: &Type) -> &'static str {
    match ty {
        Type::Bool => "Bool",
        Type::Int => "Int",
        Type::UInt => "UInt",
        Type::Float => "Float",
        Type::Vec2 => "Vec2",
        Type::Vec3 => "Vec3",
        Type::Vec4 => "Vec4",
        Type::IVec2 => "IVec2",
        Type::IVec3 => "IVec3",
        Type::IVec4 => "IVec4",
        Type::UVec2 => "UVec2",
        Type::UVec3 => "UVec3",
        Type::UVec4 => "UVec4",
        Type::BVec2 => "BVec2",
        Type::BVec3 => "BVec3",
        Type::BVec4 => "BVec4",
        _ => panic!("LPFX param type not supported in GlslParamKind: {:?}", ty),
    }
}

fn lpfx_q32_builtin_variant(funcs: &[&ParsedLpfxFunction]) -> Option<String> {
    let has_decimal = funcs.iter().any(|f| f.attribute.variant.is_some());
    if has_decimal {
        funcs
            .iter()
            .find(|f| f.attribute.variant == Some(Variant::Q32))
            .map(|f| f.info.builtin_id_variant.clone())
    } else if funcs.len() == 1 {
        Some(funcs[0].info.builtin_id_variant.clone())
    } else {
        None
    }
}

fn find_workspace_root() -> Result<PathBuf, Box<dyn std::error::Error>> {
    // Find the actual workspace root (where Cargo.toml with [workspace] is)
    let mut current = std::env::current_dir()?;
    loop {
        let cargo_toml = current.join("Cargo.toml");
        if cargo_toml.exists() {
            let content = std::fs::read_to_string(&cargo_toml)?;
            if content.contains("[workspace]") {
                // Return lp-glsl directory within the workspace
                let lp_glsl_dir = current.join("lp-glsl");
                if lp_glsl_dir.exists() && lp_glsl_dir.is_dir() {
                    return Ok(lp_glsl_dir);
                }
                // Fallback: if we're already in lp-glsl or a subdirectory, use current directory
                let mut check = std::env::current_dir()?;
                loop {
                    if check.join("lp-glsl-builtins").exists()
                        || check.join("lp-glsl-cranelift").exists()
                    {
                        return Ok(check);
                    }
                    if !check.pop() {
                        break;
                    }
                }
            }
        }
        if !current.pop() {
            return Err("Could not find workspace root".into());
        }
    }
}

fn discover_builtins(
    dir: &Path,
    base_dir: &Path,
) -> Result<Vec<BuiltinInfo>, Box<dyn std::error::Error>> {
    // Discover all functions from files using extract_builtin
    let mut builtins: Vec<BuiltinInfo> = Vec::new();

    for entry in WalkDir::new(dir) {
        let entry = entry?;
        let path = entry.path();

        if path.extension() != Some(std::ffi::OsStr::new("rs")) {
            continue;
        }

        let file_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("Invalid file name")?;

        // Skip mod.rs and test_helpers.rs
        if file_name == "mod" || file_name == "test_helpers" {
            continue;
        }

        // Compute module path relative to base_dir (builtins/)
        // This includes the full directory structure: "glsl::sin_q32", "lpfx::hash", ...
        let relative_path = path
            .strip_prefix(base_dir)
            .map_err(|_| "Failed to compute relative path")?;

        let module_path = if let Some(parent) = relative_path.parent() {
            let mut components: Vec<&str> = parent.iter().filter_map(|c| c.to_str()).collect();

            // Add file_name as the final component
            components.push(file_name);

            if components.is_empty() {
                // File is directly in builtins/ (shouldn't happen, but handle it)
                file_name.to_string()
            } else {
                // Join all components with ::
                components.join("::")
            }
        } else {
            // No parent (file is directly in base_dir)
            file_name.to_string()
        };

        let content = fs::read_to_string(path)?;
        let ast = parse_file(&content)?;

        for item in ast.items {
            if let Item::Fn(func) = item
                && let Some(builtin_info) = extract_builtin(&func, file_name, &module_path)
            {
                // Skip if already added
                if !builtins
                    .iter()
                    .any(|b| b.function_name == builtin_info.function_name)
                {
                    builtins.push(builtin_info);
                }
            }
        }
    }

    // Sort by symbol name for consistent output
    builtins.sort_by(|a, b| a.symbol_name.cmp(&b.symbol_name));

    Ok(builtins)
}

#[allow(dead_code)]
fn extract_builtin(func: &ItemFn, file_name: &str, module_path: &str) -> Option<BuiltinInfo> {
    // Check for #[unsafe(no_mangle)] attribute
    let has_no_mangle = func.attrs.iter().any(|attr| attr.path().is_ident("unsafe"));

    if !has_no_mangle {
        return None;
    }

    let func_name = func.sig.ident.to_string();

    // `__lp_<module>_<fn>_<mode>` or `__lp_<module>_<fn>` (no float mode)
    if !func_name.starts_with("__lp_") {
        return None;
    }

    let after_lp = func_name.strip_prefix("__lp_")?;
    let (builtin_module, rest) = if after_lp.starts_with("lpir_") {
        ("lpir", &after_lp[5..])
    } else if after_lp.starts_with("glsl_") {
        ("glsl", &after_lp[5..])
    } else if after_lp.starts_with("lpfx_") {
        ("lpfx", &after_lp[5..])
    } else {
        return None;
    };

    let (fn_body, builtin_mode) = if rest.ends_with("_q32") {
        (&rest[..rest.len() - 4], Some("q32".to_string()))
    } else if rest.ends_with("_f32") {
        (&rest[..rest.len() - 4], Some("f32".to_string()))
    } else {
        (rest, None)
    };
    let builtin_fn_name = fn_body.to_string();
    let builtin_module = builtin_module.to_string();

    let symbol_name = func_name.clone();

    // Derive enum variant: strip `__`, split on `_`, PascalCase each segment.
    // e.g. `__lp_glsl_sin_q32` -> LpGlslSinQ32; `__lp_lpfx_hash_1` -> LpLpfxHash1
    let name_without_prefix = func_name.strip_prefix("__").unwrap();
    let enum_variant = name_without_prefix
        .split('_')
        .map(|s| {
            let mut chars = s.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<String>();

    let param_count = func.sig.inputs.len();
    let rust_signature = format_rust_function_signature(func);

    Some(BuiltinInfo {
        enum_variant,
        symbol_name,
        function_name: func_name,
        param_count,
        file_name: file_name.to_string(),
        rust_signature,
        module_path: module_path.to_string(),
        builtin_module,
        builtin_fn_name,
        builtin_mode,
    })
}

/// Format a Rust function signature as a type string
fn format_rust_function_signature(func: &ItemFn) -> String {
    use quote::ToTokens;

    let mut sig = String::from("extern \"C\" fn(");

    // Format parameters
    let mut params = Vec::new();
    for input in &func.sig.inputs {
        if let syn::FnArg::Typed(pat_type) = input {
            let ty_str = pat_type.ty.to_token_stream().to_string();
            // Clean up the string (remove extra spaces, but preserve spaces in pointer types)
            // Fix pointer types: *mut T and *const T need space between mut/const and T
            let ty_str = ty_str.replace(" ", "");
            // Fix *mutT -> *mut T and *constT -> *const T
            let ty_str = ty_str.replace("*mut", "*mut ");
            let ty_str = ty_str.replace("*const", "*const ");
            params.push(ty_str);
        }
    }
    sig.push_str(&params.join(", "));
    sig.push_str(") -> ");

    // Format return type
    match &func.sig.output {
        syn::ReturnType::Default => sig.push_str("()"),
        syn::ReturnType::Type(_, ty) => {
            let ty_str = ty.to_token_stream().to_string();
            let ty_str = ty_str.replace(" ", "");
            // Fix pointer types in return type too
            let ty_str = ty_str.replace("*mut", "*mut ");
            let ty_str = ty_str.replace("*const", "*const ");
            sig.push_str(&ty_str);
        }
    }

    sig
}

#[allow(dead_code)]
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

/// Map `extern "C"` rust signature strings to wasm32 `wasm_encoder::ValType` lists (for imports).
fn rust_signature_to_wasm_valtype_vecs(rust_sig: &str) -> (Vec<&'static str>, Vec<&'static str>) {
    let rest = rust_sig
        .split("fn(")
        .nth(1)
        .unwrap_or_else(|| panic!("invalid rust_signature (no fn(: {:?}", rust_sig));
    let (params_str, ret_str) = rest
        .split_once(") -> ")
        .unwrap_or_else(|| panic!("invalid rust_signature (no ) -> {:?}", rust_sig));

    let param_tokens: Vec<&str> = if params_str.is_empty() {
        Vec::new()
    } else {
        params_str
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect()
    };

    let map_param = |p: &str| -> &'static str {
        let p = p.trim();
        if p.contains('*') {
            return "ValType::I32";
        }
        if p == "f32" {
            return "ValType::F32";
        }
        "ValType::I32"
    };

    let params: Vec<&'static str> = param_tokens.iter().map(|p| map_param(p)).collect();

    let ret = ret_str.trim();
    let results: Vec<&'static str> = if ret == "()" {
        Vec::new()
    } else {
        vec![map_param(ret)]
    };

    (params, results)
}

fn generate_wasm_import_types(path: &Path, builtins: &[BuiltinInfo]) {
    let header = r#"//! WASM valtypes for each `BuiltinId` import (wasm32, `extern "C"` layout).
//!
//! AUTO-GENERATED by lp-glsl-builtins-gen-app. Do not edit manually.
//!
//! Regenerate: `cargo run -p lp-glsl-builtins-gen-app` or `scripts/build-builtins.sh`

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;
use lp_glsl_builtin_ids::BuiltinId;
use wasm_encoder::ValType;

/// Parameter and result WASM types for the imported builtin function.
pub(super) fn wasm_import_val_types(builtin: BuiltinId) -> (Vec<ValType>, Vec<ValType>) {
    match builtin {
"#;

    let mut output = String::from(header);
    if builtins.is_empty() {
        output.push_str("        BuiltinId::_Placeholder => (Vec::new(), Vec::new()),\n");
    } else {
        for builtin in builtins {
            let (params, results) = rust_signature_to_wasm_valtype_vecs(&builtin.rust_signature);
            let p_vec = if params.is_empty() {
                "Vec::new()".to_string()
            } else {
                format!("vec![{}]", params.join(", "))
            };
            let r_vec = if results.is_empty() {
                "Vec::new()".to_string()
            } else {
                format!("vec![{}]", results.join(", "))
            };
            output.push_str(&format!(
                "        BuiltinId::{} => ({}, {}),\n",
                builtin.enum_variant, p_vec, r_vec
            ));
        }
    }
    output.push_str("    }\n}\n");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create wasm import types parent dir");
    }
    fs::write(path, output).expect("Failed to write builtin_wasm_import_types.rs");
}

fn generate_builtin_ids(path: &Path, builtins: &[BuiltinInfo]) {
    let header = r#"//! Builtin function IDs for lp-glsl.
//!
//! This file is AUTO-GENERATED by lp-glsl-builtins-gen-app. Do not edit manually.
//!
//! To regenerate: `cargo run -p lp-glsl-builtins-gen-app`

#![no_std]

/// Enum identifying builtin functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinId {
"#;

    let mut output = String::from(header);
    if builtins.is_empty() {
        output.push_str("    #[allow(dead_code)]\n");
        output.push_str("    _Placeholder,\n");
    } else {
        for builtin in builtins {
            output.push_str(&format!("    {},\n", builtin.enum_variant));
        }
    }
    output.push_str("}\n\n");

    output.push_str("impl BuiltinId {\n");
    output.push_str("    pub fn name(&self) -> &'static str {\n");
    output.push_str("        match self {\n");
    if builtins.is_empty() {
        output.push_str("            BuiltinId::_Placeholder => \"\",\n");
    } else {
        for builtin in builtins {
            output.push_str(&format!(
                "            BuiltinId::{} => \"{}\",\n",
                builtin.enum_variant, builtin.symbol_name
            ));
        }
    }
    output.push_str("        }\n");
    output.push_str("    }\n\n");

    output.push_str("    pub fn builtin_id_from_name(name: &str) -> Option<BuiltinId> {\n");
    output.push_str("        match name {\n");
    if builtins.is_empty() {
        output.push_str("            _ => None,\n");
    } else {
        for builtin in builtins {
            output.push_str(&format!(
                "            \"{}\" => Some(BuiltinId::{}),\n",
                builtin.symbol_name, builtin.enum_variant
            ));
        }
        output.push_str("            _ => None,\n");
    }
    output.push_str("        }\n");
    output.push_str("    }\n\n");

    output.push_str("    pub fn all() -> &'static [BuiltinId] {\n");
    output.push_str("        &[\n");
    if builtins.is_empty() {
        output.push_str("            BuiltinId::_Placeholder,\n");
    } else {
        for builtin in builtins {
            output.push_str(&format!(
                "            BuiltinId::{},\n",
                builtin.enum_variant
            ));
        }
    }
    output.push_str("        ]\n");
    output.push_str("    }\n\n");

    output.push_str("    pub fn module(&self) -> Module {\n");
    output.push_str("        match self {\n");
    if builtins.is_empty() {
        output.push_str("            BuiltinId::_Placeholder => Module::Glsl,\n");
    } else {
        for builtin in builtins {
            let m = match builtin.builtin_module.as_str() {
                "lpir" => "Module::Lpir",
                "glsl" => "Module::Glsl",
                "lpfx" => "Module::Lpfx",
                other => panic!("unknown builtin_module: {other}"),
            };
            output.push_str(&format!(
                "            BuiltinId::{} => {},\n",
                builtin.enum_variant, m
            ));
        }
    }
    output.push_str("        }\n");
    output.push_str("    }\n\n");

    output.push_str("    pub fn fn_name(&self) -> &'static str {\n");
    output.push_str("        match self {\n");
    if builtins.is_empty() {
        output.push_str("            BuiltinId::_Placeholder => \"\",\n");
    } else {
        for builtin in builtins {
            output.push_str(&format!(
                "            BuiltinId::{} => \"{}\",\n",
                builtin.enum_variant, builtin.builtin_fn_name
            ));
        }
    }
    output.push_str("        }\n");
    output.push_str("    }\n\n");

    output.push_str("    pub fn mode(&self) -> Option<Mode> {\n");
    output.push_str("        match self {\n");
    if builtins.is_empty() {
        output.push_str("            BuiltinId::_Placeholder => None,\n");
    } else {
        for builtin in builtins {
            let arm = match builtin.builtin_mode.as_deref() {
                Some("q32") => "Some(Mode::Q32)",
                Some("f32") => "Some(Mode::F32)",
                None => "None",
                Some(x) => panic!("unknown mode: {x}"),
            };
            output.push_str(&format!(
                "            BuiltinId::{} => {},\n",
                builtin.enum_variant, arm
            ));
        }
    }
    output.push_str("        }\n");
    output.push_str("    }\n");
    output.push_str("}\n\n");

    output.push_str("/// Builtin module for imports and linking.\n");
    output.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]\n");
    output.push_str("pub enum Module {\n");
    output.push_str("    Lpir,\n");
    output.push_str("    Glsl,\n");
    output.push_str("    Lpfx,\n");
    output.push_str("}\n\n");
    output.push_str("/// Float ABI for mode-specific builtins.\n");
    output.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]\n");
    output.push_str("pub enum Mode {\n");
    output.push_str("    Q32,\n");
    output.push_str("    F32,\n");
    output.push_str("}\n\n");

    output.push_str("mod glsl_builtin_mapping;\n\n");
    output.push_str("pub use glsl_builtin_mapping::glsl_lpfx_q32_builtin_id;\n");
    output.push_str("pub use glsl_builtin_mapping::glsl_q32_math_builtin_id;\n");
    output.push_str("pub use glsl_builtin_mapping::lpir_q32_builtin_id;\n");
    output.push_str("pub use glsl_builtin_mapping::GlslParamKind;\n");

    fs::write(path, output).expect("Failed to write builtin-ids lib.rs");
}

fn generate_registry(path: &Path, builtins: &[BuiltinInfo]) {
    let header = r#"//! This file is AUTO-GENERATED. Do not edit manually.
//!
//! To regenerate this file, run:
//!     cargo run --bin lp-glsl-builtins-gen-app --manifest-path lp-glsl/lp-glsl-builtins-gen-app/Cargo.toml
//!
//! Or use the build script:
//!     scripts/build-builtins.sh

"#;

    let mut output = String::from(header);
    output.push_str("//! Builtin function registry implementation.\n");
    output.push_str("//!\n");
    output
        .push_str("//! Provides enum-based registry for builtin functions with support for both\n");
    output.push_str("//! JIT (function pointer) and emulator (ELF symbol) linking.\n");
    output.push('\n');

    output.push_str("pub use lp_glsl_builtin_ids::BuiltinId;\n\n");
    output.push_str("use crate::error::{ErrorCode, GlslError};\n");
    output.push_str("use cranelift_codegen::ir::{AbiParam, Signature, types};\n");
    output.push_str("use cranelift_codegen::isa::CallConv;\n");
    output.push_str("use cranelift_module::{Linkage, Module};\n\n");
    output.push_str("#[cfg(not(feature = \"std\"))]\n");
    output.push_str("use alloc::format;\n\n");

    // Extension trait for format() - cannot add inherent methods to foreign type
    output.push_str(
        "/// Format affinity for builtins (Cranelift-specific, format-aware declaration).\n",
    );
    output.push_str("trait BuiltinIdFormat {\n");
    output.push_str("    fn format(&self) -> Option<crate::FloatMode>;\n");
    output.push_str("}\n\n");
    output.push_str("impl BuiltinIdFormat for BuiltinId {\n");
    output.push_str("    fn format(&self) -> Option<crate::FloatMode> {\n");
    output.push_str("        match self {\n");
    if builtins.is_empty() {
        output.push_str("            BuiltinId::_Placeholder => None,\n");
    } else {
        for builtin in builtins {
            let fmt = if builtin.enum_variant.ends_with("Q32") {
                "Some(crate::FloatMode::Q32)"
            } else if builtin.enum_variant.ends_with("F32") {
                "Some(crate::FloatMode::F32)"
            } else {
                "None"
            };
            output.push_str(&format!(
                "            BuiltinId::{} => {},\n",
                builtin.enum_variant, fmt
            ));
        }
    }
    output.push_str("        }\n");
    output.push_str("    }\n");
    output.push_str("}\n\n");

    // Generate signature_for_builtin() - free function (cannot add inherent methods to foreign type)
    output.push_str("/// Get the Cranelift signature for this builtin function.\n");
    output.push_str("///\n");
    output.push_str("/// `pointer_type` is the native pointer type for the target architecture.\n");
    output.push_str("/// For RISC-V 32-bit, this should be `types::I32`.\n");
    output.push_str(
        "/// For 64-bit architectures (like Apple Silicon), this should be `types::I64`.\n",
    );
    output.push_str("pub fn signature_for_builtin(builtin: BuiltinId, pointer_type: types::Type) -> Signature {\n");
    output.push_str("    let mut sig = Signature::new(CallConv::SystemV);\n");
    output.push_str("    match builtin {\n");

    if builtins.is_empty() {
        output.push_str("            BuiltinId::_Placeholder => {\n");
        output.push_str("                // Placeholder - no builtins defined\n");
        output.push_str("            }\n");
    } else {
        // Detect StructReturn functions (void return + pointer first param)
        let uses_struct_return = |b: &&BuiltinInfo| -> bool {
            b.rust_signature.contains("-> ()") && b.rust_signature.contains("*mut ")
        };

        // Detect functions with out parameters that return a value (not void)
        // These have *mut in params but return i32 (or other non-void type)
        let uses_out_param = |b: &&BuiltinInfo| -> bool {
            // Check for functions that return a value (not void) and have *mut pointer params
            let has_non_void_return = !b.rust_signature.contains("-> ()");
            // Check for *mut (with or without space) - signatures are normalized to *mut
            let has_mut_ptr =
                b.rust_signature.contains("*mut") || b.function_name.contains("psrdnoise");
            has_non_void_return && has_mut_ptr
        };

        // Separate StructReturn functions, out-param functions, and regular functions
        let (struct_return_builtins, rest): (Vec<_>, Vec<_>) =
            builtins.iter().partition(|b| uses_struct_return(b));
        let (mut out_param_builtins, regular_builtins): (Vec<_>, Vec<_>) =
            rest.iter().partition(|b| uses_out_param(b));

        // Manually add psrdnoise functions if they weren't detected (workaround for detection issue)
        for builtin in builtins.iter() {
            if builtin.function_name.contains("psrdnoise") {
                let already_added = out_param_builtins
                    .iter()
                    .any(|b: &&BuiltinInfo| b.function_name == builtin.function_name);
                if !already_added {
                    out_param_builtins.push(builtin);
                }
            }
        }

        // Helper to count i32/f32 parameters before the pointer in a signature
        // Both i32 and f32 map to types::I32 in Cranelift signatures
        let count_i32_before_pointer = |sig: &str| -> usize {
            // Extract the parameter list: "extern \"C\" fn(i32, i32, *mut i32, u32) -> i32"
            // We want to find the part between fn( and ) ->
            if let Some(start) = sig.find("fn(")
                && let Some(end) = sig.find(") ->")
            {
                let params_str = &sig[start + 3..end];
                let params: Vec<&str> = params_str.split(',').map(|s| s.trim()).collect();
                // Count params that are "i32" or "f32" before we hit "*mut"
                let mut count = 0;
                for param in params {
                    if param.contains("*mut") {
                        break;
                    }
                    // Check if param is "i32" or "f32" (exact match or starts with "i32"/"f32" followed by space/end)
                    if param == "i32"
                        || param == "f32"
                        || (param.starts_with("i32")
                            && (param.len() == 3 || param.chars().nth(3) == Some(' ')))
                        || (param.starts_with("f32")
                            && (param.len() == 3 || param.chars().nth(3) == Some(' ')))
                    {
                        count += 1;
                    }
                }
                return count;
            }
            0
        };

        // Group out-param functions by number of i32 params before pointer
        let mut out_param_groups: std::collections::HashMap<usize, Vec<_>> =
            std::collections::HashMap::new();
        for builtin in &out_param_builtins {
            let i32_count = count_i32_before_pointer(&builtin.rust_signature);
            out_param_groups
                .entry(i32_count)
                .or_insert_with(Vec::new)
                .push(builtin);
        }

        // Generate out-param signatures (functions with pointer params that return a value)
        for (i32_count, group) in out_param_groups.iter() {
            if group.is_empty() {
                continue;
            }
            output.push_str("            ");
            for (i, builtin) in group.iter().enumerate() {
                if i > 0 {
                    output.push_str(" | ");
                }
                output.push_str(&format!("BuiltinId::{}", builtin.enum_variant));
            }
            output.push_str(" => {\n");
            output.push_str(&format!(
                "                // Out parameter function: ({} i32 params, pointer_type) -> i32\n",
                i32_count
            ));
            // Add i32 parameters
            for _ in 0..*i32_count {
                output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            }
            // Add pointer parameter
            output.push_str("                sig.params.push(AbiParam::new(pointer_type));\n");
            // Add return value
            output.push_str("                sig.returns.push(AbiParam::new(types::I32));\n");
            output.push_str("            }\n");
        }

        // Group StructReturn functions by parameter count
        let struct_return_6_params: Vec<_> = struct_return_builtins
            .iter()
            .filter(|b| b.param_count == 6)
            .collect();
        let struct_return_5_params: Vec<_> = struct_return_builtins
            .iter()
            .filter(|b| b.param_count == 5)
            .collect();
        let struct_return_4_params: Vec<_> = struct_return_builtins
            .iter()
            .filter(|b| b.param_count == 4)
            .collect();
        let struct_return_3_params: Vec<_> = struct_return_builtins
            .iter()
            .filter(|b| b.param_count == 3)
            .collect();
        let struct_return_2_params: Vec<_> = struct_return_builtins
            .iter()
            .filter(|b| b.param_count == 2)
            .collect();

        // Generate StructReturn signatures
        if !struct_return_6_params.is_empty() {
            output.push_str("            ");
            for (i, builtin) in struct_return_6_params.iter().enumerate() {
                if i > 0 {
                    output.push_str(" | ");
                }
                output.push_str(&format!("BuiltinId::{}", builtin.enum_variant));
            }
            output.push_str(" => {\n");
            output.push_str(
                "                // Result pointer as normal parameter: (pointer_type, i32, i32, i32, i32, i32) -> ()\n",
            );
            output.push_str("                sig.params.insert(0, AbiParam::new(pointer_type));\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                // Functions with result pointer return void\n");
            output.push_str("            }\n");
        }

        if !struct_return_5_params.is_empty() {
            output.push_str("            ");
            for (i, builtin) in struct_return_5_params.iter().enumerate() {
                if i > 0 {
                    output.push_str(" | ");
                }
                output.push_str(&format!("BuiltinId::{}", builtin.enum_variant));
            }
            output.push_str(" => {\n");
            output.push_str(
                "                // Result pointer as normal parameter: (pointer_type, i32, i32, i32, i32) -> ()\n",
            );
            output.push_str("                sig.params.insert(0, AbiParam::new(pointer_type));\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                // Functions with result pointer return void\n");
            output.push_str("            }\n");
        }

        if !struct_return_4_params.is_empty() {
            output.push_str("            ");
            for (i, builtin) in struct_return_4_params.iter().enumerate() {
                if i > 0 {
                    output.push_str(" | ");
                }
                output.push_str(&format!("BuiltinId::{}", builtin.enum_variant));
            }
            output.push_str(" => {\n");
            output.push_str("                // Result pointer as normal parameter: (pointer_type, i32, i32, i32) -> ()\n");
            output.push_str("                sig.params.insert(0, AbiParam::new(pointer_type));\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                // Functions with result pointer return void\n");
            output.push_str("            }\n");
        }

        if !struct_return_3_params.is_empty() {
            output.push_str("            ");
            for (i, builtin) in struct_return_3_params.iter().enumerate() {
                if i > 0 {
                    output.push_str(" | ");
                }
                output.push_str(&format!("BuiltinId::{}", builtin.enum_variant));
            }
            output.push_str(" => {\n");
            output.push_str("                // Result pointer as normal parameter: (pointer_type, i32, i32) -> ()\n");
            output.push_str("                sig.params.insert(0, AbiParam::new(pointer_type));\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                // Functions with result pointer return void\n");
            output.push_str("            }\n");
        }

        if !struct_return_2_params.is_empty() {
            output.push_str("            ");
            for (i, builtin) in struct_return_2_params.iter().enumerate() {
                if i > 0 {
                    output.push_str(" | ");
                }
                output.push_str(&format!("BuiltinId::{}", builtin.enum_variant));
            }
            output.push_str(" => {\n");
            output.push_str("                // Result pointer as normal parameter: (pointer_type, i32) -> ()\n");
            output.push_str("                sig.params.insert(0, AbiParam::new(pointer_type));\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                // Functions with result pointer return void\n");
            output.push_str("            }\n");
        }

        // Group regular functions by parameter count
        let senary_ops: Vec<_> = regular_builtins
            .iter()
            .filter(|b| b.param_count == 6)
            .collect();
        let quinary_ops: Vec<_> = regular_builtins
            .iter()
            .filter(|b| b.param_count == 5)
            .collect();
        let quaternary_ops: Vec<_> = regular_builtins
            .iter()
            .filter(|b| b.param_count == 4)
            .collect();
        let ternary_ops: Vec<_> = regular_builtins
            .iter()
            .filter(|b| b.param_count == 3)
            .collect();
        let binary_ops: Vec<_> = regular_builtins
            .iter()
            .filter(|b| b.param_count == 2)
            .collect();
        let unary_ops: Vec<_> = regular_builtins
            .iter()
            .filter(|b| b.param_count == 1)
            .collect();

        if !senary_ops.is_empty() {
            output.push_str("            ");
            for (i, builtin) in senary_ops.iter().enumerate() {
                if i > 0 {
                    output.push_str(" | ");
                }
                output.push_str(&format!("BuiltinId::{}", builtin.enum_variant));
            }
            output.push_str(" => {\n");
            output.push_str("                // (i32, i32, i32, i32, i32, i32) -> i32\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                sig.returns.push(AbiParam::new(types::I32));\n");
            output.push_str("            }\n");
        }

        if !quinary_ops.is_empty() {
            output.push_str("            ");
            for (i, builtin) in quinary_ops.iter().enumerate() {
                if i > 0 {
                    output.push_str(" | ");
                }
                output.push_str(&format!("BuiltinId::{}", builtin.enum_variant));
            }
            output.push_str(" => {\n");
            output.push_str("                // (i32, i32, i32, i32, i32) -> i32\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                sig.returns.push(AbiParam::new(types::I32));\n");
            output.push_str("            }\n");
        }

        if !quaternary_ops.is_empty() {
            output.push_str("            ");
            for (i, builtin) in quaternary_ops.iter().enumerate() {
                if i > 0 {
                    output.push_str(" | ");
                }
                output.push_str(&format!("BuiltinId::{}", builtin.enum_variant));
            }
            output.push_str(" => {\n");
            output.push_str("                // (i32, i32, i32, i32) -> i32\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                sig.returns.push(AbiParam::new(types::I32));\n");
            output.push_str("            }\n");
        }

        if !ternary_ops.is_empty() {
            output.push_str("            ");
            for (i, builtin) in ternary_ops.iter().enumerate() {
                if i > 0 {
                    output.push_str(" | ");
                }
                output.push_str(&format!("BuiltinId::{}", builtin.enum_variant));
            }
            output.push_str(" => {\n");
            output.push_str("                // (i32, i32, i32) -> i32\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                sig.returns.push(AbiParam::new(types::I32));\n");
            output.push_str("            }\n");
        }

        if !binary_ops.is_empty() {
            output.push_str("            ");
            for (i, builtin) in binary_ops.iter().enumerate() {
                if i > 0 {
                    output.push_str(" | ");
                }
                output.push_str(&format!("BuiltinId::{}", builtin.enum_variant));
            }
            output.push_str(" => {\n");
            output.push_str("                // (i32, i32) -> i32\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                sig.returns.push(AbiParam::new(types::I32));\n");
            output.push_str("            }\n");
        }

        if !unary_ops.is_empty() {
            output.push_str("            ");
            for (i, builtin) in unary_ops.iter().enumerate() {
                if i > 0 {
                    output.push_str(" | ");
                }
                output.push_str(&format!("BuiltinId::{}", builtin.enum_variant));
            }
            output.push_str(" => {\n");
            output.push_str("                // (i32) -> i32\n");
            output.push_str("                sig.params.push(AbiParam::new(types::I32));\n");
            output.push_str("                sig.returns.push(AbiParam::new(types::I32));\n");
            output.push_str("            }\n");
        }
    }

    output.push_str("    }\n");
    output.push_str("    sig\n");
    output.push_str("}\n\n");

    // Generate get_function_pointer()
    output.push_str("/// Get function pointer for a builtin (JIT mode only).\n");
    output.push_str("///\n");
    output.push_str("/// Returns the function pointer that can be registered with JITModule.\n");
    output.push_str("pub fn get_function_pointer(builtin: BuiltinId) -> *const u8 {\n");

    // Collect unique import paths for `use lp_glsl_builtins::builtins::{ glsl::{...}, lpir::{...}, lpfx::..., ... };`
    let mut glsl_files: BTreeSet<String> = BTreeSet::new();
    let mut lpir_files: BTreeSet<String> = BTreeSet::new();
    let mut lpfx_roots: BTreeSet<String> = BTreeSet::new();

    for builtin in builtins {
        let components: Vec<&str> = builtin.module_path.split("::").collect();
        match components.first().copied() {
            Some("glsl") if components.len() >= 2 => {
                glsl_files.insert(components[1].to_string());
            }
            Some("lpir") if components.len() >= 2 => {
                lpir_files.insert(components[1].to_string());
            }
            Some("lpfx") if components.len() >= 2 => {
                lpfx_roots.insert(format!("lpfx::{}", components[1]));
            }
            _ => {}
        }
    }

    let mut import_parts: Vec<String> = Vec::new();
    if !glsl_files.is_empty() {
        import_parts.push(format!(
            "glsl::{{{}}}",
            glsl_files.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    if !lpir_files.is_empty() {
        import_parts.push(format!(
            "lpir::{{{}}}",
            lpir_files.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    import_parts.extend(lpfx_roots);
    import_parts.sort();

    if !import_parts.is_empty() {
        output.push_str(&format!(
            "    use lp_glsl_builtins::builtins::{{{}}};\n",
            import_parts.join(", ")
        ));
    }

    output.push_str("    match builtin {\n");
    if builtins.is_empty() {
        output.push_str("        BuiltinId::_Placeholder => core::ptr::null(),\n");
    } else {
        for builtin in builtins {
            // Path prefix for `usage_path::__lp_*` in the match (after `use` above).
            let components: Vec<&str> = builtin.module_path.split("::").collect();
            let usage_path = if components.len() >= 2 {
                components[1..].join("::")
            } else {
                builtin.module_path.clone()
            };

            output.push_str(&format!(
                "        BuiltinId::{} => {}::{} as *const u8,\n",
                builtin.enum_variant, usage_path, builtin.function_name
            ));
        }
    }
    output.push_str("    }\n");
    output.push_str("}\n\n");

    // Generate declare_builtins and related functions
    output.push_str("/// Declare builtin functions as external symbols.\n");
    output.push_str("///\n");
    output.push_str(
        "/// This is the same for both JIT and emulator - they both use Linkage::Import.\n",
    );
    output.push_str("/// The difference is only in how they're linked:\n");
    output.push_str(
        "/// - JIT: Function pointers are registered via symbol_lookup_fn during module creation\n",
    );
    output.push_str(
        "/// - Emulator: Symbols are resolved by the linker when linking the static library\n",
    );
    output.push_str("///\n");
    output.push_str("/// `pointer_type` is the native pointer type for the target architecture.\n");
    output.push_str("/// For RISC-V 32-bit, this should be `types::I32`.\n");
    output.push_str(
        "/// For 64-bit architectures (like Apple Silicon), this should be `types::I64`.\n",
    );
    output.push_str("/// `format` filters builtins: in Q32 mode, F32-only builtins are skipped; in Float mode, Q32 builtins are skipped.\n");
    output.push_str("pub fn declare_builtins<M: Module>(\n");
    output.push_str("    module: &mut M,\n");
    output.push_str("    pointer_type: types::Type,\n");
    output.push_str("    format: crate::FloatMode,\n");
    output.push_str(") -> Result<(), GlslError> {\n");
    output.push_str("    for builtin in BuiltinId::all() {\n");
    output.push_str("        if let Some(f) = builtin.format() {\n");
    output.push_str("            if f != format {\n");
    output.push_str("                continue;\n");
    output.push_str("            }\n");
    output.push_str("        }\n");
    output.push_str("        let name = builtin.name();\n");
    output.push_str("        let sig = signature_for_builtin(*builtin, pointer_type);\n\n");
    output.push_str("        module\n");
    output.push_str("            .declare_function(name, Linkage::Import, &sig)\n");
    output.push_str("            .map_err(|e| {\n");
    output.push_str("                GlslError::new(\n");
    output.push_str("                    ErrorCode::E0400,\n");
    output.push_str("                    format!(\"Failed to declare builtin '{name}': {e}\"),\n");
    output.push_str("                )\n");
    output.push_str("            })?;\n");
    output.push_str("    }\n\n");
    output.push_str("    Ok(())\n");
    output.push_str("}\n\n");

    output.push_str("/// Declare and link builtin functions for JIT mode.\n");
    output.push_str("///\n");
    output
        .push_str("/// This declares all builtins as external functions. The function pointers\n");
    output.push_str(
        "/// are registered via a symbol lookup function that's added during module creation.\n",
    );
    output.push_str("///\n");
    output.push_str("/// `pointer_type` is the native pointer type for the target architecture.\n");
    output.push_str("pub fn declare_for_jit<M: Module>(\n");
    output.push_str("    module: &mut M,\n");
    output.push_str("    pointer_type: types::Type,\n");
    output.push_str("    format: crate::FloatMode,\n");
    output.push_str(") -> Result<(), GlslError> {\n");
    output.push_str("    declare_builtins(module, pointer_type, format)\n");
    output.push_str("}\n\n");

    output.push_str("/// Declare builtin functions as external symbols for emulator mode.\n");
    output.push_str("///\n");
    output.push_str(
        "/// This declares all builtins as external symbols (Linkage::Import) that will\n",
    );
    output.push_str("/// be resolved by the linker when linking the static library.\n");
    output.push_str("///\n");
    output.push_str("/// `pointer_type` is the native pointer type for the target architecture.\n");
    output.push_str("pub fn declare_for_emulator<M: Module>(\n");
    output.push_str("    module: &mut M,\n");
    output.push_str("    pointer_type: types::Type,\n");
    output.push_str("    format: crate::FloatMode,\n");
    output.push_str(") -> Result<(), GlslError> {\n");
    output.push_str("    declare_builtins(module, pointer_type, format)\n");
    output.push_str("}\n");

    fs::write(path, output).expect("Failed to write registry.rs");
}

fn generate_builtin_refs(path: &Path, builtins: &[BuiltinInfo]) {
    let header = r#"//! This file is AUTO-GENERATED. Do not edit manually.
//!
//! To regenerate this file, run:
//!     cargo run --bin lp-glsl-builtins-gen-app --manifest-path lp-glsl/lp-glsl-builtins-gen-app/Cargo.toml
//!
//! Or use the build script:
//!     scripts/build-builtins.sh

"#;

    let mut output = String::from(header);
    if builtins.is_empty() {
        output.push_str("// No builtins to import\n\n");
    } else {
        // Group builtins by module path
        use std::collections::HashMap;
        let mut builtins_by_module: HashMap<String, Vec<&BuiltinInfo>> = HashMap::new();
        for builtin in builtins {
            builtins_by_module
                .entry(builtin.module_path.clone())
                .or_default()
                .push(builtin);
        }

        // Generate imports for each module
        // Sort modules for consistent output
        let mut modules: Vec<_> = builtins_by_module.keys().collect();
        modules.sort();

        for module_path in modules {
            let module_builtins = &builtins_by_module[module_path];

            let components: Vec<&str> = module_path.split("::").collect();

            let (import_path, function_prefix) = if components.len() == 1 {
                (
                    format!("lp_glsl_builtins::builtins::{}", components[0]),
                    None,
                )
            } else if components[0] == "glsl" || components[0] == "lpir" {
                // One Rust module per file: import symbols from the leaf module.
                (format!("lp_glsl_builtins::builtins::{}", module_path), None)
            } else {
                // lpfx::...::file — import parent path, qualify with last component
                let import_components = &components[..components.len() - 1];
                let import_path_str = import_components.join("::");
                let import_path = format!("lp_glsl_builtins::builtins::{}", import_path_str);
                let function_prefix = Some(components.last().unwrap());
                (import_path, function_prefix)
            };

            output.push_str(&format!("use {}::{{\n", import_path));
            for (i, builtin) in module_builtins.iter().enumerate() {
                if i > 0 {
                    output.push_str(",\n");
                }
                // Include submodule prefix if needed
                if let Some(prefix) = function_prefix {
                    output.push_str(&format!("    {}::{}", prefix, builtin.function_name));
                } else {
                    output.push_str(&format!("    {}", builtin.function_name));
                }
            }
            output.push_str(",\n};\n");
        }
        output.push('\n');
    }

    output.push_str("/// Reference all builtin functions to prevent dead code elimination.\n");
    output.push_str("///\n");
    output.push_str(
        "/// This function ensures all builtin functions are included in the executable\n",
    );
    output
        .push_str("/// by creating function pointers and reading them with volatile operations.\n");
    output.push_str("pub fn ensure_builtins_referenced() {\n");
    output.push_str("    unsafe {\n");

    // Generate function pointer declarations
    for builtin in builtins {
        // Use the extracted Rust signature - this should always be available
        let fn_type = &builtin.rust_signature;
        // Use full function name suffix for unique variable names
        let var_suffix = builtin
            .function_name
            .strip_prefix("__lp_")
            .unwrap_or(&builtin.function_name);
        let var_suffix = var_suffix.replace("__", "_").replace("-", "_");
        output.push_str(&format!(
            "        let _{var_suffix}_fn: {fn_type} = {};\n",
            builtin.function_name
        ));
    }

    output.push('\n');
    output.push_str("        // Force these to be included by using them in a way that can't be optimized away\n");
    output.push_str("        // We'll use volatile reads to prevent optimization\n");

    // Generate read_volatile calls
    for builtin in builtins {
        // Use full function name suffix for unique variable names (same as above)
        let var_suffix = builtin
            .function_name
            .strip_prefix("__lp_")
            .unwrap_or(&builtin.function_name);
        let var_suffix = var_suffix.replace("__", "_").replace("-", "_");
        let var_name = format!("_{var_suffix}_fn");
        output.push_str(&format!(
            "        let _ = core::ptr::read_volatile(&{} as *const _);\n",
            var_name
        ));
    }

    output.push_str("    }\n");
    output.push_str("}\n");

    fs::write(path, output).expect("Failed to write builtin_refs.rs");
}

fn generate_dir_mod_rs(path: &Path, builtins: &[BuiltinInfo], doc_line: &str) {
    let header = r#"//! This file is AUTO-GENERATED. Do not edit manually.
//!
//! To regenerate this file, run:
//!     cargo run --bin lp-glsl-builtins-gen-app --manifest-path lp-glsl/lp-glsl-builtins-gen-app/Cargo.toml
//!
//! Or use the build script:
//!     scripts/build-builtins.sh

"#;

    let mut output = String::from(header);
    output.push_str(&format!("//! {}\n\n", doc_line));

    let mut names: BTreeSet<String> = BTreeSet::new();
    for builtin in builtins {
        names.insert(builtin.file_name.clone());
    }
    for name in names {
        output.push_str(&format!("pub mod {};\n", name));
    }

    fs::write(path, output).expect("Failed to write mod.rs");
}

fn generate_testcase_mapping(path: &Path, builtins: &[BuiltinInfo]) {
    // Read existing file
    let content = fs::read_to_string(path).expect("Failed to read mapping.rs");

    // Find the map_testcase_to_builtin function and replace it
    let start_marker = "/// Map TestCase function name and argument count to BuiltinId.";

    let start_idx = content
        .find(start_marker)
        .expect("Could not find map_testcase_to_builtin function");

    // Find the end of the function (look for the closing brace after the match)
    let mut end_idx = start_idx;
    let mut brace_count = 0;
    let mut in_function = false;

    for (i, ch) in content[start_idx..].char_indices() {
        if ch == '{' {
            brace_count += 1;
            in_function = true;
        } else if ch == '}' {
            brace_count -= 1;
            if in_function && brace_count == 0 {
                end_idx = start_idx + i + 1;
                break;
            }
        }
    }

    let before = &content[..start_idx];
    let after = &content[end_idx..];

    let header = "/// Map TestCase function name and argument count to BuiltinId.\n///\n/// Returns None if the function name is not a math function that should be converted.\n/// Handles both standard C math function names (sinf, cosf) and intrinsic names (__lp_sin, __lp_cos).\n/// Supports overloaded functions by matching on both name and argument count.\n///\n/// This function is AUTO-GENERATED. Do not edit manually.\n///\n/// To regenerate this function, run:\n///     cargo run --bin lp-glsl-builtins-gen-app --manifest-path lp-glsl/lp-glsl-builtins-gen-app/Cargo.toml\n///\n/// Or use the build script:\n///     scripts/build-builtins.sh\n";

    let mut new_function = String::from(header);
    new_function.push_str(
        "pub fn map_testcase_to_builtin(testcase_name: &str, arg_count: usize) -> Option<BuiltinId> {\n",
    );
    new_function.push_str("    match (testcase_name, arg_count) {\n");

    // Generate mappings
    if builtins.is_empty() {
        // No builtins, so no mappings
    } else {
        for builtin in builtins {
            if builtin.module_path.starts_with("lpfx::") {
                continue;
            }
            let intrinsic_name = builtin.symbol_name.clone();
            let q32_alias = format!("lp_{}_{}f", builtin.builtin_module, builtin.builtin_fn_name);
            let mut extra = String::new();
            if builtin.builtin_module == "glsl" {
                if builtin.builtin_fn_name == "mod" {
                    extra.push_str(" | \"fmodf\"");
                } else {
                    extra.push_str(&format!(" | \"{}f\"", builtin.builtin_fn_name));
                }
            }
            if builtin.builtin_fn_name == "fnearest" {
                extra.push_str(" | \"roundevenf\"");
            }
            if builtin.builtin_module == "lpir" && builtin.builtin_fn_name == "fsqrt" {
                extra.push_str(" | \"sqrtf\"");
            }
            new_function.push_str(&format!(
                "        (\"{}\" | \"{}\"{}, {}) => Some(BuiltinId::{}),\n",
                q32_alias, intrinsic_name, extra, builtin.param_count, builtin.enum_variant
            ));
        }
    }

    new_function.push_str("        _ => None,\n");
    new_function.push_str("    }\n");
    new_function.push_str("}\n");

    let new_content = format!("{}{}{}", before, new_function, after);
    fs::write(path, new_content).expect("Failed to write mapping.rs");
}

fn format_generated_files(workspace_root: &Path, files: &[&Path]) {
    use std::process::Command;

    // Run cargo fmt on the generated files
    let mut cmd = Command::new("cargo");
    cmd.arg("fmt");
    cmd.arg("--");

    for file in files {
        // Get relative path from workspace root
        if let Ok(relative_path) = file.strip_prefix(workspace_root) {
            cmd.arg(relative_path);
        }
    }

    // Run from workspace root
    let output = cmd
        .current_dir(workspace_root)
        .output()
        .expect("Failed to run cargo fmt");

    if !output.status.success() {
        eprintln!("Warning: cargo fmt failed on generated files:");
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    }
}
