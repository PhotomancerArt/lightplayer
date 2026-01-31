use std::fs;
use std::path::{Path, PathBuf};
use syn::{Item, ItemFn, parse_file};
use walkdir::WalkDir;

mod discovery;
mod lpfx;

use discovery::discover_lpfx_functions;
use lpfx::generate::generate_lpfx_fns;
use lpfx::process::process_lpfx_functions;
use lpfx::validate::validate_lpfx_functions;

#[derive(Debug, Clone)]
struct BuiltinInfo {
    enum_variant: String,
    symbol_name: String,
    function_name: String,
    param_count: usize,
    file_name: String,
    /// Rust function signature types as strings (e.g., "extern \"C\" fn(f32, u32) -> f32")
    rust_signature: String,
    /// Module path relative to builtins/ directory (e.g., "q32", "lpfx::hash", "lpfx::simplex::simplex1_q32")
    module_path: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let workspace_root = find_workspace_root().expect("Failed to find workspace root");
    let builtins_dir = workspace_root
        .join("lp-glsl-builtins")
        .join("src")
        .join("builtins");
    let q32_dir = builtins_dir.join("q32");
    let lpfx_dir = builtins_dir.join("lpfx");

    let mut builtins =
        discover_builtins(&q32_dir, &builtins_dir).expect("Failed to discover builtins");
    let lpfx_builtins =
        discover_builtins(&lpfx_dir, &builtins_dir).expect("Failed to discover lpfx builtins");
    builtins.extend(lpfx_builtins);

    // Generate registry.rs
    let registry_path = workspace_root
        .join("lp-glsl-compiler")
        .join("src")
        .join("backend")
        .join("builtins")
        .join("registry.rs");
    generate_registry(&registry_path, &builtins);

    // Generate builtin_refs.rs
    let builtin_refs_path = workspace_root
        .join("lp-glsl-builtins-emu-app")
        .join("src")
        .join("builtin_refs.rs");
    generate_builtin_refs(&builtin_refs_path, &builtins);

    // Generate mod.rs (only q32 functions, not lpfx functions)
    let q32_builtins: Vec<BuiltinInfo> = builtins
        .iter()
        .filter(|b| b.module_path.starts_with("q32::"))
        .cloned()
        .collect();
    let mod_rs_path = workspace_root
        .join("lp-glsl-builtins")
        .join("src")
        .join("builtins")
        .join("q32")
        .join("mod.rs");
    generate_mod_rs(&mod_rs_path, &q32_builtins);

    // Generate testcase mapping in math.rs
    let math_rs_path = workspace_root
        .join("lp-glsl-compiler")
        .join("src")
        .join("backend")
        .join("transform")
        .join("q32")
        .join("converters")
        .join("math.rs");
    generate_testcase_mapping(&math_rs_path, &builtins);

    // Generate lpfx_fns.rs
    let lpfx_fns_path = workspace_root
        .join("lp-glsl-compiler")
        .join("src")
        .join("frontend")
        .join("semantic")
        .join("lpfx")
        .join("lpfx_fns.rs");
    generate_lpfx_fns_file(&lpfx_fns_path, &lpfx_dir)?;

    // Format generated files using cargo fmt
    // Need actual workspace root for cargo fmt, not lp-glsl directory
    let actual_workspace_root = workspace_root
        .parent()
        .ok_or("lp-glsl directory has no parent")?;
    format_generated_files(
        actual_workspace_root,
        &[
            &registry_path,
            &builtin_refs_path,
            &mod_rs_path,
            &math_rs_path,
            &lpfx_fns_path,
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
                        || check.join("lp-glsl-compiler").exists()
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
        // This includes the full directory structure: "q32", "lpfx::hash", "lpfx::simplex::simplex1_q32"
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

    // Only process functions that start with __ (builtin convention)
    if !func_name.starts_with("__") {
        return None;
    }

    // Extract symbol name (function name)
    let symbol_name = func_name.clone();

    // Derive enum variant name by:
    // 1. Strip leading __
    // 2. Split by _ and capitalize each word
    // 3. Join together
    // Examples:
    // __lp_q32_sqrt -> LpQ32Sqrt
    // __lpfx_hash_1 -> LpfxHash1
    // __lpfx_snoise1_q32 -> LpfxSnoise1Q32
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

    // Count parameters (extern "C" functions don't have self)
    let param_count = func.sig.inputs.len();

    // Extract Rust function signature
    let rust_signature = format_rust_function_signature(func);

    Some(BuiltinInfo {
        enum_variant,
        symbol_name,
        function_name: func_name,
        param_count,
        file_name: file_name.to_string(),
        rust_signature,
        module_path: module_path.to_string(),
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

/// Strip the prefix from a function name to get the base name.
/// This is a generic function that strips the leading `__` and returns the rest.
/// Callers should handle specific patterns as needed.
fn strip_function_prefix(name: &str) -> &str {
    name.strip_prefix("__").unwrap_or(name)
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

    output.push_str("use crate::error::{ErrorCode, GlslError};\n");
    output.push_str("use cranelift_codegen::ir::{AbiParam, Signature, types};\n");
    output.push_str("use cranelift_codegen::isa::CallConv;\n");
    output.push_str("use cranelift_module::{Linkage, Module};\n\n");
    output.push_str("#[cfg(not(feature = \"std\"))]\n");
    output.push_str("use alloc::format;\n\n");

    // Generate enum
    output.push_str("/// Enum identifying builtin functions.\n");
    output.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]\n");
    output.push_str("pub enum BuiltinId {\n");
    if builtins.is_empty() {
        output.push_str("    // No builtins defined yet\n");
        output.push_str("    #[allow(dead_code)]\n");
        output.push_str("    _Placeholder,\n");
    } else {
        for builtin in builtins {
            output.push_str(&format!("    {},\n", builtin.enum_variant));
        }
    }
    output.push_str("}\n\n");

    // Generate impl BuiltinId
    output.push_str("impl BuiltinId {\n");
    output.push_str("    /// Get the symbol name for this builtin function.\n");
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

    // Generate builtin_id_from_name() method
    output.push_str("    /// Get the BuiltinId from its symbol name.\n");
    output.push_str("    ///\n");
    output.push_str("    /// Returns `None` if the name is not a known builtin function.\n");
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

    // Generate signature() method
    output.push_str("    /// Get the Cranelift signature for this builtin function.\n");
    output.push_str("    /// \n");
    output.push_str(
        "    /// `pointer_type` is the native pointer type for the target architecture.\n",
    );
    output.push_str("    /// For RISC-V 32-bit, this should be `types::I32`.\n");
    output.push_str(
        "    /// For 64-bit architectures (like Apple Silicon), this should be `types::I64`.\n",
    );
    output.push_str("    pub fn signature(&self, pointer_type: types::Type) -> Signature {\n");
    output.push_str("        let mut sig = Signature::new(CallConv::SystemV);\n");
    output.push_str("        match self {\n");

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

    output.push_str("        }\n");
    output.push_str("        sig\n");
    output.push_str("    }\n\n");

    // Generate all() method
    output.push_str("    /// Get all builtin IDs.\n");
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
    output.push_str("    }\n");
    output.push_str("}\n\n");

    // Generate get_function_pointer()
    output.push_str("/// Get function pointer for a builtin (JIT mode only).\n");
    output.push_str("///\n");
    output.push_str("/// Returns the function pointer that can be registered with JITModule.\n");
    output.push_str("pub fn get_function_pointer(builtin: BuiltinId) -> *const u8 {\n");

    // Collect unique import paths
    // Import path: what goes in `use lp_glsl_builtins::builtins::{...};`
    use std::collections::HashSet;
    let mut import_paths: HashSet<String> = HashSet::new();

    for builtin in builtins {
        let components: Vec<&str> = builtin.module_path.split("::").collect();

        let import_path = if components[0] == "q32" {
            "q32".to_string()
        } else if components.len() >= 2 && components[0] == "lpfx" {
            // lpfx modules: import is "lpfx::{second_component}"
            format!("lpfx::{}", components[1])
        } else {
            // Fallback: use module_path as-is
            builtin.module_path.clone()
        };

        import_paths.insert(import_path);
    }

    // Generate imports
    let mut imports: Vec<String> = import_paths.into_iter().collect();
    imports.sort();

    if !imports.is_empty() {
        output.push_str(&format!(
            "    use lp_glsl_builtins::builtins::{{{}}};\n",
            imports.join(", ")
        ));
    }

    output.push_str("    match builtin {\n");
    if builtins.is_empty() {
        output.push_str("        BuiltinId::_Placeholder => core::ptr::null(),\n");
    } else {
        for builtin in builtins {
            // Compute usage path from module_path
            // Usage path is what comes before `::function_name` in the match
            // module_path includes file_name: "q32::acos", "lpfx::hash", "lpfx::simplex::simplex1_q32"
            let components: Vec<&str> = builtin.module_path.split("::").collect();
            let usage_path = if components[0] == "q32" {
                // q32 functions are re-exported at q32 level: q32::__lp_q32_acos
                "q32".to_string()
            } else if components.len() >= 2 && components[0] == "lpfx" {
                // lpfx functions: usage is everything after "lpfx"
                // e.g., "lpfx::simplex::simplex1_q32" -> "simplex::simplex1_q32"
                components[1..].join("::")
            } else {
                // Fallback: use module_path as-is
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
    output.push_str(
        "pub fn declare_builtins<M: Module>(module: &mut M, pointer_type: types::Type) -> Result<(), GlslError> {\n",
    );
    output.push_str("    for builtin in BuiltinId::all() {\n");
    output.push_str("        let name = builtin.name();\n");
    output.push_str("        let sig = builtin.signature(pointer_type);\n\n");
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
    output
        .push_str("pub fn declare_for_jit<M: Module>(module: &mut M, pointer_type: types::Type) -> Result<(), GlslError> {\n");
    output.push_str("    declare_builtins(module, pointer_type)\n");
    output.push_str("}\n\n");

    output.push_str("/// Declare builtin functions as external symbols for emulator mode.\n");
    output.push_str("///\n");
    output.push_str(
        "/// This declares all builtins as external symbols (Linkage::Import) that will\n",
    );
    output.push_str("/// be resolved by the linker when linking the static library.\n");
    output.push_str("///\n");
    output.push_str("/// `pointer_type` is the native pointer type for the target architecture.\n");
    output.push_str(
        "pub fn declare_for_emulator<M: Module>(module: &mut M, pointer_type: types::Type) -> Result<(), GlslError> {\n",
    );
    output.push_str("    declare_builtins(module, pointer_type)\n");
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

            // Derive import path from module_path structure
            // module_path includes file_name: "q32::sqrt", "lpfx::hash", "lpfx::simplex::simplex1_q32"
            // Import path should stop at parent directory (remove last component)
            let components: Vec<&str> = module_path.split("::").collect();

            let (import_path, function_prefix) = if components.len() == 1 {
                // Single component: "q32" (shouldn't happen with file_name, but handle it)
                (
                    format!("lp_glsl_builtins::builtins::{}", components[0]),
                    None,
                )
            } else if components[0] == "q32" {
                // q32::file_name -> import from q32, function is directly accessible
                ("lp_glsl_builtins::builtins::q32".to_string(), None)
            } else {
                // lpfx::hash or lpfx::simplex::simplex1_q32
                // Import path is everything except the last component (file_name)
                let import_components = &components[..components.len() - 1];
                let import_path_str = import_components.join("::");
                let import_path = format!("lp_glsl_builtins::builtins::{}", import_path_str);

                // Function prefix is the last component (file_name/submodule)
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

fn generate_mod_rs(path: &Path, builtins: &[BuiltinInfo]) {
    let header = r#"//! This file is AUTO-GENERATED. Do not edit manually.
//!
//! To regenerate this file, run:
//!     cargo run --bin lp-glsl-builtins-gen-app --manifest-path lp-glsl/lp-glsl-builtins-gen-app/Cargo.toml
//!
//! Or use the build script:
//!     scripts/build-builtins.sh

"#;

    let mut output = String::from(header);
    output.push_str("//! Fixed-point 16.16 arithmetic builtins.\n");
    output.push_str("//!\n");
    output.push_str("//! Functions operate on i32 values representing fixed-point numbers\n");
    output.push_str("//! with 16 bits of fractional precision.\n");
    output.push('\n');

    // Generate mod declarations (deduplicate by file name)
    let mut seen_files = std::collections::HashSet::new();
    for builtin in builtins {
        if seen_files.insert(&builtin.file_name) {
            output.push_str(&format!("mod {};\n", builtin.file_name));
        }
    }
    output.push('\n');

    // Generate pub use statements
    for builtin in builtins {
        output.push_str(&format!(
            "pub use {}::{};\n",
            builtin.file_name, builtin.function_name
        ));
    }

    fs::write(path, output).expect("Failed to write mod.rs");
}

fn generate_testcase_mapping(path: &Path, builtins: &[BuiltinInfo]) {
    // Read existing file
    let content = fs::read_to_string(path).expect("Failed to read math.rs");

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
            // Skip all LPFX functions - they are handled via proper lookup chain
            // (name -> BuiltinId -> LpfxFn -> q32_impl) instead of string matching
            if builtin.module_path.starts_with("lpfx::") {
                continue;
            }
            // Not an LPFX function or lookup failed - treat as regular q32 function
            // Regular q32 functions
            let base_name = strip_function_prefix(&builtin.symbol_name);

            // Generate C math function name (e.g., lp_q32_sinf)
            let c_name = format!("{}f", base_name);

            // Generate intrinsic name - for q32 functions, use the symbol name directly
            // (e.g., __lp_q32_sin)
            let intrinsic_name = builtin.symbol_name.clone();

            // Extract standard C math function name (e.g., "sinf" from "__lp_q32_sin")
            // Pattern: __lp_q32_<name> -> <name>f
            let standard_c_name =
                if let Some(name_part) = builtin.symbol_name.strip_prefix("__lp_q32_") {
                    format!("{}f", name_part)
                } else {
                    String::new()
                };

            // Special case: GLSL's mod() compiles to fmodf, not modf
            // Check if function name ends with _mod (e.g., __lp_q32_mod)
            let additional_names = if builtin.symbol_name.ends_with("_mod")
                || builtin.symbol_name.ends_with("_mod\"")
            {
                " | \"fmodf\"".to_string()
            } else if !standard_c_name.is_empty() {
                // Add standard C math function name (e.g., "sinf", "cosf")
                format!(" | \"{}\"", standard_c_name)
            } else {
                String::new()
            };

            new_function.push_str(&format!(
                "        (\"{}\" | \"{}\"{}, {}) => Some(BuiltinId::{}),\n",
                c_name, intrinsic_name, additional_names, builtin.param_count, builtin.enum_variant
            ));
        }
    }

    new_function.push_str("        _ => None,\n");
    new_function.push_str("    }\n");
    new_function.push_str("}\n");

    let new_content = format!("{}{}{}", before, new_function, after);
    fs::write(path, new_content).expect("Failed to write math.rs");
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
