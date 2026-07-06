use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "schema",
    about = "Generate and verify the checked-in schemas/ tree (JSON Schemas + slot shape dumps)."
)]
pub struct SchemaCli {
    #[command(subcommand)]
    pub subcommand: SchemaSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum SchemaSubcommand {
    /// Regenerate schemas/ from the model's slot shape registry and board
    /// manifest types. Owns `schemas/*.schema.json` and `schemas/shapes/*.json`:
    /// stale files matching those patterns are deleted.
    Gen(GenArgs),
}

#[derive(Debug, Args)]
pub struct GenArgs {
    /// Compare regenerated output against the checked-in files instead of
    /// writing; reports each drifted file and exits nonzero on any drift.
    #[arg(long)]
    pub check: bool,

    /// Output directory. Defaults to schemas/ under the repo root (found by
    /// searching upward from the current directory).
    #[arg(long)]
    pub out: Option<PathBuf>,
}
