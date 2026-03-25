//! Compile GLSL source for a specific target.

use crate::target::{Backend, FloatMode, Target};
use lp_glsl_exec::GlslExecutable;
use lp_glsl_wasm::WasmOptions;
use lp_riscv_emu::LogLevel;

use super::lpir_jit_executable::LpirJitExecutable;
use super::lpir_rv32_executable::LpirRv32Executable;
use super::wasm_runner::WasmExecutable;

/// Maximum execution steps before timeout. Used by both emulator and WASM backends.
pub(crate) const DEFAULT_MAX_INSTRUCTIONS: u64 = 1_000_000;

/// Map filetest FloatMode to wasm FloatMode.
fn to_wasm_float_mode(fm: FloatMode) -> lp_glsl_naga::FloatMode {
    match fm {
        FloatMode::Q32 => lp_glsl_naga::FloatMode::Q32,
        FloatMode::F32 => lp_glsl_naga::FloatMode::F32,
    }
}

/// Compile GLSL source for the given target.
pub fn compile_for_target(
    source: &str,
    target: &Target,
    _relative_path: &str,
    _log_level: LogLevel,
) -> anyhow::Result<Box<dyn GlslExecutable>> {
    match target.backend {
        Backend::Jit => {
            let exec = LpirJitExecutable::try_new(source, target.float_mode)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            Ok(Box::new(exec))
        }
        Backend::Rv32 => {
            let exec = LpirRv32Executable::try_new(source, target.float_mode)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            Ok(Box::new(exec))
        }
        Backend::Wasm => {
            let options = WasmOptions {
                float_mode: to_wasm_float_mode(target.float_mode),
            };
            let exec =
                WasmExecutable::from_source(source, options).map_err(|e| anyhow::anyhow!("{e}"))?;
            Ok(Box::new(exec))
        }
    }
}
