//! Target value parsing (riscv32.q32, wasm32.q32 -> RunMode/DecimalFormat or Wasm).

use anyhow::Result;
use lp_glsl_cranelift::{DecimalFormat, RunMode};

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
        decimal_format: DecimalFormat,
    },
    /// WASM via wasmtime.
    Wasm {
        /// Numeric format (Q32 or Float).
        decimal_format: DecimalFormat,
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

    let decimal_format = match format {
        "q32" => DecimalFormat::Q32,
        "float" => DecimalFormat::Float,
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
            decimal_format,
        },
        "wasm32" => FiletestTarget::Wasm { decimal_format },
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
                decimal_format,
            } => {
                assert!(matches!(run_mode, RunMode::Emulator { .. }));
                assert_eq!(*decimal_format, DecimalFormat::Q32);
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
                decimal_format,
            } => {
                assert!(matches!(run_mode, RunMode::Emulator { .. }));
                assert_eq!(*decimal_format, DecimalFormat::Float);
            }
            _ => panic!("expected Cranelift"),
        }
    }

    #[test]
    fn test_parse_target_wasm32_q32() {
        let target = parse_target("wasm32.q32").unwrap();
        match &target {
            FiletestTarget::Wasm { decimal_format } => {
                assert_eq!(*decimal_format, DecimalFormat::Q32);
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
