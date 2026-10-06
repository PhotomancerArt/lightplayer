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
    /// Write a board manifest to a device's /hardware.json (as Studio's
    /// flash does). Takes effect on the next boot.
    Stamp(StampArgs),
    /// A board's filesystem (`lpfs`) across partition layouts: measure it,
    /// back it up, move it to the new layout, put a backup back.
    Lpfs(LpfsArgs),
    /// Draw the desk's boards for the board bench's page: each registered
    /// board's LightPlayer drawing, and an art board's piece. Needs `board`
    /// and a build with `--features desk-images` (`just desk-images`).
    DeskImages(DeskImagesArgs),
}

#[derive(Debug, Args)]
pub struct DeskImagesArgs {
    /// Where the pictures go: the bench's `images/` under this directory.
    /// Defaults to $BOARD_HOME, else ~/.photomancer/desk.
    #[arg(long)]
    pub home: Option<PathBuf>,
    /// Only this board (its slug).
    #[arg(long)]
    pub only: Option<String>,
    /// How far into an art piece's project its picture is taken, in seconds.
    #[arg(long, default_value_t = 2.0)]
    pub time: f32,
}

#[derive(Debug, Args)]
pub struct LpfsArgs {
    #[command(subcommand)]
    pub command: LpfsCommand,
}

#[derive(Debug, Subcommand)]
pub enum LpfsCommand {
    /// How full a board's filesystem is, and whether its files fit the
    /// current C6 layout. Reads only (a port is reset back into its
    /// firmware afterwards).
    Report(LpfsReportArgs),
    /// Save a board's filesystem: the raw region, the partition table, and
    /// a backup ZIP. Reads only.
    Save(LpfsSaveArgs),
    /// Move a board's files to the layout of a firmware package, writing
    /// the firmware too. Stores a backup first. WRITES THE BOARD.
    Migrate(LpfsMigrateArgs),
    /// Put a backup ZIP's files back onto a board already on the package's
    /// layout (the last-resort path). WRITES THE BOARD'S FILESYSTEM.
    Restore(LpfsRestoreArgs),
    /// Refuse (exit 3) when flashing an image's partition table would move
    /// a board's files — either direction. What `just flash-fw-esp32c6`
    /// runs before espflash.
    Preflight(LpfsPreflightArgs),
    /// Build a 4 MiB emulator chip image: a merged firmware image, a
    /// partition table, and a filesystem holding a directory's files
    /// (emulator walks and tests).
    #[command(hide = true)]
    Fixture(LpfsFixtureArgs),
}

/// Where the image whose layout a board is measured against comes from.
#[derive(Debug, Args, Clone)]
pub struct LpfsTargetArgs {
    /// The target partition table as CSV. Defaults to
    /// lp-fw/fw-esp32c6/partitions.csv under the repo.
    #[arg(long)]
    pub table: Option<PathBuf>,
    /// Measure against this many 4 KB blocks instead of a table's lpfs row.
    #[arg(long)]
    pub target_blocks: Option<u32>,
}

#[derive(Debug, Args)]
pub struct LpfsReportArgs {
    /// A board's serial port (reads its table and filesystem over the
    /// bootloader).
    #[arg(long, conflicts_with_all = ["image", "dir"])]
    pub port: Option<String>,
    /// A raw image: either a whole 4 MiB chip, or one filesystem region
    /// (assumed the pre-2026-10 C6 layout unless --offset/--blocks say
    /// otherwise).
    #[arg(long, conflicts_with = "dir")]
    pub image: Option<PathBuf>,
    /// With --image of a region: where the region was on the chip.
    #[arg(long, value_parser = parse_hex_or_dec)]
    pub offset: Option<u32>,
    /// A project directory, measured as if it were the board's only project
    /// (plus a stamped board manifest and identity).
    #[arg(long)]
    pub dir: Option<PathBuf>,
    #[command(flatten)]
    pub target: LpfsTargetArgs,
    /// Machine-readable output.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct LpfsSaveArgs {
    /// The board's serial port.
    #[arg(long)]
    pub port: String,
    /// Directory to write the raw region, the table and the backup ZIP into.
    #[arg(long)]
    pub out: PathBuf,
}

#[derive(Debug, Args)]
pub struct LpfsFirmwareArgs {
    /// A packaged firmware build's manifest.json
    /// (`lp-cli firmware package <id>`).
    #[arg(long, conflicts_with = "merged")]
    pub manifest: Option<PathBuf>,
    /// Or a merged image (bootloader + table + app at 0x0, as
    /// `espflash save-image --merge` writes).
    #[arg(long)]
    pub merged: Option<PathBuf>,
    /// The chip a --merged image is for.
    #[arg(long, default_value = "esp32c6")]
    pub chip: String,
}

#[derive(Debug, Args)]
pub struct LpfsMigrateArgs {
    /// The board's serial port.
    #[arg(long)]
    pub port: String,
    #[command(flatten)]
    pub firmware: LpfsFirmwareArgs,
    /// Where the mandatory backup is stored (default
    /// ~/.lightplayer/backups/<mac>/).
    #[arg(long)]
    pub backup_dir: Option<PathBuf>,
    /// Do not ask before writing.
    #[arg(long)]
    pub yes: bool,
    /// Do not wait for the board's hello afterwards.
    #[arg(long)]
    pub no_verify: bool,
}

#[derive(Debug, Args)]
pub struct LpfsRestoreArgs {
    /// The board's serial port.
    #[arg(long)]
    pub port: String,
    /// The backup ZIP (format 2) to put back.
    #[arg(long)]
    pub archive: PathBuf,
    #[command(flatten)]
    pub firmware: LpfsFirmwareArgs,
    /// Restore even though the archive's board address is not this board's.
    #[arg(long)]
    pub other_board: bool,
    /// Do not ask before writing.
    #[arg(long)]
    pub yes: bool,
}

#[derive(Debug, Args)]
pub struct LpfsPreflightArgs {
    /// The board's serial port.
    #[arg(long)]
    pub port: String,
    /// The partition table (CSV) of the image about to be flashed.
    #[arg(long)]
    pub table: PathBuf,
    /// Erase the board's filesystem instead of refusing (a test board whose
    /// files are not wanted).
    #[arg(long, conflicts_with = "migrate")]
    pub discard_lpfs: bool,
    /// Exit 4 instead of 3 on a mismatch, telling the caller to run
    /// `lp-cli hardware lpfs migrate` (just flash-fw-esp32c6 migrate=1).
    #[arg(long)]
    pub migrate: bool,
}

#[derive(Debug, Args)]
pub struct LpfsFixtureArgs {
    /// The merged firmware image to place at 0x0.
    #[arg(long)]
    pub merged: PathBuf,
    /// The partition table (CSV) to write at 0x8000 — and whose lpfs row
    /// the filesystem is built at. Defaults to the frozen pre-2026-10 table.
    #[arg(long)]
    pub table: Option<PathBuf>,
    /// The directory whose tree becomes the board's filesystem root.
    #[arg(long)]
    pub tree: PathBuf,
    /// Where to write the 4 MiB chip image.
    #[arg(long)]
    pub out: PathBuf,
}

fn parse_hex_or_dec(text: &str) -> Result<u32, String> {
    let digits = text.trim();
    match digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        Some(hex) => u32::from_str_radix(hex, 16).map_err(|e| e.to_string()),
        None => digits.parse::<u32>().map_err(|e| e.to_string()),
    }
}

#[derive(Debug, Args)]
pub struct StampArgs {
    /// The device, e.g. serial:/dev/cu.usbmodem2101 or serial:auto.
    pub host: String,
    /// The board manifest JSON to write, e.g.
    /// lp-core/lpc-hardware/boards/seeed/xiao-esp32-c6.json.
    pub manifest: PathBuf,
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
    #[value(name = "desktop")]
    Desktop,
}

impl From<HardwareTargetArg> for lpc_hardware::HardwareTarget {
    fn from(value: HardwareTargetArg) -> Self {
        match value {
            HardwareTargetArg::Esp32c6 => Self::Esp32c6,
            HardwareTargetArg::Esp32s3 => Self::Esp32s3,
            HardwareTargetArg::Rv32imacEmu => Self::Rv32imacEmu,
            HardwareTargetArg::Desktop => Self::Desktop,
        }
    }
}

impl HardwareTargetArg {
    /// Every target, in menu order. `interactive_new_manifest` picks from this,
    /// so a new variant reaches the interactive manager without a second edit.
    pub const ALL: &'static [Self] = &[
        Self::Esp32c6,
        Self::Esp32s3,
        Self::Rv32imacEmu,
        Self::Desktop,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Esp32c6 => "esp32c6",
            Self::Esp32s3 => "esp32s3",
            Self::Rv32imacEmu => "rv32imac_emu",
            Self::Desktop => "desktop",
        }
    }
}
