//! Compile GLSL source for a specific target (one LPVM module per file).

use crate::targets::Target;
use lp_riscv_emu::LogLevel;
use lpir::CompilerConfig;

use super::filetest_lpvm::CompiledShader;

/// Build [`CompilerConfig`] from filetest `compile-opt` overrides (validated keys/values).
pub fn build_compiler_config(overrides: &[(String, String)]) -> anyhow::Result<CompilerConfig> {
    let mut c = CompilerConfig::default();
    for (k, v) in overrides {
        c.apply(k.trim(), v.trim())
            .map_err(|e| anyhow::anyhow!("compile-opt {k}: {e}"))?;
    }
    Ok(c)
}

/// Compile GLSL source for the given target.
pub fn compile_for_target(
    source: &str,
    target: &Target,
    _relative_path: &str,
    log_level: LogLevel,
    compiler_config: &CompilerConfig,
) -> anyhow::Result<CompiledShader> {
    CompiledShader::compile_glsl(source, target, log_level, compiler_config)
}
