//! Discovery of LPFX functions with #[lpfx_impl] attributes

use std::fs;
use std::path::{Path, PathBuf};
use syn::{Item, ItemFn, parse_file};
use walkdir::WalkDir;

use crate::lpfx::errors::LpfxCodegenError;

/// Information about a discovered LPFX function
#[derive(Debug, Clone)]
pub struct LpfxFunctionInfo {
    /// Rust function name (e.g., "__lpfx_snoise3_f32")
    pub rust_fn_name: String,
    /// BuiltinId enum variant name (e.g., "LpfxSnoise3Float")
    pub builtin_id_variant: String,
    /// File path where function is defined
    pub file_path: PathBuf,
    /// Whether function has #[lpfx_impl] attribute (will be parsed in next phase)
    pub has_lpfx_impl_attr: bool,
}

/// Discover all LPFX functions in the given directory
pub fn discover_lpfx_functions(dir: &Path) -> Result<Vec<LpfxFunctionInfo>, LpfxCodegenError> {
    let mut functions: Vec<LpfxFunctionInfo> = Vec::new();

    for entry in WalkDir::new(dir) {
        let entry = entry.map_err(|e| LpfxCodegenError::AttributeParseError {
            function_name: String::new(),
            file_path: String::new(),
            error: format!("Failed to walk directory: {}", e),
        })?;

        let path = entry.path();

        if path.extension() != Some(std::ffi::OsStr::new("rs")) {
            continue;
        }

        let file_name = path.file_stem().and_then(|s| s.to_str()).ok_or_else(|| {
            LpfxCodegenError::AttributeParseError {
                function_name: String::new(),
                file_path: path.display().to_string(),
                error: "Invalid file name".to_string(),
            }
        })?;

        // Skip mod.rs and test_helpers.rs
        if file_name == "mod" || file_name == "test_helpers" {
            continue;
        }

        let content =
            fs::read_to_string(path).map_err(|e| LpfxCodegenError::AttributeParseError {
                function_name: String::new(),
                file_path: path.display().to_string(),
                error: format!("Failed to read file: {}", e),
            })?;

        let ast = parse_file(&content).map_err(|e| LpfxCodegenError::AttributeParseError {
            function_name: String::new(),
            file_path: path.display().to_string(),
            error: format!("Failed to parse file: {}", e),
        })?;

        for item in ast.items {
            if let Item::Fn(func) = item
                && let Some(info) = extract_lpfx_function(&func, path)
            {
                // Skip if already added
                if !functions
                    .iter()
                    .any(|f| f.rust_fn_name == info.rust_fn_name)
                {
                    functions.push(info);
                }
            }
        }
    }

    // Sort by function name for consistent output
    functions.sort_by(|a, b| a.rust_fn_name.cmp(&b.rust_fn_name));

    Ok(functions)
}

/// Extract LPFX function information from a function item
fn extract_lpfx_function(func: &ItemFn, file_path: &Path) -> Option<LpfxFunctionInfo> {
    let func_name = func.sig.ident.to_string();

    // Only process functions that start with __lpfx_ (LPFX convention)
    if !func_name.starts_with("__lpfx_") {
        return None;
    }

    // Check for #[lpfx_impl] or #[lpfx_impl_macro::lpfx_impl] attribute
    let has_lpfx_impl_attr = func.attrs.iter().any(|attr| {
        let path = attr.path();
        if path.is_ident("lpfx_impl") {
            return true;
        }
        // Check if last segment is "lpfx_impl"
        if let Some(last_seg) = path.segments.last() {
            return last_seg.ident == "lpfx_impl";
        }
        false
    });

    // Derive BuiltinId enum variant name by:
    // 1. Strip leading __
    // 2. Split by _ and capitalize each word
    // 3. Join together
    // Examples:
    // __lpfx_hash_1 -> LpfxHash1
    // __lpfx_snoise1_f32 -> LpfxSnoise1F32
    // __lpfx_snoise1_q32 -> LpfxSnoise1Q32
    let name_without_prefix = func_name.strip_prefix("__").unwrap();
    let builtin_id_variant = name_without_prefix
        .split('_')
        .map(|s| {
            let mut chars = s.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<String>();

    Some(LpfxFunctionInfo {
        rust_fn_name: func_name,
        builtin_id_variant,
        file_path: file_path.to_path_buf(),
        has_lpfx_impl_attr,
    })
}
