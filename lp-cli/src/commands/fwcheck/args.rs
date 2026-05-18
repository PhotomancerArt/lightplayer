use clap::{Args, Subcommand, ValueEnum};

#[derive(Debug, Args)]
pub struct FwcheckCli {
    #[command(subcommand)]
    pub command: FwcheckCommand,
}

#[derive(Debug, Subcommand)]
pub enum FwcheckCommand {
    /// List known firmware checks.
    List,
    /// Resolve and print the ESP32 serial port.
    Port(FwcheckPortArgs),
    /// Build, flash/run, capture, and summarize one firmware check.
    Run(FwcheckRunArgs),
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum FwcheckTargetArg {
    #[value(name = "esp32c6")]
    Esp32C6,
    FwEmu,
}

#[derive(Debug, Args)]
pub struct FwcheckRunArgs {
    /// Target to run on.
    #[arg(value_enum)]
    pub target: FwcheckTargetArg,
    /// Check slug, for example `shader-compile-stress`.
    pub check: String,
    /// Optional serial port override for hardware targets.
    #[arg(long)]
    pub port: Option<String>,
    /// Optional note appended to the trace directory name.
    #[arg(long)]
    pub note: Option<String>,
    /// Timeout in seconds while waiting for the done marker.
    #[arg(long, default_value_t = 120)]
    pub timeout_secs: u64,
    /// Stream raw build, flash, and firmware output while the check runs.
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(Debug, Args)]
pub struct FwcheckPortArgs {
    /// Optional serial port override.
    #[arg(long)]
    pub port: Option<String>,
}
