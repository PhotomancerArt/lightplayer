use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "link",
    about = "Measure a board's host link (the lp-link comms lab)."
)]
pub struct LinkCli {
    #[command(subcommand)]
    pub subcommand: LinkSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum LinkSubcommand {
    /// Prove `lp-link` against a `test_comms_lab` board: bring the link up,
    /// echo soak messages of random sizes, take a board stream, ask for log
    /// lines, read the board's counters, and check every message arrived
    /// whole, once and in order.
    ///
    /// Targets: a serial device, `tcp://host:port` (an emulated board's
    /// link), or `emu:<fw-esp32c6 ELF>` (the C6 machine in this process, in
    /// EMULATED time, with `--faults` and `--free-lag-ns` on its USB link).
    ///
    /// Examples:
    ///
    ///   lp-cli link lab /dev/cu.usbmodem1101 --echo-secs 60 --stream-secs 60
    ///
    ///   lp-cli link lab /dev/cu.usbmodem1101 --termios chrome --host-stall-every-ms 1000 --host-stall-ms 100
    ///
    ///   lp-cli link lab emu:target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6 \
    ///       --faults in-drop=1%,in-tail=1%,in-run=0.1%,out-drop=1%,seed=7
    #[command(verbatim_doc_comment)]
    Lab(LabArgs),
}

#[derive(Debug, Args)]
pub struct LabArgs {
    /// A serial device, `tcp://host:port`, or `emu:<ELF>`.
    pub target: String,

    /// Seconds of echo soak.
    #[arg(long, default_value_t = 10.0)]
    pub echo_secs: f64,

    /// Seconds of board stream.
    #[arg(long, default_value_t = 10.0)]
    pub stream_secs: f64,

    /// Smallest soak message, bytes.
    #[arg(long, default_value_t = 16)]
    pub min: usize,

    /// Largest soak message, bytes.
    #[arg(long, default_value_t = 16_384)]
    pub max: usize,

    /// Echo messages outstanding at once.
    #[arg(long, default_value_t = 4)]
    pub in_flight: usize,

    /// Seed for message sizes (and the emulated run's nonces).
    #[arg(long, default_value_t = 1)]
    pub seed: u64,

    /// Log lines to ask the board for (0 = skip).
    #[arg(long, default_value_t = 100)]
    pub logs: u32,

    /// Length of each asked-for log line.
    #[arg(long, default_value_t = 100)]
    pub log_len: usize,

    /// Stall the board's executor this long once, mid-echo (a shader
    /// compile's shape); 0 = never.
    #[arg(long, default_value_t = 0)]
    pub stall_ms: u32,

    /// How a serial device is opened: `raw`, or `chrome` (Chromium's Web
    /// Serial termios on macOS, PARMRK set, with its 0xFF fold).
    #[arg(long, value_enum, default_value_t = super::lab_port::TermiosMode::Raw)]
    pub termios: super::lab_port::TermiosMode,

    /// Stop reading for `--host-stall-ms` every this many ms (a busy page).
    #[arg(long, default_value_t = 0)]
    pub host_stall_every_ms: u64,

    /// How long each host stall lasts.
    #[arg(long, default_value_t = 0)]
    pub host_stall_ms: u64,

    /// `emu:` only: the USB link's fault injector spec
    /// (lp_emu_esp_common::link_faults), e.g. `in-drop=1%,out-drop=1%,seed=7`.
    #[arg(long)]
    pub faults: Option<String>,

    /// `emu:` only: the USB block's free lag hypothesis, emulated ns.
    #[arg(long, default_value_t = 0)]
    pub free_lag_ns: u64,

    /// `emu:` only: the time grade, `t1` or `t2`.
    #[arg(long, default_value = "t1")]
    pub grade: String,

    /// `emu:` only: record the board's block census and print the hottest
    /// blocks (where the link's instructions go).
    #[arg(long)]
    pub blockprof: bool,

    /// `emu:` only: emulated microseconds per slice between host passes.
    #[arg(long, default_value_t = 250)]
    pub slice_us: u64,

    /// Instead of the soak: bring the link up, tell the board to panic, and
    /// check the panic arrives as raw text, then a reset, then a new session.
    #[arg(long)]
    pub panic_test: bool,

    /// Plain COBS instead of COBS-FF (0xFF on the wire): the A/B control,
    /// against a `test_comms_lab_plain_cobs` image.
    #[arg(long)]
    pub plain_cobs: bool,

    /// Override the host link's minimum retransmit timer, ms (tuning).
    #[arg(long)]
    pub min_rto_ms: Option<u64>,

    /// Write the run's report as JSON here.
    #[arg(long)]
    pub json: Option<PathBuf>,

    /// A serial device or socket only: every byte read, as read, to this file.
    #[arg(long)]
    pub raw_capture: Option<PathBuf>,

    /// Configuration label for a port run (e.g. `silicon:esp32c6 10:bd:a3:b0:8e:30`).
    #[arg(long, default_value = "")]
    pub label: String,
}
