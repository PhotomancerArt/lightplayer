use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

/// `lp-cli profile …` — run a session or `profile diff` (stub).
#[derive(Debug, Parser)]
#[command(name = "profile", about = "Run a profiling session or compare two profile directories.")]
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

    /// Collectors to enable (comma-separated). m0 supports: alloc.
    #[arg(long, default_value = "alloc", value_delimiter = ',')]
    pub collect: Vec<String>,

    /// Number of frames to advance the workload.
    #[arg(long, default_value_t = 10)]
    pub frames: u32,

    /// Optional human-readable note appended to the profile dir.
    #[arg(long)]
    pub note: Option<String>,
}

#[derive(Debug, Args)]
pub struct ProfileDiffArgs {
    pub a: PathBuf,
    pub b: PathBuf,
}
