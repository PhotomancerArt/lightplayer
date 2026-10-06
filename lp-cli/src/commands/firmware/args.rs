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
    /// (`manifest.json` schemaVersion 2 + merged image). A split build also
    /// gets its OTA files (`ota-manifest.json`, `core.z`, `engine.z`) in its
    /// parts directory, `target/firmware-parts/<id>/`.
    Package(PackageArgs),
    /// Gather packaged firmware into a release staging directory under the
    /// release's asset names (`<target>.<file>`). Compresses nothing.
    ReleaseAssets(ReleaseAssetsArgs),
    /// Re-verify a release staging directory from its files alone.
    ReleaseCheck(ReleaseCheckArgs),
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

#[derive(Debug, Args)]
pub struct ReleaseAssetsArgs {
    /// Targets to stage, comma-separated (default: every id in
    /// `lp-fw/builds/served.json`).
    #[arg(long, value_delimiter = ',')]
    pub targets: Option<Vec<String>>,

    /// The staging directory; must be absent or empty.
    #[arg(long)]
    pub out: PathBuf,

    /// Also stage a dev version (the pre-merge dry run). A release never
    /// passes it.
    #[arg(long)]
    pub allow_dev: bool,
}

#[derive(Debug, Args)]
pub struct ReleaseCheckArgs {
    /// The staging directory (or a downloaded release) to verify.
    pub dir: PathBuf,

    /// Accept a dev version (the pre-merge dry run).
    #[arg(long)]
    pub allow_dev: bool,

    /// Targets to check, comma-separated (default: every id in
    /// `lp-fw/builds/served.json` in a checkout, otherwise every
    /// `<target>.package.json` in the directory).
    #[arg(long, value_delimiter = ',')]
    pub targets: Option<Vec<String>>,
}
