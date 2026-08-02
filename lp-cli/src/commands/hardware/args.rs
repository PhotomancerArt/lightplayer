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
    /// List attached serial hardware; `--probe` identifies ESP32 chips.
    List(ListArgs),
    /// Manage checked-in board manifests.
    Manifest(ManifestArgs),
    /// Calibrate board-visible GPIO labels with ESP32 firmware.
    Calibrate(CalibrateArgs),
}

#[derive(Debug, Args)]
pub struct ListArgs {
    /// Identify the chip on each port via the espflash handshake. Resets
    /// idle boards (bootloader, then back into the app); busy ports are
    /// reported, not reset. Each port gets a hard timeout.
    #[arg(long)]
    pub probe: bool,

    /// Only show boards whose probed chip matches (implies --probe).
    /// Exits nonzero when nothing matches.
    #[arg(long)]
    pub chip: Option<String>,

    /// Show every serial port, not just USB devices.
    #[arg(long)]
    pub all: bool,

    /// Per-port probe timeout in seconds.
    #[arg(long, default_value_t = 10)]
    pub probe_timeout_secs: u64,

    /// Emit JSON instead of a table.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ManifestArgs {
    /// Repository root. Defaults to searching upward from the current directory.
    #[arg(long)]
    pub repo: Option<PathBuf>,

    /// Board manifest directory. Defaults to lp-core/lpc-hardware/boards under the repo root.
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
    /// Hardware target running the calibration firmware.
    #[arg(value_enum)]
    pub target: Option<HardwareTargetArg>,
    /// Board manifest id, for example seeed/xiao-esp32-c6.
    #[arg(long)]
    pub board: Option<String>,
    /// Serial port path, auto, or serial:auto.
    #[arg(long)]
    pub port: Option<String>,
    /// Repository root. Defaults to searching upward from the current directory.
    #[arg(long)]
    pub repo: Option<PathBuf>,
    /// Board manifest directory. Defaults to lp-core/lpc-hardware/boards under the repo root.
    #[arg(long)]
    pub boards_dir: Option<PathBuf>,
    /// Firmware response timeout before a pin is treated as crash-suspect.
    #[arg(long, default_value_t = 1000)]
    pub timeout_ms: u64,
    /// Board-visible label currently connected to the scope.
    #[arg(long)]
    pub label: Option<String>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum HardwareTargetArg {
    #[value(name = "esp32c6")]
    Esp32c6,
    #[value(name = "esp32s3")]
    Esp32s3,
    #[value(name = "rv32imac_emu")]
    Rv32imacEmu,
}

impl From<HardwareTargetArg> for lpc_hardware::HardwareTarget {
    fn from(value: HardwareTargetArg) -> Self {
        match value {
            HardwareTargetArg::Esp32c6 => Self::Esp32c6,
            HardwareTargetArg::Esp32s3 => Self::Esp32s3,
            HardwareTargetArg::Rv32imacEmu => Self::Rv32imacEmu,
        }
    }
}

impl HardwareTargetArg {
    /// Every target, in menu order. `interactive_new_manifest` picks from this,
    /// so a new variant reaches the interactive manager without a second edit.
    pub const ALL: &'static [Self] = &[Self::Esp32c6, Self::Esp32s3, Self::Rv32imacEmu];

    pub fn label(self) -> &'static str {
        match self {
            Self::Esp32c6 => "esp32c6",
            Self::Esp32s3 => "esp32s3",
            Self::Rv32imacEmu => "rv32imac_emu",
        }
    }
}
