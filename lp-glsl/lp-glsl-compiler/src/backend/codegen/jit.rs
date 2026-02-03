//! JIT codegen - build executable from GlModule<JITModule>

use crate::backend::module::gl_module::GlModule;
use crate::error::{ErrorCode, GlslError};
use crate::exec::jit::GlslJitModule;
use alloc::{format, string::String, vec::Vec};
use cranelift_jit::JITModule;
use cranelift_module::Module;
use hashbrown::HashMap;
use log;

/// Build JIT executable from GlModule<JITModule>
/// Called by GlModule<JITModule>::build_executable()
pub fn build_jit_executable(
    mut gl_module: GlModule<JITModule>,
) -> Result<GlslJitModule, GlslError> {
    // Builtin functions are already declared when the module was created

    // 1. Define all functions (compile them)
    // Collect function data first to avoid borrowing conflicts
    let funcs: Vec<(
        String,
        cranelift_codegen::ir::Function,
        cranelift_module::FuncId,
    )> = gl_module
        .fns
        .iter()
        .map(|(name, gl_func)| (name.clone(), gl_func.function.clone(), gl_func.func_id))
        .collect();

    for (name, func, func_id) in funcs {
        // Create context using immutable borrow
        let mut ctx = {
            let module_ref = gl_module.module_internal();
            module_ref.make_context()
        };
        ctx.func = func;
        // Define function using mutable borrow
        gl_module
            .module_mut_internal()
            .define_function(func_id, &mut ctx)
            .map_err(|e| {
                // Check if this is a verifier error by checking the error message
                // If it is, verify the function again to get detailed error messages
                let error_str = format!("{e}");
                let error_msg = if error_str.contains("Verifier errors") {
                    // It's a verifier error - verify the function again to get detailed errors
                    use cranelift_codegen::verifier::verify_function;
                    let module_ref = gl_module.module_internal();
                    let isa = module_ref.isa();

                    if let Err(verifier_errors) = verify_function(&ctx.func, isa) {
                        // Format verifier errors with the function IR for context
                        #[cfg(feature = "std")]
                        {
                            use cranelift_codegen::print_errors::pretty_verifier_error;
                            format!(
                                "Failed to define function '{}': Verifier errors\n\n{}",
                                name,
                                pretty_verifier_error(&ctx.func, None, verifier_errors)
                            )
                        }
                        #[cfg(not(feature = "std"))]
                        {
                            format!(
                                "Failed to define function '{}': Verifier errors\n\n{}",
                                name, verifier_errors
                            )
                        }
                    } else {
                        // Fallback if verification somehow succeeds
                        format!("Failed to define function '{name}': {e}")
                    }
                } else {
                    format!("Failed to define function '{name}': {e}")
                };

                GlslError::new(ErrorCode::E0400, error_msg)
            })?;
        // Clear context using immutable borrow
        {
            let module_ref = gl_module.module_internal();
            module_ref.clear_context(&mut ctx);
        }
    }

    // 2. Finalize definitions
    gl_module
        .module_mut_internal()
        .finalize_definitions()
        .map_err(|e| {
            GlslError::new(
                ErrorCode::E0400,
                format!("Failed to finalize definitions: {e}"),
            )
        })?;

    // 3. Extract function pointers
    let mut function_ptrs = HashMap::new();
    for (name, gl_func) in &gl_module.fns {
        let ptr = gl_module
            .module_internal()
            .get_finalized_function(gl_func.func_id);
        function_ptrs.insert(name.clone(), ptr);
    }

    // 3. Build signatures map from GlModule metadata
    let signatures = gl_module.glsl_signatures.clone();
    let mut cranelift_signatures = HashMap::new();
    for (name, gl_func) in &gl_module.fns {
        cranelift_signatures.insert(name.clone(), gl_func.clif_sig.clone());
    }

    // 4. Get target properties (requires mutable reference for ISA caching)
    let call_conv = gl_module
        .target
        .default_call_conv()
        .map_err(|e| GlslError::new(ErrorCode::E0400, format!("Failed to get call conv: {e}")))?;
    let pointer_type = gl_module.target.pointer_type().map_err(|e| {
        GlslError::new(ErrorCode::E0400, format!("Failed to get pointer type: {e}"))
    })?;

    // 5. Create GlslJitModule
    Ok(GlslJitModule {
        jit_module: gl_module.into_module(),
        function_ptrs,
        signatures,
        cranelift_signatures,
        call_conv,
        pointer_type,
    })
}

/// Build JIT executable from GlModule<JITModule> with aggressive memory optimization
///
/// This function frees CLIF IR immediately after each function compilation and drops
/// unused GlModule fields to minimize memory usage. Use this in memory-constrained
/// environments like embedded systems.
pub fn build_jit_executable_memory_optimized(
    mut gl_module: GlModule<JITModule>,
) -> Result<GlslJitModule, GlslError> {
    // Builtin functions are already declared when the module was created

    // 1. Extract signatures and target info early (before compilation)
    let signatures = gl_module.glsl_signatures.clone();
    let call_conv = gl_module
        .target
        .default_call_conv()
        .map_err(|e| GlslError::new(ErrorCode::E0400, format!("Failed to get call conv: {e}")))?;
    let pointer_type = gl_module.target.pointer_type().map_err(|e| {
        GlslError::new(ErrorCode::E0400, format!("Failed to get pointer type: {e}"))
    })?;

    // 2. Extract function metadata we'll need later (name, func_id, clif_sig)
    // This allows us to free the CLIF IR and drop the fns HashMap after compilation
    let func_metadata: Vec<(
        String,
        cranelift_module::FuncId,
        cranelift_codegen::ir::Signature,
    )> = gl_module
        .fns
        .iter()
        .map(|(name, gl_func)| (name.clone(), gl_func.func_id, gl_func.clif_sig.clone()))
        .collect();

    // 3. Build cranelift signatures map before freeing CLIF IR
    let mut cranelift_signatures = HashMap::new();
    for (name, _, sig) in &func_metadata {
        cranelift_signatures.insert(name.clone(), sig.clone());
    }

    // 4. Define all functions (compile them), freeing CLIF IR after each
    for (name, func_id, _) in &func_metadata {
        // Extract function IR for this function
        let func = gl_module
            .fns
            .get(name)
            .ok_or_else(|| {
                GlslError::new(
                    ErrorCode::E0400,
                    format!("Function '{name}' not found in module"),
                )
            })?
            .function
            .clone();

        // Create context and compile function
        let mut ctx = {
            let module_ref = gl_module.module_internal();
            module_ref.make_context()
        };
        ctx.func = func;
        gl_module
            .module_mut_internal()
            .define_function(*func_id, &mut ctx)
            .map_err(|e| {
                // Check if this is a verifier error by checking the error message
                // If it is, verify the function again to get detailed error messages
                let error_str = format!("{e}");
                let error_msg = if error_str.contains("Verifier errors") {
                    // It's a verifier error - verify the function again to get detailed errors
                    use cranelift_codegen::verifier::verify_function;
                    let module_ref = gl_module.module_internal();
                    let isa = module_ref.isa();

                    if let Err(verifier_errors) = verify_function(&ctx.func, isa) {
                        // Format verifier errors with the function IR for context
                        #[cfg(feature = "std")]
                        {
                            use cranelift_codegen::print_errors::pretty_verifier_error;
                            format!(
                                "Failed to define function '{}': Verifier errors\n\n{}",
                                name,
                                pretty_verifier_error(&ctx.func, None, verifier_errors)
                            )
                        }
                        #[cfg(not(feature = "std"))]
                        {
                            format!(
                                "Failed to define function '{}': Verifier errors\n\n{}",
                                name, verifier_errors
                            )
                        }
                    } else {
                        // Fallback if verification somehow succeeds
                        format!("Failed to define function '{name}': {e}")
                    }
                } else {
                    format!("Failed to define function '{name}': {e}")
                };

                GlslError::new(ErrorCode::E0400, error_msg)
            })?;
        {
            let module_ref = gl_module.module_internal();
            module_ref.clear_context(&mut ctx);
        }

        // Free CLIF IR for this function by removing it from the HashMap
        // We've already extracted what we need (func_id, clif_sig) in func_metadata
        gl_module.fns.remove(name);
    }

    // 5. Finalize definitions
    gl_module
        .module_mut_internal()
        .finalize_definitions()
        .map_err(|e| {
            GlslError::new(
                ErrorCode::E0400,
                format!("Failed to finalize definitions: {e}"),
            )
        })?;

    // 6. Extract function pointers using the stored func_ids
    let mut function_ptrs = HashMap::new();
    for (name, func_id, _) in &func_metadata {
        let ptr = gl_module.module_internal().get_finalized_function(*func_id);
        function_ptrs.insert(name.clone(), ptr);
    }

    // 7. Extract JITModule and drop the rest of GlModule
    // This frees: function_registry, source_text, source_loc_manager, source_map, and the now-empty fns HashMap
    let jit_module = gl_module.into_module();

    // 8. Create GlslJitModule
    Ok(GlslJitModule {
        jit_module,
        function_ptrs,
        signatures,
        cranelift_signatures,
        call_conv,
        pointer_type,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::module::gl_module::GlModule;
    use crate::backend::module::test_helpers::test_helpers::build_simple_function;
    use crate::backend::target::Target;
    use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature, types};
    use cranelift_codegen::isa::CallConv;
    use cranelift_module::Linkage;

    #[test]
    #[cfg(feature = "std")]
    fn test_build_jit_executable() {
        use crate::exec::executable::GlslExecutable;

        let target = Target::host_jit().unwrap();
        let mut gl_module = GlModule::new_jit(target).unwrap();

        // Build a simple function that returns 42
        let mut sig = Signature::new(CallConv::SystemV);
        sig.returns.push(AbiParam::new(types::I32));

        build_simple_function(&mut gl_module, "main", Linkage::Export, sig, |builder| {
            let val = builder.ins().iconst(types::I32, 42);
            builder.ins().return_(&[val]);
            Ok(())
        })
        .unwrap();

        // Build executable
        let mut executable = build_jit_executable(gl_module).unwrap();
        assert!(executable.function_ptrs.contains_key("main"));

        // Actually call the function and verify it returns 42
        let result = executable.call_i32("main", &[]).unwrap();
        assert_eq!(result, 42);
    }
}
