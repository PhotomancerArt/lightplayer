use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "firmware",
    about = "Build, package and inspect firmware variants."
)]
pub struct FirmwareCli {
    #[command(subcommand)]
    pub subcommand: FirmwareSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum FirmwareSubcommand {
    /// Extract and print the manifest core embedded in a firmware artifact
    /// (ELF, espflash merged image, or wasm module). The blob is parsed and
    /// re-serialized, so `show` succeeding also validates its shape.
    Show(ShowArgs),
    /// List the checked-in firmware build definitions (`lp-fw/builds/`).
    List(ListArgs),
    /// Cargo-build one firmware variant from its build definition.
    Build(BuildArgs),
    /// Build, merge and emit a distributable firmware package
    /// (`manifest.json` schemaVersion 2 + merged image).
    Package(PackageArgs),
}

#[derive(Debug, Args)]
pub struct ShowArgs {
    /// Path to the firmware artifact to scan.
    pub artifact: PathBuf,

    /// Print the payload exactly as embedded instead of pretty-printing.
    #[arg(long)]
    pub raw: bool,
}

#[derive(Debug, Args)]
pub struct ListArgs {
    /// Emit the build definitions as JSON instead of a table.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct BuildArgs {
    /// Build definition id (see `lp-cli firmware list`).
    pub id: String,
}

#[derive(Debug, Args)]
pub struct PackageArgs {
    /// Build definition id (see `lp-cli firmware list`).
    pub id: String,

    /// Output directory; defaults to
    /// `target/studio-web-assets/firmware/<id>`.
    #[arg(long)]
    pub out: Option<PathBuf>,

    /// Package an already-built ELF instead of running cargo first.
    #[arg(long)]
    pub no_build: bool,
}
