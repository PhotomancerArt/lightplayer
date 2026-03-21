//! Compile GLSL source for a specific target.

use crate::target::{Backend, FloatMode, Target};
use lp_glsl_cranelift::glsl_emu_riscv32_with_metadata;
use lp_glsl_cranelift::{GlslOptions, RunMode};
use lp_glsl_wasm::WasmOptions;
use lp_riscv_emu::LogLevel;

const DEFAULT_MAX_MEMORY: usize = 1024 * 1024;
const DEFAULT_STACK_SIZE: usize = 64 * 1024;

/// Maximum execution steps before timeout. Used by both emulator and WASM backends.
pub(crate) const DEFAULT_MAX_INSTRUCTIONS: u64 = 1_000_000;

/// Map filetest FloatMode to cranelift FloatMode.
fn to_cranelift_float_mode(fm: FloatMode) -> lp_glsl_cranelift::FloatMode {
    match fm {
        FloatMode::Q32 => lp_glsl_cranelift::FloatMode::Q32,
        FloatMode::F32 => lp_glsl_cranelift::FloatMode::Float,
    }
}

/// Map filetest FloatMode to wasm FloatMode.
fn to_wasm_float_mode(fm: FloatMode) -> lp_glsl_naga::FloatMode {
    match fm {
        FloatMode::Q32 => lp_glsl_naga::FloatMode::Q32,
        FloatMode::F32 => lp_glsl_naga::FloatMode::Float,
    }
}

/// Compile GLSL source for the given target.
pub fn compile_for_target(
    source: &str,
    target: &Target,
    relative_path: &str,
    log_level: LogLevel,
) -> anyhow::Result<Box<dyn lp_glsl_cranelift::GlslExecutable>> {
    match target.backend {
        Backend::Cranelift => {
            let run_mode = RunMode::Emulator {
                max_memory: DEFAULT_MAX_MEMORY,
                stack_size: DEFAULT_STACK_SIZE,
                max_instructions: DEFAULT_MAX_INSTRUCTIONS,
                log_level: Some(log_level),
            };
            let options = GlslOptions {
                run_mode,
                float_mode: to_cranelift_float_mode(target.float_mode),
                q32_opts: lp_glsl_cranelift::Q32Options::default(),
                memory_optimized: false,
                target_override: None,
                max_errors: lp_glsl_cranelift::DEFAULT_MAX_ERRORS,
            };
            let exec =
                glsl_emu_riscv32_with_metadata(source, options, Some(relative_path.to_string()))
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
            Ok(exec)
        }
        Backend::Wasm => {
            let options = WasmOptions {
                float_mode: to_wasm_float_mode(target.float_mode),
            };
            let exec = crate::test_run::wasm_runner::WasmExecutable::from_source(source, options)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            Ok(Box::new(exec))
        }
    }
}
