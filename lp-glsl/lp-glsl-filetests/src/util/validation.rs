//! CLIF validation utilities.

use anyhow::{Context, Result};
use cranelift_codegen::isa::TargetIsa;
use cranelift_codegen::print_errors::pretty_verifier_error;
use cranelift_codegen::verify_function;
use cranelift_object::ObjectModule;
use lp_glsl_compiler::backend::module::gl_module::GlModule;

/// Validate all functions in a GlModule
pub fn validate_clif_module(module: &GlModule<ObjectModule>, isa: &dyn TargetIsa) -> Result<()> {
    // Validate user functions (excluding main)
    for (name, gl_func) in &module.fns {
        if name != "main" {
            verify_function(&gl_func.function, isa)
                .map_err(|errors| {
                    anyhow::anyhow!(
                        "CLIF validation failed for function '{}':\n{}",
                        name,
                        pretty_verifier_error(&gl_func.function, None, errors)
                    )
                })
                .with_context(|| format!("failed to validate function '{name}'"))?;
        }
    }

    // Validate main function
    if let Some(main_func) = module.fns.get("main") {
        verify_function(&main_func.function, isa)
            .map_err(|errors| {
                anyhow::anyhow!(
                    "CLIF validation failed for main function:\n{}",
                    pretty_verifier_error(&main_func.function, None, errors)
                )
            })
            .with_context(|| "failed to validate main function")?;
    }

    Ok(())
}
