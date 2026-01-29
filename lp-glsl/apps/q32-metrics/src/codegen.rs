use anyhow::Result;
use cranelift_codegen::ir::Function;
use cranelift_module::FuncId;
use cranelift_object::ObjectModule;
use hashbrown::HashMap;
use lp_glsl_compiler::backend::module::gl_module::GlModule;
use lp_glsl_compiler::backend::util::clif_format::format_function;
use std::fs;
use std::path::Path;

/// Compile function and extract vcode and assembly
fn compile_function_and_extract(
    module: &mut GlModule<ObjectModule>,
    name: &str,
    func: Function,
    func_id: FuncId,
) -> Result<(Option<String>, Option<String>)> {
    module
        .compile_function_and_extract_codegen(name, func, func_id)
        .map_err(|e| anyhow::anyhow!("Failed to compile function {}: {}", name, e))
}

/// Write CLIF, vcode, and assembly files for all functions
/// Returns a map of function names to (vcode_size, assembly_size)
pub fn write_codegen_files(
    test_dir: &Path,
    module_before: &mut GlModule<ObjectModule>,
    module_after: &mut GlModule<ObjectModule>,
    verbose: bool,
) -> Result<HashMap<String, (usize, usize)>> {
    // Build name mappings for CLIF formatting
    let mut name_mapping_before: HashMap<String, String> = HashMap::new();
    for (name, gl_func) in &module_before.fns {
        name_mapping_before.insert(gl_func.func_id.as_u32().to_string(), name.clone());
    }

    let mut name_mapping_after: HashMap<String, String> = HashMap::new();
    for (name, gl_func) in &module_after.fns {
        name_mapping_after.insert(gl_func.func_id.as_u32().to_string(), name.clone());
    }

    // Sort function names for deterministic output
    let mut func_names: Vec<String> = module_before.fns.keys().cloned().collect();
    func_names.sort();

    let mut vcode_assembly_sizes: HashMap<String, (usize, usize)> = HashMap::new();

    for name in &func_names {
        if let Some(gl_func_before) = module_before.fns.get(name)
            && let Some(gl_func_after) = module_after.fns.get(name)
        {
            // Write CLIF files (existing logic)
            let clif_before = format_function(&gl_func_before.function, name, &name_mapping_before)
                .map_err(|e| {
                    anyhow::anyhow!("Failed to format function {} (before): {}", name, e)
                })?;
            let clif_after = format_function(&gl_func_after.function, name, &name_mapping_after)
                .map_err(|e| {
                    anyhow::anyhow!("Failed to format function {} (after): {}", name, e)
                })?;

            let pre_clif_file = test_dir.join(format!("{}.pre.clif", name));
            let post_clif_file = test_dir.join(format!("{}.post.clif", name));

            fs::write(&pre_clif_file, &clif_before).map_err(|e| {
                anyhow::anyhow!("Failed to write {}: {}", pre_clif_file.display(), e)
            })?;
            fs::write(&post_clif_file, &clif_after).map_err(|e| {
                anyhow::anyhow!("Failed to write {}: {}", post_clif_file.display(), e)
            })?;

            // Only compile and extract vcode/assembly for after transform
            // Pre-transform code has float operations that we can't lower to RISC-V32
            let (vcode_after, asm_after) = compile_function_and_extract(
                module_after,
                name,
                gl_func_after.function.clone(),
                gl_func_after.func_id,
            )?;

            // Write vcode file (only post-transform)
            if let Some(ref vcode) = vcode_after {
                let vcode_file = test_dir.join(format!("{}.post.vcode", name));
                fs::write(&vcode_file, vcode).map_err(|e| {
                    anyhow::anyhow!("Failed to write {}: {}", vcode_file.display(), e)
                })?;
            }

            // Write assembly file (only post-transform)
            if let Some(ref asm) = asm_after {
                let asm_file = test_dir.join(format!("{}.post.s", name));
                fs::write(&asm_file, asm).map_err(|e| {
                    anyhow::anyhow!("Failed to write {}: {}", asm_file.display(), e)
                })?;
            }

            // Calculate sizes for statistics (use after transform sizes)
            let vcode_size = vcode_after.as_ref().map(|s| s.len()).unwrap_or(0);
            let assembly_size = asm_after.as_ref().map(|s| s.len()).unwrap_or(0);
            vcode_assembly_sizes.insert(name.clone(), (vcode_size, assembly_size));

            if verbose {
                eprintln!(
                    "  Wrote {}.pre.clif, {}.post.clif, {}.post.vcode, and {}.post.s",
                    name, name, name, name
                );
            }
        }
    }

    Ok(vcode_assembly_sizes)
}
