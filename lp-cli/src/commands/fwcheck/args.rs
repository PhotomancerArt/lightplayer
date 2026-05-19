use clap::{Args, Subcommand, ValueEnum};
use std::path::PathBuf;

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
    /// Build, flash, capture boot serial, push a project, and exit once it is running.
    Demo(FwcheckDemoArgs),
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
pub struct FwcheckDemoArgs {
    /// Target to run on.
    #[arg(value_enum)]
    pub target: FwcheckTargetArg,
    /// Project directory, or an example name such as `basic`.
    pub project: PathBuf,
    /// Optional serial port override for hardware targets.
    #[arg(long)]
    pub port: Option<String>,
    /// Optional note appended to the trace directory name.
    #[arg(long)]
    pub note: Option<String>,
    /// Firmware features, excluding `esp32c6` which is always added.
    #[arg(long, default_value = "server")]
    pub features: String,
    /// Timeout in seconds for the whole push/load/run sequence.
    #[arg(long, default_value_t = 120)]
    pub timeout_secs: u64,
    /// Seconds to keep capturing after project load before declaring success.
    #[arg(long, default_value_t = 3)]
    pub settle_secs: u64,
    /// Stream raw build and flash output while setup runs.
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(Debug, Args)]
pub struct FwcheckPortArgs {
    /// Optional serial port override.
    #[arg(long)]
    pub port: Option<String>,
}
