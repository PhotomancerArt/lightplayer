use anyhow::Result;
use cranelift_object::ObjectModule;
use lp_glsl_compiler::backend::module::gl_module::GlModule;
use lp_glsl_compiler::backend::q32::{FixedPointFormat, Q32Options};
use lp_glsl_compiler::backend::target::Target;
use lp_glsl_compiler::frontend::codegen::numeric::{FloatStrategy, NumericMode, Q32Strategy};
use lp_glsl_compiler::{DEFAULT_MAX_ERRORS, GlslCompiler};

pub fn compile_and_transform(
    glsl_source: &str,
    _format: FixedPointFormat,
) -> Result<(GlModule<ObjectModule>, GlModule<ObjectModule>)> {
    let target = Target::riscv32_emulator()
        .map_err(|e| anyhow::anyhow!("Failed to create target: {}", e))?;

    let mut compiler_before = GlslCompiler::new();
    let module_before = compiler_before
        .compile_to_gl_module_object(
            glsl_source,
            target.clone(),
            DEFAULT_MAX_ERRORS,
            NumericMode::Float(FloatStrategy),
        )
        .map_err(|e| anyhow::anyhow!("Failed to compile GLSL: {}", e))?;

    let mut compiler_after = GlslCompiler::new();
    let module_after = compiler_after
        .compile_to_gl_module_object(
            glsl_source,
            target,
            DEFAULT_MAX_ERRORS,
            NumericMode::Q32(Q32Strategy::new(Q32Options::default())),
        )
        .map_err(|e| anyhow::anyhow!("Failed to compile GLSL (Q32): {}", e))?;

    Ok((module_before, module_after))
}
