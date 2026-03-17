//! Target value parsing (riscv32.q32, wasm32.q32 -> RunMode/FloatMode or Wasm).

use anyhow::Result;
use lp_glsl_cranelift::{FloatMode, RunMode};

/// Default maximum memory for emulator (in bytes).
const DEFAULT_MAX_MEMORY: usize = 1024 * 1024; // 1MB

/// Default stack size for emulator (in bytes).
const DEFAULT_STACK_SIZE: usize = 64 * 1024; // 64KB

/// Default maximum instructions for emulator.
const DEFAULT_MAX_INSTRUCTIONS: u64 = 1_000_000;

/// Target backend for filetest execution.
#[derive(Clone)]
pub enum FiletestTarget {
    /// Cranelift RISC-V 32 emulator.
    Cranelift {
        /// Run mode (emulator config).
        run_mode: RunMode,
        /// Numeric format (Q32 or Float).
        float_mode: FloatMode,
    },
    /// WASM via wasmtime.
    Wasm {
        /// Numeric format (Q32 or Float).
        float_mode: FloatMode,
    },
}

/// Parse target string (e.g., "riscv32.q32", "wasm32.q32") into FiletestTarget.
pub fn parse_target(target: &str) -> Result<FiletestTarget> {
    let parts: Vec<&str> = target.split('.').collect();
    if parts.len() != 2 {
        anyhow::bail!("invalid target format: expected '<arch>.<format>', got '{target}'");
    }

    let arch = parts[0];
    let format = parts[1];

    let float_mode = match format {
        "q32" => FloatMode::Q32,
        "float" => FloatMode::Float,
        _ => anyhow::bail!("unsupported format: {format}"),
    };

    let target = match arch {
        "riscv32" => FiletestTarget::Cranelift {
            run_mode: RunMode::Emulator {
                max_memory: DEFAULT_MAX_MEMORY,
                stack_size: DEFAULT_STACK_SIZE,
                max_instructions: DEFAULT_MAX_INSTRUCTIONS,
                log_level: None, // Will be set by caller based on output mode
            },
            float_mode,
        },
        "wasm32" => FiletestTarget::Wasm { float_mode },
        _ => anyhow::bail!("unsupported architecture: {arch}"),
    };

    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_target_riscv32_q32() {
        let target = parse_target("riscv32.q32").unwrap();
        match &target {
            FiletestTarget::Cranelift {
                run_mode,
                float_mode,
            } => {
                assert!(matches!(run_mode, RunMode::Emulator { .. }));
                assert_eq!(*float_mode, FloatMode::Q32);
            }
            _ => panic!("expected Cranelift"),
        }
    }

    #[test]
    fn test_parse_target_riscv32_float() {
        let target = parse_target("riscv32.float").unwrap();
        match &target {
            FiletestTarget::Cranelift {
                run_mode,
                float_mode,
            } => {
                assert!(matches!(run_mode, RunMode::Emulator { .. }));
                assert_eq!(*float_mode, FloatMode::Float);
            }
            _ => panic!("expected Cranelift"),
        }
    }

    #[test]
    fn test_parse_target_wasm32_q32() {
        let target = parse_target("wasm32.q32").unwrap();
        match &target {
            FiletestTarget::Wasm { float_mode } => {
                assert_eq!(*float_mode, FloatMode::Q32);
            }
            _ => panic!("expected Wasm"),
        }
    }

    #[test]
    fn test_parse_target_invalid_format() {
        assert!(parse_target("riscv32.invalid").is_err());
    }

    #[test]
    fn test_parse_target_invalid_arch() {
        assert!(parse_target("x86_64.q32").is_err());
    }

    #[test]
    fn test_parse_target_invalid_structure() {
        assert!(parse_target("riscv32").is_err());
        assert!(parse_target("riscv32.q32.extra").is_err());
    }
}
