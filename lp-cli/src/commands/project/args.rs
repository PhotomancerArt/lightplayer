use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "project",
    about = "Inspect and upgrade LightPlayer project packages on disk."
)]
pub struct ProjectCli {
    #[command(subcommand)]
    pub subcommand: ProjectSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum ProjectSubcommand {
    /// Classify a project directory's on-disk format and, if it is between
    /// the upgrade floor and the current format, migrate it forward.
    ///
    /// Dry-run by default: the report is printed but nothing is written.
    /// Pass --apply to write the changed files in place.
    ///
    /// Exit codes:
    ///   0  already current, or upgraded successfully (with --apply)
    ///   1  refused: below the upgrade floor, a future format, not a
    ///      project, unreadable, or a shape the upgrader will not guess at
    ///   2  dry run only: the project is upgradable and would change, but
    ///      --apply was not given, so nothing was written
    #[command(verbatim_doc_comment)]
    Upgrade(UpgradeArgs),
}

#[derive(Debug, Args)]
pub struct UpgradeArgs {
    /// Project directory (must contain project.json).
    pub dir: PathBuf,

    /// Write the upgraded files in place. Without this flag the report is
    /// printed and nothing on disk changes.
    #[arg(long)]
    pub apply: bool,
}
