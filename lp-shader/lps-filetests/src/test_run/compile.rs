//! Compile GLSL source for a specific target (one LPVM module per file).

use crate::targets::Target;
use lp_riscv_emu::LogLevel;
use lpir::CompilerConfig;
use std::sync::Mutex;

use super::filetest_lpvm::CompiledShader;

/// Suite-level `--force-opt` / `LPS_FILETEST_FORCE_OPT` overrides, installed for the duration of
/// [`crate::run`] so [`build_compiler_config`] can apply them after per-file `compile-opt`
/// directives without threading through `run_detail`.
static FORCED_COMPILER_OPTS: Mutex<Option<Vec<(String, String)>>> = Mutex::new(None);

/// RAII: installs [`FORCED_COMPILER_OPTS`] for the lifetime of [`crate::run`].
pub(crate) struct ForceCompilerOptsGuard;

impl ForceCompilerOptsGuard {
    pub(crate) fn install(opts: &[(String, String)]) -> Self {
        *FORCED_COMPILER_OPTS.lock().expect("forced compiler opts mutex poisoned") =
            Some(opts.to_vec());
        Self
    }
}

impl Drop for ForceCompilerOptsGuard {
    fn drop(&mut self) {
        *FORCED_COMPILER_OPTS.lock().expect("forced compiler opts mutex poisoned") = None;
    }
}

/// Build [`CompilerConfig`] from filetest `compile-opt` overrides (validated keys/values), then
/// suite-level force overrides (if any) so they win over per-file directives.
pub fn build_compiler_config(overrides: &[(String, String)]) -> anyhow::Result<CompilerConfig> {
    let mut c = CompilerConfig::default();
    for (k, v) in overrides {
        c.apply(k.trim(), v.trim())
            .map_err(|e| anyhow::anyhow!("compile-opt ({k}): {e}"))?;
    }
    if let Some(forced) = FORCED_COMPILER_OPTS
        .lock()
        .expect("forced compiler opts mutex poisoned")
        .as_ref()
    {
        for (k, v) in forced {
            c.apply(k.trim(), v.trim())
                .map_err(|e| anyhow::anyhow!("--force-opt ({k}): {e}"))?;
        }
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
