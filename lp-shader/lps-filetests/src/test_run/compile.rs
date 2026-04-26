//! Compile GLSL source for a specific target (one LPVM module per file).

use crate::targets::Target;
use lp_riscv_emu::LogLevel;
use lpir::CompilerConfig;
use lps_shared::TextureBindingSpec;
use std::collections::BTreeMap;
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
        *FORCED_COMPILER_OPTS
            .lock()
            .expect("forced compiler opts mutex poisoned") = Some(opts.to_vec());
        Self
    }
}

impl Drop for ForceCompilerOptsGuard {
    fn drop(&mut self) {
        *FORCED_COMPILER_OPTS
            .lock()
            .expect("forced compiler opts mutex poisoned") = None;
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
    texture_specs: &BTreeMap<String, TextureBindingSpec>,
) -> anyhow::Result<CompiledShader> {
    CompiledShader::compile_glsl(source, target, log_level, compiler_config, texture_specs)
}

#[cfg(test)]
mod texture_spec_compile_tests {
    use super::*;
    use crate::targets::{Backend, ExecMode, FloatMode, Isa, Target};
    use lp_riscv_emu::LogLevel;
    use lps_shared::{TextureFilter, TextureShapeHint, TextureStorageFormat, TextureWrap};

    fn jit_q32_target() -> Target {
        Target {
            backend: Backend::Jit,
            float_mode: FloatMode::Q32,
            isa: Isa::Native,
            exec_mode: ExecMode::Jit,
        }
    }

    fn sample_spec() -> TextureBindingSpec {
        TextureBindingSpec {
            format: TextureStorageFormat::Rgba16Unorm,
            filter: TextureFilter::Nearest,
            wrap_x: TextureWrap::ClampToEdge,
            wrap_y: TextureWrap::ClampToEdge,
            shape_hint: TextureShapeHint::General2D,
        }
    }

    #[test]
    fn compile_fails_when_sampler2d_without_texture_spec() {
        let glsl = r#"
float add(float a, float b) { return a + b; }
uniform sampler2D tex;
"#;
        let target = jit_q32_target();
        let cfg = CompilerConfig::default();
        let empty = BTreeMap::new();
        let err = match compile_for_target(glsl, &target, "", LogLevel::None, &cfg, &empty) {
            Err(e) => e,
            Ok(_) => panic!("expected texture spec validation error"),
        };
        let s = format!("{err:#}");
        assert!(
            s.contains("tex") && s.contains("no texture binding spec"),
            "{s}"
        );
    }

    #[test]
    fn compile_succeeds_when_sampler2d_has_matching_spec() {
        let glsl = r#"
float add(float a, float b) { return a + b; }
uniform sampler2D tex;
"#;
        let target = jit_q32_target();
        let cfg = CompilerConfig::default();
        let mut specs = BTreeMap::new();
        specs.insert(String::from("tex"), sample_spec());
        compile_for_target(glsl, &target, "", LogLevel::None, &cfg, &specs)
            .expect("compile with matching texture spec");
    }
}
