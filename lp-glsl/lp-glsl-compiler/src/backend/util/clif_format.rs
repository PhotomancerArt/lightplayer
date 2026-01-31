//! CLIF IR formatting utilities for debugging and diagnostics

use crate::backend::module::gl_module::GlModule;
use crate::error::GlslError;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use cranelift_codegen::ir::{ExternalName, UserFuncName};
use cranelift_codegen::write_function;
use cranelift_module::Module;
use hashbrown::HashMap;

/// Format a GlModule as CLIF text for debugging.
/// This produces a human-readable representation of all functions in the module.
#[cfg(feature = "std")]
pub fn format_clif_module<M: Module>(module: &GlModule<M>) -> Result<String, GlslError> {
    let mut result = String::new();

    // Build mapping from func_id string to function name for updating external references
    let mut name_mapping: HashMap<String, String> = HashMap::new();
    for (name, gl_func) in &module.fns {
        name_mapping.insert(gl_func.func_id.as_u32().to_string(), name.clone());
    }

    // Add user functions (excluding main)
    let mut user_funcs: Vec<_> = module
        .fns
        .iter()
        .filter(|(name, _)| *name != "main")
        .collect();
    // Sort by name for deterministic output
    user_funcs.sort_by_key(|(name, _)| *name);

    for (_name, gl_func) in user_funcs {
        let func_text = format_function(&gl_func.function, _name, &name_mapping)?;
        result.push_str(&func_text);
        result.push('\n');
    }

    // Add main function
    if let Some(main_func) = module.fns.get("main") {
        let main_text = format_function(&main_func.function, "main", &name_mapping)?;
        result.push_str(&main_text);
    }

    Ok(result)
}

/// Format a single function as CLIF text.
/// The function is cloned and its name is set to the provided name, and external function
/// references are updated to use testcase names.
pub fn format_function(
    func: &cranelift_codegen::ir::Function,
    name: &str,
    name_mapping: &HashMap<String, String>,
) -> Result<String, GlslError> {
    // Clone the function so we can modify it
    let mut func_clone = func.clone();

    // Set the function name to use testcase format (%name)
    func_clone.name = UserFuncName::testcase(name.as_bytes());

    // Update external function references to use testcase names
    // First collect the user_named_funcs mapping to avoid borrow conflicts
    let user_named_funcs: HashMap<_, _> = func_clone
        .params
        .user_named_funcs()
        .iter()
        .map(|(k, v)| (k, v.clone()))
        .collect();
    for (_, ext_func) in func_clone.dfg.ext_funcs.iter_mut() {
        if let ExternalName::User(user_name_ref) = &ext_func.name {
            // Look up the function name from the user_named_funcs
            if let Some(user_name) = user_named_funcs.get(user_name_ref) {
                // The user_name.index should correspond to the func_id
                // Look up the name in the mapping
                if let Some(func_name) = name_mapping.get(&user_name.index.to_string()) {
                    ext_func.name = ExternalName::testcase(func_name.as_bytes());
                }
            }
        } else if let ExternalName::TestCase(_) = &ext_func.name {
            // Already using testcase format, leave as is
        } else if let ExternalName::LibCall(_) = &ext_func.name {
            // LibCall names are already in % format, leave as is
        }
    }

    let mut buf = String::new();
    write_function(&mut buf, &func_clone).map_err(|e| {
        GlslError::new(
            crate::error::ErrorCode::E0400,
            format!("failed to write function: {e}"),
        )
    })?;

    // Return plain CLIF IR text (no comment prefixes)
    Ok(buf)
}
