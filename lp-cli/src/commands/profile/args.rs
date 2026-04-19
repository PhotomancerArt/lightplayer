use clap::{Args, Parser, Subcommand};
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
}

#[derive(Debug, Args)]
pub struct ProfileArgs {
    /// Workload directory (defaults to examples/basic).
    #[arg(default_value = "examples/basic")]
    pub dir: PathBuf,

    /// Collectors to enable (comma-separated). m1 supports: alloc, events.
    /// Default: events. (events is implicitly fed to the mode gate even
    /// when not in this list, but events.jsonl is only written when
    /// "events" is included here.)
    #[arg(long, default_value = "events", value_delimiter = ',')]
    pub collect: Vec<String>,

    /// Profile mode (state machine over the perf-event stream).
    #[arg(long, value_enum, default_value_t = ProfileMode::SteadyRender)]
    pub mode: ProfileMode,

    /// Safety cap on emulated cycles. The run terminates with exit
    /// code 0 and a warning if reached.
    #[arg(long, default_value_t = 200_000_000)]
    pub max_cycles: u64,

    /// Optional human-readable note appended to the profile dir.
    #[arg(long)]
    pub note: Option<String>,
}

#[derive(Debug, Args)]
pub struct ProfileDiffArgs {
    pub a: PathBuf,
    pub b: PathBuf,
}
