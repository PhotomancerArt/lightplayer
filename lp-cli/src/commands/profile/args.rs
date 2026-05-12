use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

use super::mode::ProfileMode;

/// `lp-cli profile …` — run a session or `profile diff` (stub).
#[derive(Debug, Parser)]
#[command(
    name = "profile",
    about = "Run a profiling session or compare two profile directories."
)]
pub struct ProfileCli {
    #[command(subcommand)]
    pub subcommand: Option<ProfileSubcommand>,

    #[command(flatten)]
    pub run: ProfileArgs,
}

#[derive(Debug, Subcommand)]
pub enum ProfileSubcommand {
    /// Compare two profile directories (not yet implemented).
    Diff(ProfileDiffArgs),
    /// Explain one function in an existing cpu-profile.json.
    Function(ProfileFunctionArgs),
}

#[derive(Debug, Args)]
pub struct ProfileArgs {
    /// Workload directory (defaults to examples/basic).
    #[arg(default_value = "examples/basic")]
    pub dir: PathBuf,

    /// Collectors to enable (comma-separated). m2 supports: alloc, events, cpu.
    /// Default: cpu. (`events` is auto-included when `cpu` is enabled.)
    #[arg(long, default_value = "cpu", value_delimiter = ',')]
    pub collect: Vec<String>,

    /// Profile mode (state machine over the perf-event stream).
    #[arg(long, value_enum, default_value_t = ProfileMode::SteadyRender)]
    pub mode: ProfileMode,

    /// Cycle attribution model for the CPU collector.
    #[arg(long, value_enum, default_value_t = CycleModelArg::Esp32C6)]
    pub cycle_model: CycleModelArg,

    /// Safety cap on emulated cycles. The run terminates with exit
    /// code 0 and a warning if reached.
    #[arg(long, default_value_t = 200_000_000)]
    pub max_cycles: u64,

    /// Optional human-readable note appended to the profile dir.
    #[arg(long)]
    pub note: Option<String>,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum CycleModelArg {
    Esp32C6,
    Uniform,
}

impl CycleModelArg {
    pub fn label(self) -> &'static str {
        match self {
            Self::Esp32C6 => "esp32c6",
            Self::Uniform => "uniform",
        }
    }

    pub fn to_emu(self) -> lp_riscv_emu::CycleModel {
        match self {
            Self::Esp32C6 => lp_riscv_emu::CycleModel::Esp32C6,
            Self::Uniform => lp_riscv_emu::CycleModel::InstructionCount,
        }
    }
}

#[derive(Debug, Args)]
pub struct ProfileDiffArgs {
    pub a: PathBuf,
    pub b: PathBuf,
}

#[derive(Debug, Args)]
pub struct ProfileFunctionArgs {
    /// Profile output directory containing cpu-profile.json.
    pub dir: PathBuf,

    /// Function name substring to inspect.
    pub function: String,

    /// Match the full function name exactly.
    #[arg(long)]
    pub exact: bool,

    /// Maximum rows per section.
    #[arg(long, default_value_t = 20)]
    pub top: usize,

    /// Optional RV32 ELF for addr2line callsite locations.
    #[arg(long)]
    pub elf: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn default_collect_is_cpu() {
        let cli = ProfileCli::parse_from(["lp-cli", "examples/basic"]);
        assert_eq!(cli.run.collect, vec!["cpu".to_string()]);
    }

    #[test]
    fn default_cycle_model_is_esp32c6() {
        let cli = ProfileCli::parse_from(["lp-cli", "examples/basic"]);
        assert!(matches!(cli.run.cycle_model, CycleModelArg::Esp32C6));
    }

    #[test]
    fn cycle_model_uniform_parses() {
        let cli = ProfileCli::parse_from(["lp-cli", "examples/basic", "--cycle-model", "uniform"]);
        assert!(matches!(cli.run.cycle_model, CycleModelArg::Uniform));
    }
}
