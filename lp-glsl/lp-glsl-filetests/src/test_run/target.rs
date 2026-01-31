//! Target value parsing (riscv32.q32 -> RunMode/DecimalFormat).

use anyhow::Result;
use lp_glsl_compiler::{DecimalFormat, RunMode};

/// Default maximum memory for emulator (in bytes).
const DEFAULT_MAX_MEMORY: usize = 1024 * 1024; // 1MB

/// Default stack size for emulator (in bytes).
const DEFAULT_STACK_SIZE: usize = 64 * 1024; // 64KB

/// Default maximum instructions for emulator.
const DEFAULT_MAX_INSTRUCTIONS: u64 = 1_000_000;

/// Parse target string (e.g., "riscv32.q32") into run mode and decimal format.
pub fn parse_target(target: &str) -> Result<(RunMode, DecimalFormat)> {
    let parts: Vec<&str> = target.split('.').collect();
    if parts.len() != 2 {
        anyhow::bail!("invalid target format: expected '<arch>.<format>', got '{target}'");
    }

    let arch = parts[0];
    let format = parts[1];

    let run_mode = match arch {
        "riscv32" => RunMode::Emulator {
            max_memory: DEFAULT_MAX_MEMORY,
            stack_size: DEFAULT_STACK_SIZE,
            max_instructions: DEFAULT_MAX_INSTRUCTIONS,
            log_level: None, // Will be set by caller based on output mode
        },
        _ => anyhow::bail!("unsupported architecture: {arch}"),
    };

    let decimal_format = match format {
        "q32" => DecimalFormat::Q32,
        "float" => DecimalFormat::Float,
        _ => anyhow::bail!("unsupported format: {format}"),
    };

    Ok((run_mode, decimal_format))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_target_riscv32_q32() {
        let (run_mode, format) = parse_target("riscv32.q32").unwrap();
        assert!(matches!(run_mode, RunMode::Emulator { .. }));
        assert_eq!(format, DecimalFormat::Q32);
    }

    #[test]
    fn test_parse_target_riscv32_float() {
        let (run_mode, format) = parse_target("riscv32.float").unwrap();
        assert!(matches!(run_mode, RunMode::Emulator { .. }));
        assert_eq!(format, DecimalFormat::Float);
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
