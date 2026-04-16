use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use syn::{Item, ItemFn, parse_file};
use walkdir::WalkDir;

mod discovery;
mod lpfn;
mod native_dispatch_codegen;

use discovery::discover_lpfn_functions;
use lpfn::errors::Variant;
use lpfn::grouping::{group_by_signature, group_functions_by_name};
use lpfn::process::process_lpfn_functions;
use lpfn::types::Type;
use lpfn::validate::{ParsedLpfnFunction, validate_lpfn_functions};

#[derive(Debug, Clone)]
pub(crate) struct BuiltinInfo {
    enum_variant: String,
    symbol_name: String,
    function_name: String,
    param_count: usize,
    /// GLSL/import-visible parameter count (excludes implicit `VmContext` pointer).
    user_visible_param_count: usize,
    /// True when the Rust `extern "C"` fn takes `*const VmContext` / `&VmContext` as first param.
    needs_vmctx: bool,
    file_name: String,
    /// Rust function signature types as strings (e.g., "extern \"C\" fn(f32, u32) -> f32")
    rust_signature: String,
    /// Module path relative to builtins/ directory (e.g., "glsl::sin_q32", "lpir::fsqrt_q32", "lpfn::hash")
    module_path: String,
    /// `lpir`, `glsl`, or `lpfn`
    builtin_module: String,
    /// Function name within the module (e.g. `fadd`, `sin`, `fbm2`, `hash_1`)
    builtin_fn_name: String,
    /// `q32`, `f32`, or mode-independent
    builtin_mode: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let workspace_root = find_workspace_root().expect("Failed to find workspace root");
    let builtins_dir = workspace_root
        .join("lps-builtins")
        .join("src")
        .join("builtins");
    let glsl_dir = builtins_dir.join("glsl");
    let lpir_dir = builtins_dir.join("lpir");
    let lpfn_dir = builtins_dir.join("lpfn");

    let mut builtins =
        discover_builtins(&glsl_dir, &builtins_dir).expect("Failed to discover glsl builtins");
    builtins.extend(
        discover_builtins(&lpir_dir, &builtins_dir).expect("Failed to discover lpir builtins"),
    );
    builtins.extend(
        discover_builtins(&lpfn_dir, &builtins_dir).expect("Failed to discover lpfn builtins"),
    );
    let vm_dir = builtins_dir.join("vm");
    builtins
        .extend(discover_builtins(&vm_dir, &builtins_dir).expect("Failed to discover vm builtins"));

    let glsl_map_path = workspace_root
        .join("lps-builtin-ids")
        .join("src")
        .join("glsl_builtin_mapping.rs");
    generate_glsl_builtin_mapping(&glsl_map_path, &builtins, &lpfn_dir)?;

    // Generate builtin-ids lib.rs (after `glsl_builtin_mapping.rs` for consistent partial runs)
    let builtin_ids_path = workspace_root
        .join("lps-builtin-ids")
        .join("src")
        .join("lib.rs");
    generate_builtin_ids(&builtin_ids_path, &builtins);

    let lpir_builtin_abi_path = workspace_root
        .join("lpvm-cranelift")
        .join("src")
        .join("generated_builtin_abi.rs");
    generate_lpvm_cranelift_builtin_abi(&lpir_builtin_abi_path, &builtins);

    // Generate builtin_refs.rs (RISC-V emu app)
    let builtin_refs_path = workspace_root
        .join("lps-builtins-emu-app")
        .join("src")
        .join("builtin_refs.rs");
    generate_builtin_refs(&builtin_refs_path, &builtins, "lps_builtins");

    // Generate builtin_refs.rs (inside lps-builtins for `crate::` paths / DCE)
    let builtin_refs_lps_path = workspace_root
        .join("lps-builtins")
        .join("src")
        .join("builtin_refs.rs");
    generate_builtin_refs(&builtin_refs_lps_path, &builtins, "crate");

    // Generate glsl/mod.rs and lpir/mod.rs (submodule lists only; lpfn keeps hand-written mod tree)
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
        .join("lps-builtins")
        .join("src")
        .join("builtins")
        .join("glsl")
        .join("mod.rs");
    let lpir_mod_rs_path = workspace_root
        .join("lps-builtins")
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

    let vm_builtins: Vec<BuiltinInfo> = builtins
        .iter()
        .filter(|b| b.module_path.starts_with("vm::"))
        .cloned()
        .collect();
    let vm_mod_rs_path = workspace_root
        .join("lps-builtins")
        .join("src")
        .join("builtins")
        .join("vm")
        .join("mod.rs");
    generate_dir_mod_rs(
        &vm_mod_rs_path,
        &vm_builtins,
        "VMContext-aware builtins (fixed-point Q32).",
    );

    let wasm_import_types_path = workspace_root
        .join("lpvm-wasm")
        .join("src")
        .join("emit")
        .join("builtin_wasm_import_types.rs");
    generate_wasm_import_types(&wasm_import_types_path, &builtins);

    let native_dispatch_path = workspace_root
        .join("lpvm-wasm")
        .join("src")
        .join("rt_wasmtime")
        .join("native_builtin_dispatch.rs");
    native_dispatch_codegen::generate_native_wasmtime_dispatch(&native_dispatch_path, &builtins);

    // Format generated files using cargo fmt
    // Need actual workspace root for cargo fmt, not lps directory
    let actual_workspace_root = workspace_root
        .parent()
        .ok_or("lps directory has no parent")?;
    format_generated_files(
        actual_workspace_root,
        &[
            &builtin_ids_path,
            &lpir_builtin_abi_path,
            &builtin_refs_path,
            &builtin_refs_lps_path,
            &glsl_mod_rs_path,
            &lpir_mod_rs_path,
            &vm_mod_rs_path,
            &glsl_map_path,
            &wasm_import_types_path,
            &native_dispatch_path,
        ],
    );

    println!("Generated all builtin boilerplate files");
    Ok(())
}

/// GLSL / LPFX name → `BuiltinId` for WASM Q32 codegen (auto-generated).
fn generate_glsl_builtin_mapping(
    path: &Path,
    builtins: &[BuiltinInfo],
    lpfn_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let discovered = discover_lpfn_functions(lpfn_dir)?;
    let parsed = process_lpfn_functions(&discovered)?;
    validate_lpfn_functions(&parsed)?;

    let header = r#"//! GLSL / LPIR / LPFX name → `BuiltinId` for Q32 WASM imports.
//!
//! AUTO-GENERATED by lps-builtins-gen-app. Do not edit manually.
//!
//! - `glsl_q32_math_builtin_id`: `@glsl::*` scalar imports.
//! - `lpir_q32_builtin_id`: `@lpir::*` library ops (e.g. `sqrt`).
//! - `glsl_lpfn_q32_builtin_id`: `lpfn_*` overloads keyed by parameter types.
//!
//! Regenerate: `cargo run -p lps-builtins-gen-app` or `scripts/build-builtins.sh`

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
         /// Map `@vm::*` import name + user-visible argument count to a Q32 builtin.\n\
         pub fn vm_q32_builtin_id(name: &str, arg_count: usize) -> Option<BuiltinId> {\n\
         match (name, arg_count) {\n",
    );

    for builtin in builtins {
        if builtin.builtin_module != "vm" {
            continue;
        }
        let import_name = format!("__lp_{}", builtin.builtin_fn_name);
        out.push_str(&format!(
            "        (\"{}\", {}) => Some(BuiltinId::{}),\n",
            import_name, builtin.user_visible_param_count, builtin.enum_variant
        ));
    }

    out.push_str(
        "        _ => None,\n    }\n}\n\n\
         /// Map `lpfn_*` name + parameter type list to the Q32 `BuiltinId`.\n\
         pub fn glsl_lpfn_q32_builtin_id(name: &str, params: &[GlslParamKind]) -> Option<BuiltinId> {\n\
         match (name, params) {\n",
    );

    let mut lpfn_arms: Vec<String> = Vec::new();
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
            let Some(variant) = lpfn_q32_builtin_variant(&signature_funcs) else {
                continue;
            };
            let kinds: Vec<String> = sig
                .parameters
                .iter()
                .map(|p| format!("GlslParamKind::{}", type_to_glsl_param_kind_variant(&p.ty)))
                .collect();
            let pat = kinds.join(", ");
            let escaped_name = sig.name.replace('\\', "\\\\").replace('"', "\\\"");
            lpfn_arms.push(format!(
                "        (\"{escaped_name}\", &[{pat}]) => Some(BuiltinId::{variant}),\n",
            ));
        }
    }
    lpfn_arms.sort();
    for arm in lpfn_arms {
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
        .expect("lpfn fbm2 q32");
    let sqrt_v = builtins
        .iter()
        .find(|b| b.builtin_module == "lpir" && b.builtin_fn_name == "fsqrt")
        .map(|b| b.enum_variant.as_str())
        .expect("lpir fsqrt builtin");
    let get_fuel_v = builtins
        .iter()
        .find(|b| b.builtin_module == "vm" && b.builtin_fn_name == "get_fuel")
        .map(|b| b.enum_variant.as_str())
        .expect("vm get_fuel builtin");

    out.push_str(&format!(
        "#[cfg(test)]\nmod glsl_builtin_mapping_tests {{\n    use crate::BuiltinId;\n    use super::{{glsl_lpfn_q32_builtin_id, glsl_q32_math_builtin_id, lpir_q32_builtin_id, vm_q32_builtin_id, GlslParamKind}};\n\n    #[test]\n    fn q32_sin() {{\n        assert_eq!(\n            glsl_q32_math_builtin_id(\"sin\", 1),\n            Some(BuiltinId::{sin_v})\n        );\n    }}\n\n    #[test]\n    fn q32_atan_two_args_is_atan2_import() {{\n        assert_eq!(\n            glsl_q32_math_builtin_id(\"atan\", 2),\n            Some(BuiltinId::{atan2_v})\n        );\n    }}\n\n    #[test]\n    fn lpir_sqrt() {{\n        assert_eq!(lpir_q32_builtin_id(\"sqrt\", 1), Some(BuiltinId::{sqrt_v}));\n    }}\n\n    #[test]\n    fn vm_get_fuel() {{\n        assert_eq!(\n            vm_q32_builtin_id(\"__lp_get_fuel\", 0),\n            Some(BuiltinId::{get_fuel_v})\n        );\n    }}\n\n    #[test]\n    fn lpfn_fbm_vec2() {{\n        assert_eq!(\n            glsl_lpfn_q32_builtin_id(\n                \"lpfn_fbm\",\n                &[GlslParamKind::Vec2, GlslParamKind::Int, GlslParamKind::UInt],\n            ),\n            Some(BuiltinId::{fbm_v})\n        );\n    }}\n}}\n",
        sin_v = sin_v,
        atan2_v = atan2_v,
        sqrt_v = sqrt_v,
        get_fuel_v = get_fuel_v,
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

fn lpfn_q32_builtin_variant(funcs: &[&ParsedLpfnFunction]) -> Option<String> {
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
    // Directory that contains `lps-builtins/` (lp-shader/), or repo root + lp-shader/
    let mut current = std::env::current_dir()?;
    loop {
        if current.join("lps-builtins").join("Cargo.toml").exists() {
            return Ok(current);
        }
        let lp_sh = current.join("lp-shader");
        if lp_sh.join("lps-builtins").join("Cargo.toml").exists() {
            return Ok(lp_sh);
        }
        if !current.pop() {
            break;
        }
    }
    Err("Could not find lp-shader root (missing lps-builtins/Cargo.toml). Run from lp-shader/ or repo root.".into())
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
        // This includes the full directory structure: "glsl::sin_q32", "lpfn::hash", ...
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

    // `__lps_*` — GLSL imports; `__lp_lpir_*` / `__lp_lpfn_*` / `__lp_vm_*`
    let (builtin_module, rest) = if let Some(r) = func_name.strip_prefix("__lps_") {
        ("glsl", r)
    } else if let Some(after_lp) = func_name.strip_prefix("__lp_") {
        if let Some(r) = after_lp.strip_prefix("lpir_") {
            ("lpir", r)
        } else if let Some(r) = after_lp.strip_prefix("glsl_") {
            ("glsl", r)
        } else if let Some(r) = after_lp.strip_prefix("vm_") {
            ("vm", r)
        } else if let Some(r) = after_lp.strip_prefix("lpfn_") {
            ("lpfn", r)
        } else {
            return None;
        }
    } else {
        return None;
    };

    let (fn_body, builtin_mode) = if let Some(s) = rest.strip_suffix("_q32") {
        (s, Some("q32".to_string()))
    } else if let Some(s) = rest.strip_suffix("_f32") {
        (s, Some("f32".to_string()))
    } else {
        (rest, None)
    };
    let builtin_fn_name = fn_body.to_string();
    let builtin_module = builtin_module.to_string();

    let symbol_name = func_name.clone();

    // Match `lps-builtin-ids` naming: LpGlslSinQ32, LpLpirFaddQ32, LpLpfnHash1, …
    let enum_variant = {
        let pascal: String = rest
            .split('_')
            .map(|s| {
                let mut chars = s.chars();
                match chars.next() {
                    None => String::new(),
                    Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                }
            })
            .collect::<String>();
        let prefix = match builtin_module.as_str() {
            "glsl" => "LpGlsl",
            "lpir" => "LpLpir",
            "vm" => "LpVm",
            "lpfn" => "LpLpfn",
            _ => return None,
        };
        format!("{prefix}{pascal}")
    };

    let param_count = func.sig.inputs.len();
    let rust_signature = format_rust_function_signature(func);
    let needs_vmctx = rust_signature_contains_vmcontext(&rust_signature) || builtin_module == "vm";
    let user_visible_param_count = param_count.saturating_sub(usize::from(needs_vmctx));

    Some(BuiltinInfo {
        enum_variant,
        symbol_name,
        function_name: func_name,
        param_count,
        user_visible_param_count,
        needs_vmctx,
        file_name: file_name.to_string(),
        rust_signature,
        module_path: module_path.to_string(),
        builtin_module,
        builtin_fn_name,
        builtin_mode,
    })
}

fn rust_signature_contains_vmcontext(rust_sig: &str) -> bool {
    rust_sig.contains("VmContext")
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
//! AUTO-GENERATED by lps-builtins-gen-app. Do not edit manually.
//!
//! Regenerate: `cargo run -p lps-builtins-gen-app` or `scripts/build-builtins.sh`

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;
use lps_builtin_ids::BuiltinId;
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
    let header = r#"//! Builtin function IDs for lps.
//!
//! This file is AUTO-GENERATED by lps-builtins-gen-app. Do not edit manually.
//!
//! To regenerate: `cargo run -p lps-builtins-gen-app`

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
                "vm" => "Module::Vm",
                "lpfn" => "Module::Lpfn",
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
    output.push_str("    }\n\n");

    output.push_str("    pub fn needs_vmctx(&self) -> bool {\n");
    output.push_str("        match self {\n");
    if builtins.is_empty() {
        output.push_str("            BuiltinId::_Placeholder => false,\n");
    } else {
        for builtin in builtins {
            output.push_str(&format!(
                "            BuiltinId::{} => {},\n",
                builtin.enum_variant, builtin.needs_vmctx
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
    output.push_str("    Vm,\n");
    output.push_str("    Lpfn,\n");
    output.push_str("}\n\n");
    output.push_str("/// Float ABI for mode-specific builtins.\n");
    output.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]\n");
    output.push_str("pub enum Mode {\n");
    output.push_str("    Q32,\n");
    output.push_str("    F32,\n");
    output.push_str("}\n\n");

    output.push_str("mod glsl_builtin_mapping;\n\n");
    output.push_str("pub use glsl_builtin_mapping::glsl_lpfn_q32_builtin_id;\n");
    output.push_str("pub use glsl_builtin_mapping::glsl_q32_math_builtin_id;\n");
    output.push_str("pub use glsl_builtin_mapping::lpir_q32_builtin_id;\n");
    output.push_str("pub use glsl_builtin_mapping::vm_q32_builtin_id;\n");
    output.push_str("pub use glsl_builtin_mapping::GlslParamKind;\n");

    fs::write(path, output).expect("Failed to write builtin-ids lib.rs");
}

/// Split comma-separated list at nesting depth 0 (for simple `extern "C"` fn param lists).
fn split_top_level_commas(s: &str) -> Vec<String> {
    let mut depth: i32 = 0;
    let mut start = 0;
    let mut out = Vec::new();
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                let t = s[start..i].trim();
                if !t.is_empty() {
                    out.push(t.to_string());
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    let t = s[start..].trim();
    if !t.is_empty() {
        out.push(t.to_string());
    }
    out
}

pub(crate) fn parse_rust_extern_sig(rust_sig: &str) -> (Vec<String>, String) {
    let rest = rust_sig
        .split("fn(")
        .nth(1)
        .unwrap_or_else(|| panic!("invalid rust_signature (no fn(: {:?}", rust_sig));
    let (params_str, ret_str) = rest
        .split_once(") -> ")
        .unwrap_or_else(|| panic!("invalid rust_signature (no ) -> {:?}", rust_sig));
    (
        split_top_level_commas(params_str),
        ret_str.trim().to_string(),
    )
}

fn cranelift_push_for_param_type(ty: &str) -> &'static str {
    let t = ty.trim();
    // VMContext is passed as I32 in LPIR / Cranelift (not ISA pointer width); see `signature_for_ir_func`.
    if t.contains("VmContext") {
        return "sig.params.push(AbiParam::new(types::I32));";
    }
    if t.contains('*') {
        return "sig.params.push(AbiParam::new(pointer_type));";
    }
    match t {
        "i32" | "u32" | "i8" | "u8" | "i16" | "u16" | "i64" | "u64" | "isize" | "usize"
        | "bool" => "sig.params.push(AbiParam::new(types::I32));",
        "f32" => "sig.params.push(AbiParam::new(types::F32));",
        "f64" => "sig.params.push(AbiParam::new(types::F64));",
        _ => panic!("unsupported builtin param type `{t}`", t = t),
    }
}

fn cranelift_push_for_return_type(ty: &str) -> Option<&'static str> {
    match ty.trim() {
        "()" => None,
        "i32" | "u32" | "i8" | "u8" | "i16" | "u16" | "i64" | "u64" | "isize" | "usize"
        | "bool" => Some("sig.returns.push(AbiParam::new(types::I32));"),
        "f32" => Some("sig.returns.push(AbiParam::new(types::F32));"),
        "f64" => Some("sig.returns.push(AbiParam::new(types::F64));"),
        other => panic!("unsupported return type `{other}` (rust_signature)"),
    }
}

/// Grouping key: Cranelift ABI push lines (no comment), derived from each builtin's `rust_signature`.
fn signature_abi_key(rust_sig: &str) -> String {
    let (params, ret) = parse_rust_extern_sig(rust_sig);
    let mut parts = Vec::new();
    for p in &params {
        parts.push(cranelift_push_for_param_type(p).to_string());
    }
    if let Some(r) = cranelift_push_for_return_type(&ret) {
        parts.push(r.to_string());
    }
    parts.join("\n")
}

fn emit_grouped_signature_match_arms(builtins: &[BuiltinInfo]) -> String {
    use std::collections::HashMap;
    let mut groups: HashMap<String, Vec<&BuiltinInfo>> = HashMap::new();
    for b in builtins {
        let key = signature_abi_key(&b.rust_signature);
        groups.entry(key).or_default().push(b);
    }
    let mut entries: Vec<_> = groups.into_iter().collect();
    entries.sort_by(|a, b| a.1[0].enum_variant.cmp(&b.1[0].enum_variant));
    let mut s = String::new();
    for (key, group) in entries {
        let repr = &group[0].rust_signature;
        s.push_str("            ");
        for (i, b) in group.iter().enumerate() {
            if i > 0 {
                s.push_str(" | ");
            }
            s.push_str(&format!("BuiltinId::{}", b.enum_variant));
        }
        s.push_str(" => {\n");
        s.push_str(&format!("                // {}\n", repr.replace('\n', " ")));
        for line in key.lines() {
            s.push_str("                ");
            s.push_str(line);
            s.push('\n');
        }
        s.push_str("            }\n");
    }
    s
}

fn append_get_function_pointer_match(output: &mut String, builtins: &[BuiltinInfo]) {
    let mut glsl_files: BTreeSet<String> = BTreeSet::new();
    let mut lpir_files: BTreeSet<String> = BTreeSet::new();
    let mut vm_files: BTreeSet<String> = BTreeSet::new();
    let mut lpfn_roots: BTreeSet<String> = BTreeSet::new();

    for builtin in builtins {
        let components: Vec<&str> = builtin.module_path.split("::").collect();
        match components.first().copied() {
            Some("glsl") if components.len() >= 2 => {
                glsl_files.insert(components[1].to_string());
            }
            Some("lpir") if components.len() >= 2 => {
                lpir_files.insert(components[1].to_string());
            }
            Some("vm") if components.len() >= 2 => {
                vm_files.insert(components[1].to_string());
            }
            Some("lpfn") if components.len() >= 2 => {
                lpfn_roots.insert(format!("lpfn::{}", components[1]));
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
    if !vm_files.is_empty() {
        import_parts.push(format!(
            "vm::{{{}}}",
            vm_files.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    import_parts.extend(lpfn_roots);
    import_parts.sort();

    if !import_parts.is_empty() {
        output.push_str(&format!(
            "    use lps_builtins::builtins::{{{}}};\n",
            import_parts.join(", ")
        ));
    }

    output.push_str("    match builtin {\n");
    if builtins.is_empty() {
        output.push_str("        BuiltinId::_Placeholder => core::ptr::null(),\n");
    } else {
        for builtin in builtins {
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
}

/// Cranelift signatures derived from each builtin's `rust_signature` strings in `lps-builtins`.
fn generate_lpvm_cranelift_builtin_abi(path: &Path, builtins: &[BuiltinInfo]) {
    let header = r#"//! This file is AUTO-GENERATED. Do not edit manually.
//!
//! To regenerate this file, run:
//!     cargo run --bin lps-builtins-gen-app --manifest-path lp-shader/lps-builtins-gen-app/Cargo.toml
//!
//! Or use the build script:
//!     scripts/build-builtins.sh

//! Cranelift signatures and function pointers for [`BuiltinId`].
//!
//! Generated from `rust_signature` metadata scraped from `lps-builtins`.
//! Changing an `extern "C"` builtin in `lps-builtins` without re-running codegen will desync
//! this file and fail `cargo check` until you regenerate.

"#;

    let mut output = String::from(header);
    output.push_str("use cranelift_codegen::ir::{AbiParam, Signature, types};\n");
    output.push_str("use cranelift_codegen::isa::CallConv;\n");
    output.push_str("use lps_builtin_ids::BuiltinId;\n\n");
    output.push_str(
        "pub(crate) fn cranelift_sig_for_builtin_inner(\n\
    builtin: BuiltinId,\n\
    pointer_type: types::Type,\n\
    call_conv: CallConv,\n\
) -> Signature {\n",
    );
    output.push_str("    let mut sig = Signature::new(call_conv);\n");
    output.push_str("    match builtin {\n");
    if builtins.is_empty() {
        output.push_str("        BuiltinId::_Placeholder => {}\n");
    } else {
        output.push_str(&emit_grouped_signature_match_arms(builtins));
    }
    output.push_str("    }\n");
    output.push_str("    sig\n");
    output.push_str("}\n\n");
    output
        .push_str("pub(crate) fn get_function_pointer_inner(builtin: BuiltinId) -> *const u8 {\n");
    append_get_function_pointer_match(&mut output, builtins);
    output.push_str("}\n");

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create lpvm-cranelift generated parent dir");
    }
    fs::write(path, output).expect("Failed to write generated_builtin_abi.rs");
}

fn generate_builtin_refs(path: &Path, builtins: &[BuiltinInfo], import_root: &str) {
    let header = r#"//! This file is AUTO-GENERATED. Do not edit manually.
//!
//! To regenerate this file, run:
//!     cargo run --bin lps-builtins-gen-app --manifest-path lp-shader/lps-builtins-gen-app/Cargo.toml
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
                (format!("{import_root}::builtins::{}", components[0]), None)
            } else if components[0] == "glsl" || components[0] == "lpir" {
                // One Rust module per file: import symbols from the leaf module.
                (format!("{import_root}::builtins::{}", module_path), None)
            } else {
                // lpfn::...::file — import parent path, qualify with last component
                let import_components = &components[..components.len() - 1];
                let import_path_str = import_components.join("::");
                let import_path = format!("{import_root}::builtins::{}", import_path_str);
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
//!     cargo run --bin lps-builtins-gen-app --manifest-path lp-shader/lps-builtins-gen-app/Cargo.toml
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
