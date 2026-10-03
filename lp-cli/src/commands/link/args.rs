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
    /// Open a board's port as the host of its link and write what the board
    /// says as a console — raw text, log records, each wire message as its
    /// `M!{json}` line, `[link] …` notes — until a line contains `--exit-on`
    /// or `--seconds` pass.
    ///
    /// The port is opened the way lp-cli's own transports open it: no reset
    /// dance, so a board that is already running keeps running. This is the
    /// silicon half of `lp-cli validate`'s link host; the emulated half is
    /// `lp-cli emu run --host-link`, and both write the same lines.
    ///
    /// Targets: a serial device, or `tcp://host:port` (an emulated board's
    /// link, `lp-cli emu run --link`).
    ///
    /// `--request` sends a client request once the board has said hello:
    /// the desk's way to ask a board something with nothing else on its
    /// port. Each waits for the one before to be done — answered, or for a
    /// `reboot`, the board restarted (a `[link] reset (PeerRestarted)` and a
    /// new session) — and goes after its session's hello, so a reboot
    /// followed by a hello asks the rebooted board:
    ///
    ///   lp-cli link capture /dev/cu.usbmodem1101 --console boot.txt \
    ///       --request reboot --request hello --seconds 20
    #[command(verbatim_doc_comment)]
    Capture(CaptureArgs),
}

#[derive(Debug, Args)]
pub struct CaptureArgs {
    /// A serial device, or `tcp://host:port`.
    pub target: String,

    /// Write the console here, a line at a time.
    #[arg(long)]
    pub console: PathBuf,

    /// Stop at the end of the first console line containing this text; a run
    /// that ends without one fails.
    #[arg(long = "exit-on")]
    pub exit_on: Option<String>,

    /// Wall-clock seconds to host the link for.
    #[arg(long, default_value_t = 120)]
    pub seconds: u64,

    /// Do not ask the board to pack its replies (JSON Pack).
    #[arg(long = "json-replies")]
    pub json_replies: bool,

    /// Send this client request once the board's hello arrives — the JSON
    /// of a `ClientRequest` (`'"reboot"'`, `'{"setLogLevel":{…}}'`), or a
    /// unit request's bare name (`reboot`). Repeatable; sent in order, each
    /// once the one before is done: answered, or — for a `reboot` — the
    /// board restarted and said hello again. A request whose session resets
    /// under it is done unanswered. The run fails if one is never sent,
    /// never answered, or a `reboot` never restarts the board.
    #[arg(long)]
    pub request: Vec<String>,

    /// OTA split-link spike: offer the build in this directory (`core.bin`,
    /// `engine.bin`) on the update channel and serve the board's requests.
    /// The port is reopened when it drops, since a board's USB goes away on
    /// every reset the update performs.
    #[arg(long = "ota-offer")]
    pub ota_offer: Option<PathBuf>,

    /// OTA spike, `blepipe:` targets: log in with this password once the
    /// board says hello (an untrusted link needs the edit tier to offer).
    #[arg(long)]
    pub password: Option<String>,

    /// OTA spike: chunks to keep in flight ahead of the board's request
    /// (default 4 on `blepipe:`, 1 elsewhere).
    #[arg(long = "ota-ahead")]
    pub ota_ahead: Option<u32>,

    /// OTA spike, `blepipe:` targets: the host link's send window, in frames
    /// (the board's advertised receive window still caps it).
    #[arg(long = "ble-window", default_value_t = 32)]
    pub ble_window: u8,

    /// OTA spike, `blepipe:` targets: keep the update ticket in this file, so
    /// a later run can finish an update an earlier one authorized.
    #[arg(long = "ota-ticket-file")]
    pub ota_ticket_file: Option<PathBuf>,
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
