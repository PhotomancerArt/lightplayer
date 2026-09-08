use std::path::PathBuf;

use clap::{Args, Subcommand, ValueEnum};

#[derive(Debug, Args)]
pub struct EmuCli {
    #[command(subcommand)]
    pub command: EmuCommand,
}

#[derive(Debug, Subcommand)]
pub enum EmuCommand {
    /// Boot an ESP32-C6 firmware image and serve it on a socket.
    Run(RunArgs),
}

/// Which chip. One today; the enum is here because `--chip` reads better than
/// a `run-c6` subcommand the day the classic arrives, and because a caller
/// who spells the wrong chip should be told so rather than silently served a
/// C6.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum EmuChip {
    #[default]
    #[value(name = "esp32c6")]
    Esp32C6,
}

/// Which link the socket is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum LinkKind {
    /// USB-Serial-JTAG — the link the shipped firmware speaks, and what a
    /// board on a USB cable presents. The default because it is what the
    /// product does.
    #[default]
    Usb,
    /// UART0 (GPIO16/17). What the `spike_uart0_link` images use and what a
    /// bridge board taps; a shipped image says nothing on it.
    Uart0,
}

/// Guest time's grade (plan PD5). Never a host gate (PD9) — it changes when
/// events land, not what they are.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum Grade {
    /// One cycle per instruction.
    #[default]
    T1,
    /// Per-class instruction costs.
    T2,
}

#[derive(Debug, Args)]
pub struct RunArgs {
    #[arg(long, value_enum, default_value_t = EmuChip::Esp32C6)]
    pub chip: EmuChip,

    /// A firmware ELF, loaded straight into memory at its entry point. Fast,
    /// and what the per-tick gates use.
    #[arg(long, group = "image")]
    pub elf: Option<PathBuf>,

    /// A whole merged flash image (`scripts/emu/build-merged-image.sh`), booted
    /// from the reset vector through the real mask ROM and the ESP-IDF
    /// second-stage bootloader — the closer twin of flashing a board.
    #[arg(long, group = "image")]
    pub merged: Option<PathBuf>,

    /// Address to serve the link on, for example `127.0.0.1:5591`. `lp-cli
    /// upload <project> serial:tcp://<addr>` connects to exactly this.
    ///
    /// Omitted, the machine still boots and still talks — the console is kept
    /// in memory and written by `--console` — but nothing is listening and
    /// nothing can be uploaded to it. That is the right mode for "boot this
    /// image and show me what it says", and it is deliberately what you get
    /// by default: a tool that binds a port without being asked is a tool
    /// that collides with the one already running.
    #[arg(long)]
    pub link: Option<String>,

    #[arg(long = "link-kind", value_enum, default_value_t = LinkKind::Usb)]
    pub link_kind: LinkKind,

    /// Pretend no USB host is attached at power-on. The firmware's connection
    /// monitor sees an unplugged cable; nothing a client sends is delivered
    /// until it connects.
    #[arg(long, conflicts_with = "monitor")]
    pub host_absent: bool,

    /// Hold a reader on the link for the whole run, the way
    /// `espflash flash --monitor` holds a port.
    ///
    /// Without it, a client connecting to `--link` is an application OPENING
    /// the port and disconnecting is it CLOSING one — which is what a Web
    /// Serial `open()`/`close()` means and what `lp-cli upload`'s readiness
    /// engine expects, so it is the default. It also means the guest's output
    /// after the client leaves goes nowhere, exactly as it would on a board
    /// with nothing plugged in. A walk wants both: a client that uploads and
    /// leaves, AND a transcript of everything the device said afterwards.
    /// This gives it one, by declaring the host attached and draining from
    /// power-on and leaving the socket as bytes only.
    #[arg(long)]
    pub monitor: bool,

    #[arg(long = "time-grade", value_enum, default_value_t = Grade::T1)]
    pub time_grade: Grade,

    /// How long to run, in EMULATED time: `30s`, `1500ms`, `900us`. The
    /// machine's clock, not yours — a run is over when the guest has lived
    /// this long, however long that takes here.
    #[arg(long, default_value = "30s")]
    pub timeout: String,

    /// Host-side safety net, in wall-clock seconds. The only non-deterministic
    /// thing about a run, and it can only end one.
    #[arg(long = "wall-timeout", default_value_t = 300)]
    pub wall_timeout_secs: u64,

    /// Stop at the end of the first console line containing this text.
    #[arg(long = "exit-on")]
    pub exit_on: Option<String>,

    /// Write the console transcript here as well as to stderr.
    #[arg(long)]
    pub console: Option<PathBuf>,

    /// Write every WS281x frame decoded off a pad here, one JSON line each.
    #[arg(long = "dump-frames")]
    pub dump_frames: Option<PathBuf>,

    /// Write the raw pad transitions here.
    #[arg(long = "pin-log")]
    pub pin_log: Option<PathBuf>,

    /// A flash image file to boot from and write back to, so a project
    /// uploaded in one run is still there in the next. Ignored with
    /// `--merged`, which carries the whole chip already.
    #[arg(long)]
    pub flash: Option<PathBuf>,

    /// Refuse any access to an address no peripheral claims, instead of
    /// reading zero and carrying on.
    #[arg(long = "strict-bus")]
    pub strict_bus: bool,
}
