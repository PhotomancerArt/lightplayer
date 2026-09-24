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
    /// Hold N emulated boards and serve each as two WebSocket endpoints.
    Serve(ServeArgs),
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
    /// The kernel-measured class costs, plus the flash cache's fills and the
    /// APB's wait states.
    T3,
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

    /// The USB host's state at power-on, spelled as `emu serve --usb-host`
    /// and the `lp-emu-esp32c6` binary spell it.
    ///
    /// Omitted, it follows the link. When the USB-Serial-JTAG port IS the
    /// `--link` socket it is `attached-idle`: the cable is in, the port is
    /// closed until a client connects, and the client's connect is the
    /// `open`. Before that nothing is reading, so the firmware's own write
    /// timeouts latch "host not draining" and it drops what it would have
    /// sent, as a board on a desk does when no application has the port
    /// open. A late client gets at most what the 64-byte IN FIFO still holds,
    /// then fresh frames, never a replay of the boot console
    /// (`docs/defects/2026-09-23-emulated-usb-port-drains-with-no-client-attached.md`).
    ///
    /// With no `--link`, or a UART0 one, nothing couples a client to the
    /// USB port, so it stays `attached`: open and draining from power-on,
    /// the emulator itself the reader, which is what "boot this image and
    /// show me what it says" (`--console`) needs. `--monitor` implies the
    /// same and so refuses this flag.
    ///
    /// `attached` with a USB `--link` is the explicit opt-in to a reader
    /// present since power-on: every guest write succeeds with nobody
    /// connected, and the first client is replayed all of it (up to 4 MiB).
    /// `absent` is no cable: the firmware's connection monitor sees it
    /// unplugged.
    #[arg(long = "usb-host", value_enum, conflicts_with = "monitor")]
    pub usb_host: Option<UsbHostArg>,

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

    /// Write the radio TX log here: one line per frame the WiFi blob hands
    /// the MAC, as bytes. An observation, not an air — nothing is delivered
    /// and no interrupt is raised. The line's fields are in
    /// `lp-emu/esp/lp-emu-esp32c6/README.md`, "The radio TX log".
    #[arg(long = "tx-log")]
    pub tx_log: Option<PathBuf>,

    /// Scripted host input on the PADS, deterministic: `<us> pin <n> <0|1>`,
    /// the `after`/`then` walk forms, and the `button` / `encoder`
    /// generators. Repeatable; the files concatenate in the order given.
    /// The grammar is `lp-emu/esp/lp-emu-esp32c6/src/pinscript.rs`'s, and
    /// `lp-emu-esp32c6 --help` spells it out.
    #[arg(long = "pin-script")]
    pub pin_script: Vec<PathBuf>,

    /// Tie two pads before the guest starts, `<tx>:<rx>` — a jumper on the
    /// header. Repeatable and transitive. GPIO9, 12, 13, 16, 17 and 18 are
    /// refused; gpio18 on the TX side is the one exception.
    #[arg(long = "wire")]
    pub wire: Vec<String>,

    /// A flash image file to boot from and write back to, so a project
    /// uploaded in one run is still there in the next. Ignored with
    /// `--merged`, which carries the whole chip already.
    #[arg(long)]
    pub flash: Option<PathBuf>,

    /// Refuse any access to an address no peripheral claims, instead of
    /// reading zero and carrying on.
    #[arg(long = "strict-bus")]
    pub strict_bus: bool,

    /// The board's eFuse MAC, `a0:f2:62:87:b4:8c`. Defaults to the desk
    /// board's, which is what every transcript was captured against.
    #[arg(long = "efuse-mac")]
    pub efuse_mac: Option<String>,

    /// The rate a host on UART0 sends at, default 115200. UART0 carries no
    /// clock, so the pulse-width counters the mask ROM's baud auto-detection
    /// reads can only report a rate the run STATES — this states it. It
    /// changes the divisor the ROM computes and writes to `UART0.clkdiv` and
    /// nothing else: a scripted byte still lands when the script says it
    /// does. Applied before the power-on snapshot, so a reboot keeps it.
    #[arg(long = "uart0-baud")]
    pub uart0_baud: Option<u64>,

    /// `LPPERI_CLK_EN`'s power-on value, in hex — what a board's previous
    /// firmware left in the LP domain. Default: the PAC reset `7f800000`, a
    /// clean board.
    ///
    /// Bit 29 is `LP_ANA_I2C_CK_EN`. Clear it — the bench's induced board
    /// read `5f000000` — and the second-stage bootloader hangs on the LP
    /// analog master's busy bit before its first console line, the MWDT0
    /// flash-boot watchdog shoots it 0.325 s in, and the next boot lands in
    /// the same hang, because an HP reset does not reach the LP island. That
    /// is `docs/defects/2026-09-06-c6-first-flash-bootloader-hang-lp-analog-i2c-clock.md`
    /// reproduced with no board. `power-cycle` on a served board's control
    /// channel does not clear it either: the value IS the power-on value.
    #[arg(long = "lpperi-clk-en", value_name = "HEX", value_parser = parse_hex_u32)]
    pub lpperi_clk_en: Option<u32>,
}

/// The USB host's state at power-on, for `run` and for every board `serve`
/// holds. The same three the `lp-emu-esp32c6` binary's `--usb-host` takes,
/// spelled the same way, so nobody has to learn a second vocabulary for one
/// chip.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum UsbHostArg {
    /// Cable in, port open and draining from power-on. An explicit opt-in
    /// wherever a byte socket is the port: the emulated host reads from
    /// power-on whether or not anybody is connected, so the first byte
    /// client is replayed everything the board wrote before it (up to
    /// `TCP_BACKLOG_CAP`). `run`'s default when no socket is the port.
    Attached,
    /// Cable in, port CLOSED. The byte client's connect is what opens it.
    /// The default wherever a byte socket is the port: no client means
    /// nobody is reading.
    #[default]
    #[value(name = "attached-idle")]
    AttachedIdle,
    /// No cable. On `serve`, an `attach` on the control channel is the
    /// plug-in edge; `run` has no control channel, so it stays unplugged.
    Absent,
}

impl UsbHostArg {
    /// The machine's power-on host state this spelling names.
    pub fn usb_host(self) -> lp_emu_esp32c6::machine::UsbHost {
        use lp_emu_esp32c6::machine::UsbHost;
        match self {
            UsbHostArg::Attached => UsbHost::Attached { draining: true },
            UsbHostArg::AttachedIdle => UsbHost::Attached { draining: false },
            UsbHostArg::Absent => UsbHost::Absent,
        }
    }
}

/// `lp-cli emu serve` — a registry of named boards behind a WebSocket door.
///
/// `run` is one image, one socket and a deadline; `serve` outlives any one
/// board and is what a browser (and `lp-cli upload … serial:ws://…`) talks
/// to. See `commands/emu/serve/mod.rs` for the shape of the door.
#[derive(Debug, Args)]
pub struct ServeArgs {
    #[arg(long, value_enum, default_value_t = EmuChip::Esp32C6)]
    pub chip: EmuChip,

    /// A board:
    /// `<id>=<image>[,mac=<aa:bb:cc:dd:ee:ff>][,kind=elf|merged|rom-up]`.
    /// Repeatable, and the whole point — `s9-two-boards` is about two
    /// identities, so every board gets its own MAC (the desk board's with
    /// the last octet stepped, unless `mac=` says otherwise) and its own
    /// flash file under `--state-dir`.
    ///
    /// Three kinds, and they differ in which entry the hart takes and
    /// whether the chip keeps its writes:
    ///
    /// * `kind=elf` (the default) — a firmware ELF loaded straight into
    ///   memory at its entry point, with a persistent flash part beside it.
    ///   Fast; the running image and the flash file are independent.
    /// * `kind=merged` — a whole merged flash image booted from the reset
    ///   vector through the real mask ROM, READ-ONLY: it is the image a gate
    ///   named, so it ignores `--state-dir`.
    /// * `kind=rom-up` — the reset vector out of the board's OWN flash file,
    ///   which keeps its writes. The only kind that can be flashed (by
    ///   esptool-js, espflash, esptool) and then boot what was written. The
    ///   image is a whole-chip image the flash file is SEEDED from the first
    ///   time; `blank` means a chip with nothing on it, which reaches the
    ///   mask ROM's download console by itself. (A file actually named
    ///   `blank` is still reachable as `./blank`.)
    #[arg(long = "board", value_name = "ID=IMAGE[,OPTS]")]
    pub board: Vec<String>,

    /// Where the door listens. `127.0.0.1:0` takes an ephemeral port and
    /// prints it, which is what a test and a second server want.
    #[arg(long, default_value = "127.0.0.1:5599")]
    pub listen: String,

    /// A directory holding one persistent flash file per board,
    /// `<id>.flash.bin` (PD8). Written back on a cadence, whenever a byte
    /// client closes the port, and on shutdown — so blank → flash → loaded
    /// is a sequence rather than three unrelated runs. Without it every
    /// board boots blank and forgets.
    #[arg(long = "state-dir")]
    pub state_dir: Option<PathBuf>,

    /// A directory to write each board's console transcript into,
    /// `<id>.console.log` — `run`'s `--console`, once per board.
    ///
    /// Everything the board said on its link since power-on, whether or not
    /// anyone was listening at the time, rewritten on the same cadence the
    /// flash is written back. A serve with no byte client still has a
    /// console; this is where to read it.
    #[arg(long = "console-dir")]
    pub console_dir: Option<PathBuf>,

    #[arg(long = "time-grade", value_enum, default_value_t = Grade::T1)]
    pub time_grade: Grade,

    /// The USB host's state at power-on, spelled as `lp-emu-esp32c6
    /// --usb-host` spells it.
    ///
    /// `attached-idle` is the default: the cable is in and the port is
    /// CLOSED until a byte client connects. The client's connect is the
    /// `open` and its disconnect is the `close`, provable through `state`.
    /// Before the first client nothing is reading, so the firmware's own
    /// write timeouts latch "host not draining" and it drops what it would
    /// have sent, exactly as a board on a desk does when no application has
    /// the port open. A client that connects later gets at most what the
    /// 64-byte IN FIFO still holds, then fresh frames — never a replay of
    /// the boot console or of the heartbeats nobody read
    /// (`docs/defects/2026-09-23-emulated-usb-port-drains-with-no-client-attached.md`).
    /// The boot console is not lost: `--console-dir` writes it to
    /// `<id>.console-untaken.log`.
    ///
    /// `attached` is the explicit opt-in to the old behaviour: the port is open and draining from power-on with nobody
    /// connected, so the firmware's writes all succeed and the first byte
    /// client is replayed everything written before it (up to 4 MiB). That
    /// is "an application had the port open since power-on", which is a
    /// state a real board can be in but not what an unopened port does.
    ///
    /// `absent` is no cable at all.
    #[arg(long = "usb-host", value_enum, default_value_t = UsbHostArg::AttachedIdle)]
    pub usb_host: UsbHostArg,

    /// Refuse any access to an address no peripheral claims.
    #[arg(long = "strict-bus")]
    pub strict_bus: bool,

    /// Serve the boards' radio frames on this TCP address, in the `LPA1`
    /// wire codec.
    ///
    /// AUDITABLE ONLY. One way: frames the boards' radios hand over are
    /// written to whoever is watching, and nothing is ever delivered into a
    /// board from it. A run that used this is NOT a transcript — the
    /// deterministic form of an air is `lp-emu-esp32c6`'s in-process
    /// lockstep runner, and that is the only form any transcript,
    /// validation configuration or CI job ever uses.
    #[arg(long = "air")]
    pub air: Option<String>,

    /// `LPPERI_CLK_EN`'s power-on value, in hex — what a board's previous
    /// firmware left in the LP domain. Default: the PAC reset `7f800000`, a
    /// clean board.
    ///
    /// Bit 29 is `LP_ANA_I2C_CK_EN`. Clear it — the bench's induced board
    /// read `5f000000` — and the second-stage bootloader hangs on the LP
    /// analog master's busy bit before its first console line, the MWDT0
    /// flash-boot watchdog shoots it 0.325 s in, and the next boot lands in
    /// the same hang, because an HP reset does not reach the LP island. That
    /// is `docs/defects/2026-09-06-c6-first-flash-bootloader-hang-lp-analog-i2c-clock.md`
    /// reproduced with no board. `power-cycle` on a served board's control
    /// channel does not clear it either: the value IS the power-on value.
    #[arg(long = "lpperi-clk-en", value_name = "HEX", value_parser = parse_hex_u32)]
    pub lpperi_clk_en: Option<u32>,
}

/// `5f000000` or `0x5f000000` → a `u32`. Hex without a prefix, because that
/// is how a register value is read out of a trace and pasted back in.
fn parse_hex_u32(text: &str) -> Result<u32, String> {
    let body = text.strip_prefix("0x").unwrap_or(text);
    u32::from_str_radix(body, 16).map_err(|e| format!("`{text}` is not a 32-bit hex word: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_register_word_reads_with_or_without_the_prefix() {
        assert_eq!(parse_hex_u32("5f000000"), Ok(0x5f00_0000));
        assert_eq!(parse_hex_u32("0x7f800000"), Ok(0x7f80_0000));
        assert!(parse_hex_u32("nope").is_err());
        // Wider than the register, so it is a mistake and not a truncation.
        assert!(parse_hex_u32("1_0000_0000").is_err());
    }
}
