use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::client::board_password::BoardPasswordArgs;
use crate::commands::emu::args::EmuChip;

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
    /// Targets: a serial device, `tcp://host:port` (an emulated board's
    /// link, `lp-cli emu run --link`), `lan:<host>[:port]` (a board on the
    /// network: its secure link, a locked board's password from
    /// `--password-stdin` or `LP_PASSWORD`), or `blepipe:<port>` — a board's
    /// Bluetooth link, through `spikes/ble-lab/pipe.html`, a browser page
    /// that holds the GATT connection and moves frames to and from
    /// `ws://127.0.0.1:<port>` (each connection a new link; window 16,
    /// `--ota-ahead` 4; with `--ota-password`, a running engine is logged
    /// in to first). The runbook is `spikes/ble-lab/README.md`, "Updates
    /// over Bluetooth":
    ///
    ///   lp-cli link capture blepipe:5599 --console ble.txt --seconds 900 \
    ///       --ota-offer target/ota-ble/y/ota --ota-password '…' \
    ///       --exit-on '[host-ota] done:'
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
    /// Measure how promptly a rendering board answers: request round trips,
    /// the link's own round trips (send → ACK), transfer rates both ways,
    /// and the idle frame rate.
    ///
    /// Phases: warm-up → idle (fps from heartbeats) → transfers (a file
    /// written and read back; `WriteChunk`s the board refuses without
    /// touching flash) → requests (`ListLoadedProjects`, one at a time, each
    /// after a random pause, from a fixed board time) → tail. A summary on
    /// stderr; `--json` holds every sample.
    ///
    /// Targets: a serial device or `lan:<host>[:port]` (a board on the
    /// network, its link secure; wall-clock time; the board renders whatever
    /// project it loads at boot), or `emu:<ELF>` (one chip's machine in this
    /// process, EMULATED time, with `--project` deployed first; the report
    /// adds the WS281x frames decoded off the pads). `--chip` picks the
    /// machine (`esp32c6`, the default; `esp32s3`; `esp32v3`, the classic) —
    /// a serial target ignores it and is refused if it is given. Each
    /// chip's `--project` default differs (the PLAYFUL choker on the C6,
    /// `shader-oracle` on the S3, `five-wire` on the classic), and so does
    /// `--grade`'s default: `t2` on the C6, `t1` — the only grade either
    /// has — on the S3 and classic. Compare emulated runs in frames, never
    /// against silicon in ms.
    ///
    /// Examples:
    ///
    ///   lp-cli link rtt /dev/cu.usbmodem2101 --json silicon.json
    ///
    ///   LP_PASSWORD=… lp-cli link rtt lan:lp-3f2a.local --json lan.json
    ///
    ///   lp-cli link rtt emu:target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6 \
    ///       --requests-at-s 40 --json emu.json --console emu.console.txt
    ///
    ///   lp-cli link rtt emu:target/xtensa-esp32s3-none-elf/release-esp32s3/fw-esp32s3 \
    ///       --chip esp32s3 --json s3.json
    ///
    ///   lp-cli link rtt emu:target/xtensa-esp32-none-elf/release-esp32v3/fw-esp32v3 \
    ///       --chip esp32v3 --json v3.json
    #[command(verbatim_doc_comment)]
    Rtt(RttArgs),
}

#[derive(Debug, Args)]
pub struct RttArgs {
    /// A serial device, `lan:<host>[:port]` (a board on the network), or
    /// `emu:<ELF>`.
    pub target: String,
    #[command(flatten)]
    pub board_password: BoardPasswordArgs,
    /// `emu:` only: which chip's machine to host. A serial target is real
    /// hardware and refuses this.
    #[arg(long, value_enum)]
    pub chip: Option<EmuChip>,
    /// Write the full report (every sample) here, as JSON.
    #[arg(long)]
    pub json: Option<PathBuf>,
    /// Write the board's whole console here.
    #[arg(long)]
    pub console: Option<PathBuf>,
    /// Requests to time.
    #[arg(long, default_value_t = 150)]
    pub count: usize,
    /// Largest random pause before each request, ms.
    #[arg(long, default_value_t = 40)]
    pub max_gap_ms: u64,
    /// Seed for the pauses, so two runs pause alike.
    #[arg(long, default_value_t = 7)]
    pub seed: u64,
    /// Reads of the file (the board → host rate).
    #[arg(long, default_value_t = 10)]
    pub reads: usize,
    /// Size of the file written and read back, bytes.
    #[arg(long, default_value_t = 10240)]
    pub read_bytes: usize,
    /// Refused chunk writes (the host → board rate).
    #[arg(long, default_value_t = 10)]
    pub writes: usize,
    /// Size of each refused chunk, bytes.
    #[arg(long, default_value_t = 8192)]
    pub write_bytes: usize,
    /// Seconds before anything is measured.
    #[arg(long, default_value_t = 6.0)]
    pub warmup_s: f64,
    /// Seconds of render alone, for the idle frame rate (≥ 10 for two
    /// heartbeats).
    #[arg(long, default_value_t = 20.0)]
    pub idle_s: f64,
    /// Board time to start the transfers at, seconds: since power-on on
    /// `emu:`, the board's uptime (from its heartbeats) on a serial device.
    /// 0 = right after the idle phase.
    #[arg(long, default_value_t = 0.0)]
    pub transfers_at_s: f64,
    /// Board time to start the requests at, as `--transfers-at-s`. 0 = right
    /// after the transfers.
    #[arg(long, default_value_t = 0.0)]
    pub requests_at_s: f64,
    /// Seconds after the requests (≥ 10 for two more heartbeats).
    #[arg(long, default_value_t = 11.0)]
    pub tail_s: f64,
    /// `emu:` only: the time grade. Default: `t2` on the C6, `t1` — the only
    /// grade either machine has — on the S3 and classic; anything else
    /// there is refused.
    #[arg(long)]
    pub grade: Option<String>,
    /// `emu:` only: the project to deploy. Default: the PLAYFUL choker on
    /// the C6, `shader-oracle` on the S3, `five-wire` on the classic.
    #[arg(long)]
    pub project: Option<PathBuf>,
    /// A label carried into the JSON report.
    #[arg(long, default_value = "")]
    pub label: String,
}

#[derive(Debug, Args)]
pub struct CaptureArgs {
    /// A serial device, `tcp://host:port`, `lan:<host>[:port]` (a board on
    /// the network), or `blepipe:<port>`.
    pub target: String,
    #[command(flatten)]
    pub board_password: BoardPasswordArgs,

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

    /// An over-the-air update on the link's channel 3. The port is reopened
    /// when it drops: a board's USB goes away on every reset an update
    /// makes, and each reopen is a new link session (on `blepipe:`, each
    /// Bluetooth connection the page makes is).
    #[command(flatten)]
    pub ota: crate::commands::ota_host::OtaArgs,
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
