use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "link",
    about = "Measure a board's host link (the soak variant)."
)]
pub struct LinkCli {
    #[command(subcommand)]
    pub subcommand: LinkSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum LinkSubcommand {
    /// Drive a `soak_link` firmware image and check every byte it sends.
    ///
    /// Opens the port, asks the board for packed replies (unless
    /// `--encoding json`), starts the soak with a `SOAK! …` line, reads for
    /// `--seconds`, stops it, and prints what was lost: soak frames missing
    /// or damaged, packed frames torn, and the byte ledger (bytes the
    /// board's io_task says it wrote against bytes read). Needs a board (or
    /// an emulated one) running `fw-esp32c6 --features soak_link`.
    ///
    /// Examples:
    ///
    ///   lp-cli link soak --port tcp://127.0.0.1:5591 --seconds 30
    ///
    ///   lp-cli link soak --port /dev/cu.usbmodem1101 --max 4096 --stall-every-ms 2000 --stall-ms 300
    #[command(verbatim_doc_comment)]
    Soak(SoakArgs),

    /// Check a raw capture of a soak stream made elsewhere (the Chrome
    /// raw-reader page, a pty log) by the same rules as `link soak`.
    SoakVerify(SoakVerifyArgs),
}

#[derive(Debug, Args)]
pub struct SoakArgs {
    /// A serial device path (`/dev/cu.usbmodem1101`) or `tcp://host:port`
    /// (an emulated board's link, `lp-cli emu run --link`).
    #[arg(long)]
    pub port: String,

    /// `packed` (ask for packed replies, like Studio) or `json`.
    #[arg(long, default_value = "packed")]
    pub encoding: String,

    /// How long to soak.
    #[arg(long, default_value_t = 30.0)]
    pub seconds: f64,

    /// Smallest soak text, bytes (at least 48).
    #[arg(long, default_value_t = 16)]
    pub min: u32,

    /// Largest soak text, bytes (at most 16000).
    #[arg(long, default_value_t = 16_384)]
    pub max: u32,

    /// Payload bytes per second the board aims for; 0 = as fast as it goes.
    #[arg(long, default_value_t = 0)]
    pub rate: u32,

    /// A console log line after every N soak frames (0 = none).
    #[arg(long, default_value_t = 0)]
    pub logs: u32,

    /// Seed for the board's pad and sizes.
    #[arg(long, default_value_t = 1)]
    pub seed: u32,

    /// Most milliseconds one server-loop pass spends sending.
    #[arg(long, default_value_t = 20)]
    pub budget: u32,

    /// Stop reading for `--stall-ms` every this many milliseconds (0 = never):
    /// a host application that falls behind, as a busy page does.
    #[arg(long, default_value_t = 0)]
    pub stall_every_ms: u64,

    /// How long each stall lasts.
    #[arg(long, default_value_t = 0)]
    pub stall_ms: u64,

    /// Host → board echo traffic, payload bytes per second (0 = none).
    #[arg(long, default_value_t = 0)]
    pub echo_bps: u32,

    /// Size of each echo line, bytes.
    #[arg(long, default_value_t = 512)]
    pub echo_len: usize,

    /// Bytes per read call (the OS read size the reader asks for).
    #[arg(long, default_value_t = 65_536)]
    pub read_size: usize,

    /// Write `capture.bin`, `events.jsonl` and `summary.json` here.
    #[arg(long)]
    pub out: Option<PathBuf>,

    /// A label for the summary (the configuration: board or emulator, image).
    #[arg(long, default_value = "")]
    pub label: String,
}

#[derive(Debug, Args)]
pub struct SoakVerifyArgs {
    /// The raw capture.
    pub capture: PathBuf,

    /// Write `events.jsonl` and `summary.json` here.
    #[arg(long)]
    pub out: Option<PathBuf>,

    /// A label for the summary.
    #[arg(long, default_value = "")]
    pub label: String,
}
