use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "firmware",
    about = "Inspect firmware build artifacts via their embedded manifest core."
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
}

#[derive(Debug, Args)]
pub struct ShowArgs {
    /// Path to the firmware artifact to scan.
    pub artifact: PathBuf,

    /// Print the payload exactly as embedded instead of pretty-printing.
    #[arg(long)]
    pub raw: bool,
}
