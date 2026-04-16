//! Compile GLSL source for a specific target (one LPVM module per file).

use crate::targets::Target;
use lp_riscv_emu::LogLevel;

use super::filetest_lpvm::CompiledShader;

/// Compile GLSL source for the given target.
pub fn compile_for_target(
    source: &str,
    target: &Target,
    _relative_path: &str,
    log_level: LogLevel,
) -> anyhow::Result<CompiledShader> {
    CompiledShader::compile_glsl(source, target, log_level)
}
