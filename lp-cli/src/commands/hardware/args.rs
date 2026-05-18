use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "hardware", about = "Developer hardware manifest tools.")]
pub struct HardwareCli {
    #[command(subcommand)]
    pub subcommand: Option<HardwareSubcommand>,
}

#[derive(Debug, Subcommand)]
pub enum HardwareSubcommand {
    /// Manage checked-in board manifests.
    Manifest(ManifestArgs),
    /// Stub for firmware-assisted board calibration.
    Calibrate(CalibrateArgs),
}

#[derive(Debug, Args)]
pub struct ManifestArgs {
    /// Repository root. Defaults to searching upward from the current directory.
    #[arg(long)]
    pub repo: Option<PathBuf>,

    /// Board manifest directory. Defaults to lp-core/lpc-shared/boards under the repo root.
    #[arg(long)]
    pub boards_dir: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<ManifestSubcommand>,
}

#[derive(Debug, Subcommand)]
pub enum ManifestSubcommand {
    /// List manifests.
    List,
    /// Show one manifest.
    Show { id: String },
    /// Validate one manifest or all manifests.
    Validate { id: Option<String> },
    /// Create a new manifest.
    New(NewManifestArgs),
    /// Update manifest metadata.
    Set(SetManifestArgs),
    /// Delete a manifest.
    Delete(DeleteManifestArgs),
}

#[derive(Debug, Args)]
pub struct NewManifestArgs {
    #[arg(long, value_enum)]
    pub target: HardwareTargetArg,
    #[arg(long)]
    pub vendor: String,
    #[arg(long)]
    pub product: String,
    #[arg(long)]
    pub url: Option<String>,
    #[arg(long)]
    pub description: Option<String>,
    #[arg(long)]
    pub id: Option<String>,
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct SetManifestArgs {
    pub id: String,
    #[arg(long, value_enum)]
    pub target: Option<HardwareTargetArg>,
    #[arg(long)]
    pub vendor: Option<String>,
    #[arg(long)]
    pub product: Option<String>,
    #[arg(long)]
    pub url: Option<String>,
    #[arg(long)]
    pub description: Option<String>,
}

#[derive(Debug, Args)]
pub struct DeleteManifestArgs {
    pub id: String,
    #[arg(long)]
    pub yes: bool,
}

#[derive(Debug, Args)]
pub struct CalibrateArgs {
    pub target: String,
    #[arg(long)]
    pub board: String,
    #[arg(long)]
    pub port: Option<String>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum HardwareTargetArg {
    Esp32c6,
    Rv32imacEmu,
}

impl From<HardwareTargetArg> for lpc_shared::hardware::HardwareTarget {
    fn from(value: HardwareTargetArg) -> Self {
        match value {
            HardwareTargetArg::Esp32c6 => Self::Esp32c6,
            HardwareTargetArg::Rv32imacEmu => Self::Rv32imacEmu,
        }
    }
}

impl HardwareTargetArg {
    pub fn label(self) -> &'static str {
        match self {
            Self::Esp32c6 => "esp32c6",
            Self::Rv32imacEmu => "rv32imac_emu",
        }
    }
}
