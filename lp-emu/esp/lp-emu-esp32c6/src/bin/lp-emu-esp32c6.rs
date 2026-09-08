//! `lp-emu-esp32c6` — run a C6 image on the machine.
//!
//! Every timeout is **emulated** time, so a run is the same run on a laptop
//! and on a loaded CI box. `--wall-timeout` is the single wall-clock input
//! and it is a safety net: it can end a run, never change one.
//!
//! The argument parser is written out by hand rather than pulled from a
//! crate. The flag list is fixed and small, the exit codes are a contract
//! (0 / 2 / 3 / 4), and the error text is part of what a bring-up session
//! reads — all three are easier to keep exactly right in fifty lines than
//! through a derive.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use lp_emu_esp_common::pins::PadId;
use lp_emu_esp_common::{RegGrade, ScriptedSource};
use lp_emu_esp32c6::control::parse_usb_script;
use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::loader::EfuseIdentity;
use lp_emu_esp32c6::machine::{
    AppSource, Esp32C6Builder, Esp32C6Machine, FrameSink, Outcome, PinLogSink, RomSource,
    StopCondition, StripConfig, TimeGrade, Uart0Sink, UsbHost, UsbSjDrain, UsbSjSink,
};
use lp_emu_esp32c6::memmap;
use lp_emu_esp32c6::periph::rmt::RefillStats;
use lp_emu_esp32c6::pinscript::{parse_pin_script, parse_wire};

const USAGE: &str = "\
lp-emu-esp32c6 — the ESP32-C6 machine

USAGE:
    lp-emu-esp32c6 --elf <app.elf> [options]
    lp-emu-esp32c6 --merged <chip.bin> [--elf <app.elf>] [options]
    lp-emu-esp32c6 --hooks

OPTIONS:
    --elf <path>            the application image to direct-load. With
                            --merged it is never loaded: it is the symbol
                            table --probe/--break-at read and the image the
                            boot report cross-checks against
    --merged <path>         boot the chip from its RESET VECTOR with these
                            bytes in flash: the second-stage bootloader at
                            0x0, the partition table at 0x8000, the app in
                            the factory partition — the layout a flasher
                            writes (scripts/emu/build-merged-image.sh). The
                            real mask ROM and the real bootloader do the
                            loading. Read-only; implies the chip's size
    --rom <path>            a mask ROM ELF (default: the vendored C6 rev0 image)
    --time-grade t1|t2      t1 = instruction count, t2 = the per-class model [t1]
    --timeout <5s|1500ms|900us>
                            EMULATED time to run for [100ms]
    --wall-timeout <s>      host-clock safety net; exits 4
    --exit-on <substr>      stop at the end of the line this appears on, on
                            EITHER console (UART0 or the USB link — the
                            shipped image's console is the USB one)
    --uart0 stdout|file:<path>|tcp:<host:port>
                            where UART0's bytes go; tcp: LISTENS, one client at a
                            time, and the client's bytes are UART0's RX (not
                            deterministic: wall clock decides their cycle)
    --uart0-script <file>   scripted host input for UART0, deterministic:
                            one chunk per line, either at an EMULATED time
                            (`1500 \"M!{...}\\n\"`, `2000 4d 21 0a`) or waiting
                            for something the device said
                            (`after \"boot complete\" +5ms \"M!{...}\\n\"`).
                            Strings take \\n \\r \\t \\0 \\\\ \\\" \\xNN escapes.
    --flash <file>          the flash chip's bytes: read at start, written back
                            at exit (created blank if absent) — the board's
                            flash, surviving a run
    --flash-copy <file>     the same file read once and never written
    --flash-size <4M|8M>    the modelled chip's size [4M]
    --usb-host absent|attached|attached-idle
                            the USB-Serial-JTAG host at power-on: no cable
                            (P6's machine), a host with the port open and
                            draining, or a host with the port closed [absent]
    --usb-sj stderr|file:<path>|tcp:<host:port>
                            where the bytes a USB host RECEIVED go — IN
                            packets a draining host took [kept in memory,
                            summarised at exit]. tcp: LISTENS, one client at
                            a time, and the client's bytes are the OUT
                            endpoint's source: `lp-cli … serial:tcp://<addr>`
                            connects to it (not deterministic — wall clock
                            decides which cycle a byte lands on)
    --usb-sj-drain auto|manual
                            whether a client on that socket means an
                            application opened the port: auto couples
                            connect/disconnect to open/close, manual leaves
                            both to the control channel [auto]. A cable is
                            never implied — attach/detach are control
                            commands
    --control tcp:<host:port>
                            LISTEN for the host control channel: one line
                            per command, one reply per command, in the
                            scripted fake device's vocabulary (attach,
                            detach, open, close, dtr, rts, signals, reset,
                            download-mode, state, usb-write). Not the wire:
                            no M! frame is ever sent or expected here.
                            Protocol: lp-emu/esp/README.md
    --usb-script <file>     scripted host input on the USB link,
                            deterministic: --uart0-script's grammar plus the
                            control words above, one entry per line
                            (`0 attach`, `1500 \"M!{...}\\n\"`, `6000 detach`,
                            `0 wait 500`, and the walk forms
                            `after \"boot complete\" \"M!{...}\\n\"` /
                            `then +2ms \"…\"`, whose needle is matched against
                            what a host on THIS link received). Byte lines
                            feed the OUT endpoint; file order is wire order.
                            Repeatable: the files concatenate in the order
                            given, so a scenario's cable schedule and a
                            walk's wire conversation stay separate files
    --usb-sj-tried stderr|file:<path>
                            the observation stream: bytes the guest handed to
                            the IN endpoint that no host took (pushed with no
                            host, dropped into a committed FIFO, dropped by a
                            bus reset) [kept in memory, summarised at exit]
    --dump-frames stdout|file:<path>
                            one JSON line per WS281x frame decoded off a
                            routed pad, as it completes: pad, signal, frame
                            number, start/end in us, bits, leds, the wire
                            bytes AND the same bytes unpermuted to the RGB
                            the driver was handed, errors, the reset gap.
                            Frames are always also kept in memory and
                            summarised at exit
    --strip-order grb|rgb|rbg|gbr|brg|bgr
                            the byte order `rgb` is unpermuted with [grb]
    --strip-timing ws2812|ws2811
                            the wire timing a pad is decoded against
                            (400/800 ns vs 300/900 ns highs) [ws2812]
    --pin-log file:<path>   every edge on every routed pad: `<us> gpio18 0|1`.
                            12,288 lines per 256-LED frame — never a default,
                            capped at 2,000,000 lines
    --pin-script <file>     scripted host input on the PADS, deterministic:
                            `<us> pin <n> <0|1>` at absolute guest time, plus
                            --usb-script's walk forms `after \"<line>\" pin …`
                            and `then +<ms> pin …`, plus the generators
                            `button <n> press at <us> [bounce <k> edges over
                            <us>] hold <ms>` and `encoder <a> <b> <steps>
                            cw|ccw from <us> at <hz>`. The leading number is
                            MICROSECONDS here (a contact bounce is tens of
                            them); `us` and `ms` suffixes are accepted.
                            Repeatable: the files concatenate in the order
                            given. GPIO9, 12, 13, 16, 17 and 18 are refused
    --wire <a>:<b>          tie two pads before the guest starts, so whatever
                            <a> carries <b> carries — a jumper on the header.
                            Repeatable, and transitive. `<a>` is the TX side
                            and is the only place gpio18 is allowed
                            (`--wire 18:19`, the strip pad into an RX pad)
    --reset-cause poweron|usb-uart
                            what LP_CLKRST.reset_cause says, and so what the
                            mask ROM prints as `rst:0x..`: a cold chip, or a
                            host's DTR/RTS dance on the serial bridge (the
                            silicon boot transcripts were captured after one
                            of those) [poweron]
    --reboot-on-reset       PERFORM a reset request instead of reporting it:
                            reboot the chip into the strap the request names
                            (the USB reset dance, the RWDT's stage action) and
                            carry on. Off by default — three recorded
                            scenarios read the exit code as their evidence.
                            Needs a boot chain, so it is only useful with
                            --merged
    --strap app|download    where the strapping pins were at reset, and so
                            what the ROM prints as `boot:0x..`: the flash
                            bootloader, or the ROM's own download console
                            [app]
    --efuse-mac <a0:f2:..>  the MAC the eFuse block reports [the desk board]
    --efuse-rev <0.2>       wafer major.minor [0.2]
    --seed <u64>            the machine PRNG's seed [0]
    --trace [BLOCK,BLOCK]   log every MMIO access; an optional block filter
    --trace-file <path>     write the trace here instead of stderr
    --strict-bus            an access nothing claims is fatal; exits 3
    --strict-grade modeled|documented|measured
                            an access to a register graded BELOW this level is
                            fatal (exits 3): `documented` stops on a register
                            whose behaviour is only our reading of the PAC,
                            `measured` on anything no transcript has proved.
                            It applies to the blocks that PUBLISH a grade
                            table — USB_DEVICE today — and passes over the
                            rest, because `nobody graded this block` is not
                            the same statement as `this block is modelled`.
                            The report names the blocks it checked. Tables
                            live in each block's file header
    --probe <symbol>@<ms>   print a static's word at an emulated time
    --break-at <symbol>     stop when the symbol is entered; print a0..a7, sp
                            and the backtrace; exits 5
    --hooks                 list the ROM hook table and exit
    --map                   print the memory map and exit
    -h, --help              this

EXIT CODES:
    0  --exit-on matched, or the emulated timeout was reached with no fault
    2  the hart faulted
    3  a strict-bus violation
    4  the wall-clock safety net fired
    5  a --break-at symbol was reached
";

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    match run() {
        Ok(code) => code,
        Err(message) => {
            eprintln!("lp-emu-esp32c6: {message}");
            ExitCode::from(64)
        }
    }
}

#[derive(Default)]
struct Args {
    elf: Option<PathBuf>,
    merged: Option<PathBuf>,
    rom: Option<PathBuf>,
    time_grade: TimeGrade,
    timeout: Option<u64>,
    wall_timeout: Option<Duration>,
    exit_on: Option<String>,
    uart0: Uart0Sink,
    uart0_script: Option<PathBuf>,
    usb_sj: UsbSjSink,
    flash: FlashBacking,
    flash_len: Option<u32>,
    usb_sj_tried: UsbSjSink,
    usb_host: UsbHost,
    usb_sj_drain: UsbSjDrain,
    usb_script: Vec<PathBuf>,
    pin_script: Vec<PathBuf>,
    wires: Vec<(PadId, PadId)>,
    control: Option<String>,
    efuse: EfuseIdentity,
    reset_cause: lp_emu_esp32c6::loader::ResetCause,
    /// `Option` only because `Args` derives `Default` and a strapping
    /// word has no neutral value; `None` is the app strap.
    strap: Option<lp_emu_esp_common::Strap>,
    reboot_on_reset: bool,
    seed: u64,
    trace: bool,
    trace_blocks: Vec<String>,
    trace_file: Option<PathBuf>,
    strict: bool,
    strict_grade: Option<RegGrade>,
    probes: Vec<(u64, String)>,
    break_at: Vec<String>,
    hooks: bool,
    map: bool,
    dump_frames: FrameSink,
    pin_log: PinLogSink,
    strip: StripConfig,
}

fn run() -> Result<ExitCode, String> {
    let args = parse(std::env::args().skip(1).collect())?;

    if args.map {
        print_map();
        return Ok(ExitCode::SUCCESS);
    }

    let mut builder = Esp32C6Builder::new()
        .time_grade(args.time_grade)
        .strict(args.strict)
        .strict_grade(args.strict_grade)
        .efuse(args.efuse)
        .reset_cause(args.reset_cause)
        .strap(args.strap.unwrap_or(lp_emu_esp_common::Strap::App))
        .reboot_on_reset(args.reboot_on_reset)
        .seed(args.seed)
        .uart0(args.uart0.clone())
        .usb_sj(args.usb_sj.clone())
        .flash(args.flash.clone())
        .usb_sj_tried(args.usb_sj_tried.clone())
        .usb_host(args.usb_host)
        .usb_sj_drain(args.usb_sj_drain)
        .dump_frames(args.dump_frames.clone())
        .pin_log(args.pin_log.clone())
        .strip(args.strip.order, args.strip.timing);

    if let Some(len) = args.flash_len {
        builder = builder.flash_len(len);
    }
    if let Some(addr) = args.control.clone() {
        builder = builder.control(addr);
    }
    // Repeatable, and the files concatenate in the order given: a scenario's
    // cable schedule and a walk's wire conversation are two different
    // scripts of two different lengths (the first is three lines written
    // inline by the runner, the second a 12 KB generated file), and a link
    // that carries both should not force them into one.
    if !args.usb_script.is_empty() {
        let mut bytes = ScriptedSource::new();
        let mut commands = Vec::new();
        for path in &args.usb_script {
            let text = std::fs::read_to_string(path)
                .map_err(|e| format!("reading {}: {e}", path.display()))?;
            let script = parse_usb_script(&text).map_err(|e| format!("{}: {e}", path.display()))?;
            bytes.extend(script.bytes);
            commands.extend(script.commands);
        }
        commands.sort_by_key(|(at, _)| *at);
        eprintln!(
            "usb script: {} byte(s) of host input in {} chunk(s) and {} control command(s)",
            bytes.remaining(),
            bytes.chunks(),
            commands.len()
        );
        builder = builder.usb_script(commands);
        builder = builder.usb_script_source(bytes);
    }

    for (a, b) in &args.wires {
        eprintln!("wire: {a} -> {b}");
        builder = builder.wire(*a, *b);
    }
    if !args.pin_script.is_empty() {
        let mut script = lp_emu_esp32c6::pinscript::PinScript::new();
        for path in &args.pin_script {
            let text = std::fs::read_to_string(path)
                .map_err(|e| format!("reading {}: {e}", path.display()))?;
            script.extend(parse_pin_script(&text).map_err(|e| format!("{}: {e}", path.display()))?);
        }
        let pads: Vec<String> = script.pads().iter().map(|p| format!("gpio{p}")).collect();
        eprintln!(
            "pin script: {} level(s) in {} step(s) on {}",
            script.remaining(),
            script.steps_left(),
            if pads.is_empty() {
                "no pad".to_string()
            } else {
                pads.join(", ")
            }
        );
        builder = builder.pin_script(script);
    }

    if let Some(rom) = args.rom.clone() {
        builder = builder.rom(RomSource::Path(rom));
    }
    if let Some(elf) = args.elf.clone() {
        builder = builder.app(AppSource::Path(elf));
    } else if !args.hooks && args.merged.is_none() {
        return Err("--elf or --merged is required (or --hooks / --map)".to_string());
    }
    if let Some(image) = args.merged.clone() {
        if !matches!(args.flash, FlashBacking::Blank) {
            return Err(
                "--merged is the chip's bytes; --flash / --flash-copy would be a second chip"
                    .to_string(),
            );
        }
        let len = std::fs::metadata(&image)
            .map_err(|e| format!("{}: {e}", image.display()))?
            .len();
        let len = u32::try_from(len).map_err(|_| {
            format!(
                "{} is {len} bytes; no chip this machine models is that large",
                image.display()
            )
        })?;
        if !len.is_power_of_two() {
            return Err(format!(
                "{} is {len} bytes, which is not a whole chip — a merged image is the WHOLE \
                 flash part, padded to its size (espflash `save-image --merge` does this)",
                image.display()
            ));
        }
        // Read-only on purpose: a merged image is an input, and a boot that
        // wrote back into it would quietly stop being the image the gate
        // named. `--flash` is how a run keeps its writes.
        builder = builder
            .boot_mode(lp_emu_esp32c6::machine::BootMode::RomUp)
            .flash(FlashBacking::Copy(image))
            .flash_len(args.flash_len.unwrap_or(len));
    }
    if args.trace {
        let sink: Box<dyn std::io::Write + Send> = match &args.trace_file {
            Some(path) => Box::new(
                std::fs::File::create(path)
                    .map_err(|e| format!("creating {}: {e}", path.display()))?,
            ),
            None => Box::new(std::io::stderr()),
        };
        builder = builder.trace(sink, args.trace_blocks.clone());
    }
    if let Some(script) = &args.uart0_script {
        let text = std::fs::read_to_string(script)
            .map_err(|e| format!("reading {}: {e}", script.display()))?;
        let source = parse_uart0_script(&text).map_err(|e| format!("{}: {e}", script.display()))?;
        builder = builder.uart0_script(source);
    }

    let mut machine = builder.build().map_err(|e| e.to_string())?;

    if args.hooks {
        print_hooks(&machine);
        return Ok(ExitCode::SUCCESS);
    }

    for symbol in &args.break_at {
        let at = machine
            .break_at(symbol)
            .map_err(|e| format!("--break-at {symbol}: {e}"))?;
        eprintln!("break-at: {symbol} @ {at:#010x}");
    }

    print_load_report(&machine);

    let stop = StopCondition {
        stop_cycle: Some(args.timeout.unwrap_or(100_000) * memmap::CYCLES_PER_US),
        exit_on: args.exit_on.clone(),
        wall_timeout: args.wall_timeout,
        probes: args
            .probes
            .iter()
            .map(|(ms, sym)| (ms * 1_000 * memmap::CYCLES_PER_US, sym.clone()))
            .collect(),
    };

    let outcome = machine.run_until(&stop);
    // The run is over: a frame still open on a pad is reported as
    // incomplete rather than silently dropped.
    machine.flush_frames();
    report(&mut machine, &outcome);
    Ok(ExitCode::from(outcome.exit_code() as u8))
}

fn parse(argv: Vec<String>) -> Result<Args, String> {
    let mut args = Args::default();
    let mut i = 0;
    while i < argv.len() {
        let flag = argv[i].as_str();
        let mut value = |name: &str| -> Result<String, String> {
            i += 1;
            argv.get(i)
                .cloned()
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match flag {
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            "--elf" => args.elf = Some(value("--elf")?.into()),
            "--merged" => args.merged = Some(value("--merged")?.into()),
            "--rom" => args.rom = Some(value("--rom")?.into()),
            "--time-grade" => args.time_grade = TimeGrade::parse(&value("--time-grade")?)?,
            "--timeout" => args.timeout = Some(parse_duration_us(&value("--timeout")?)?),
            "--wall-timeout" => {
                let text = value("--wall-timeout")?;
                let secs: f64 = text
                    .trim_end_matches('s')
                    .parse()
                    .map_err(|e| format!("--wall-timeout `{text}`: {e}"))?;
                args.wall_timeout = Some(Duration::from_secs_f64(secs));
            }
            "--exit-on" => args.exit_on = Some(value("--exit-on")?),
            "--uart0" => args.uart0 = parse_uart0(&value("--uart0")?)?,
            "--uart0-script" => args.uart0_script = Some(value("--uart0-script")?.into()),
            "--usb-sj" => args.usb_sj = parse_usb_sj(&value("--usb-sj")?)?,
            "--flash" => args.flash = FlashBacking::File(value("--flash")?.into()),
            "--flash-copy" => args.flash = FlashBacking::Copy(value("--flash-copy")?.into()),
            "--flash-size" => {
                let text = value("--flash-size")?;
                args.flash_len = Some(parse_flash_size(&text)?);
            }
            "--usb-sj-tried" => args.usb_sj_tried = parse_usb_sj(&value("--usb-sj-tried")?)?,
            "--usb-host" => {
                let text = value("--usb-host")?;
                args.usb_host = UsbHost::parse(&text).ok_or_else(|| {
                    format!("--usb-host `{text}`: expected absent, attached or attached-idle")
                })?;
            }
            "--usb-sj-drain" => {
                let text = value("--usb-sj-drain")?;
                args.usb_sj_drain = UsbSjDrain::parse(&text)
                    .ok_or_else(|| format!("--usb-sj-drain `{text}`: expected auto or manual"))?;
            }
            "--usb-script" => args.usb_script.push(value("--usb-script")?.into()),
            "--pin-script" => args.pin_script.push(value("--pin-script")?.into()),
            "--wire" => {
                let text = value("--wire")?;
                args.wires
                    .push(parse_wire(&text).map_err(|e| format!("--wire: {e}"))?);
            }
            "--control" => {
                let text = value("--control")?;
                args.control = Some(parse_control(&text)?);
            }
            "--strict-grade" => {
                let text = value("--strict-grade")?;
                args.strict_grade = Some(RegGrade::parse(&text).ok_or_else(|| {
                    format!("--strict-grade `{text}`: expected modeled, documented or measured")
                })?);
            }
            "--reset-cause" => {
                let text = value("--reset-cause")?;
                args.reset_cause =
                    lp_emu_esp32c6::loader::ResetCause::parse(&text).ok_or_else(|| {
                        format!("--reset-cause `{text}`: expected poweron or usb-uart")
                    })?;
            }
            "--reboot-on-reset" => args.reboot_on_reset = true,
            "--strap" => {
                let text = value("--strap")?;
                args.strap = Some(match text.as_str() {
                    "app" => lp_emu_esp_common::Strap::App,
                    "download" => lp_emu_esp_common::Strap::Download,
                    other => return Err(format!("--strap `{other}`: expected app or download")),
                });
            }
            "--efuse-mac" => args.efuse.mac = EfuseIdentity::parse_mac(&value("--efuse-mac")?)?,
            "--efuse-rev" => {
                let (major, minor) = EfuseIdentity::parse_rev(&value("--efuse-rev")?)?;
                args.efuse.wafer_major = major;
                args.efuse.wafer_minor = minor;
            }
            "--seed" => {
                let text = value("--seed")?;
                args.seed = text.parse().map_err(|e| format!("--seed `{text}`: {e}"))?;
            }
            "--trace" => {
                args.trace = true;
                // The block filter is optional and must not swallow the next
                // flag: `--trace --strict-bus` has no filter.
                if let Some(next) = argv.get(i + 1)
                    && !next.starts_with('-')
                {
                    i += 1;
                    args.trace_blocks = next.split(',').map(str::to_string).collect();
                }
            }
            "--trace-file" => {
                args.trace = true;
                args.trace_file = Some(value("--trace-file")?.into());
            }
            "--dump-frames" => args.dump_frames = parse_dump_frames(&value("--dump-frames")?)?,
            "--pin-log" => args.pin_log = parse_pin_log(&value("--pin-log")?)?,
            "--strip-order" => {
                let text = value("--strip-order")?;
                args.strip.order = StripConfig::parse_order(&text).ok_or_else(|| {
                    format!("--strip-order `{text}`: expected grb, rgb, rbg, gbr, brg or bgr")
                })?;
            }
            "--strip-timing" => {
                let text = value("--strip-timing")?;
                args.strip.timing = StripConfig::parse_timing(&text)
                    .ok_or_else(|| format!("--strip-timing `{text}`: expected ws2812 or ws2811"))?;
            }
            "--strict-bus" => args.strict = true,
            "--probe" => args.probes.push(parse_probe(&value("--probe")?)?),
            "--break-at" => args.break_at.push(value("--break-at")?),
            "--hooks" => args.hooks = true,
            "--map" => args.map = true,
            other => return Err(format!("unknown flag `{other}`\n\n{USAGE}")),
        }
        i += 1;
    }
    Ok(args)
}

/// `4M`, `8M`, `524288` → bytes. A flash size that is not a power of two is
/// refused: the JEDEC capacity byte is an exponent, so there is no honest
/// id for one.
fn parse_flash_size(text: &str) -> Result<u32, String> {
    let (digits, scale) = match text.strip_suffix(['M', 'm']) {
        Some(d) => (d, 1024 * 1024u32),
        None => match text.strip_suffix(['K', 'k']) {
            Some(d) => (d, 1024),
            None => (text, 1),
        },
    };
    let len: u32 = digits
        .parse::<u32>()
        .map_err(|e| format!("--flash-size `{text}`: {e}"))?
        .checked_mul(scale)
        .ok_or_else(|| format!("--flash-size `{text}` overflows"))?;
    if !len.is_power_of_two() || len < 64 * 1024 {
        return Err(format!(
            "--flash-size `{text}` is {len} bytes; it must be a power of two of at least 64 KiB \
             (the JEDEC capacity byte is an exponent and the cache pages by 64 KiB)"
        ));
    }
    Ok(len)
}

/// `5s`, `1500ms`, `900us` → microseconds.
fn parse_duration_us(text: &str) -> Result<u64, String> {
    let (digits, scale) = if let Some(d) = text.strip_suffix("ms") {
        (d, 1_000u64)
    } else if let Some(d) = text.strip_suffix("us") {
        (d, 1)
    } else if let Some(d) = text.strip_suffix('s') {
        (d, 1_000_000)
    } else {
        return Err(format!(
            "`{text}` has no unit — write 5s, 1500ms or 900us (emulated time)"
        ));
    };
    let n: u64 = digits.parse().map_err(|e| format!("`{text}`: {e}"))?;
    Ok(n * scale)
}

fn parse_uart0(text: &str) -> Result<Uart0Sink, String> {
    match text {
        "stdout" => Ok(Uart0Sink::Stdout),
        "memory" => Ok(Uart0Sink::Memory),
        other => match other.split_once(':') {
            Some(("file", path)) => Ok(Uart0Sink::File(path.into())),
            Some(("tcp", addr)) if addr.contains(':') => Ok(Uart0Sink::Tcp(addr.to_string())),
            Some(("tcp", addr)) => Err(format!(
                "--uart0 tcp:{addr}: write tcp:<host:port>, e.g. tcp:127.0.0.1:5555"
            )),
            _ => Err(format!(
                "`{other}` is not a UART0 destination (stdout, memory, file:<path>, tcp:<host:port>)"
            )),
        },
    }
}

fn parse_dump_frames(text: &str) -> Result<FrameSink, String> {
    match text {
        "stdout" => Ok(FrameSink::Stdout),
        "memory" => Ok(FrameSink::Memory),
        other => match other.split_once(':') {
            Some(("file", path)) => Ok(FrameSink::File(path.into())),
            _ => Err(format!(
                "`{other}` is not a frame destination (stdout, memory, file:<path>)"
            )),
        },
    }
}

fn parse_pin_log(text: &str) -> Result<PinLogSink, String> {
    match text.split_once(':') {
        Some(("file", path)) => Ok(PinLogSink::File(path.into())),
        _ => Err(format!(
            "`{text}` is not a pin-log destination (file:<path>); the log is an edge per line \
             and never goes to a console"
        )),
    }
}

fn parse_usb_sj(text: &str) -> Result<UsbSjSink, String> {
    match text {
        "stderr" => Ok(UsbSjSink::Stderr),
        "memory" => Ok(UsbSjSink::Memory),
        other => match other.split_once(':') {
            Some(("file", path)) => Ok(UsbSjSink::File(path.into())),
            Some(("tcp", addr)) if addr.contains(':') => Ok(UsbSjSink::Tcp(addr.to_string())),
            Some(("tcp", addr)) => Err(format!(
                "--usb-sj tcp:{addr}: write tcp:<host:port>, e.g. tcp:127.0.0.1:5556"
            )),
            _ => Err(format!(
                "`{other}` is not a USB-SJ destination (stderr, memory, file:<path>, \
                 tcp:<host:port>)"
            )),
        },
    }
}

/// `--control tcp:<host:port>`. Only `tcp:` exists: a control channel with
/// nobody on the other end is a flag with no effect, and the scripted form
/// (`--usb-script`) is the file-shaped way to say the same thing.
fn parse_control(text: &str) -> Result<String, String> {
    match text.split_once(':') {
        Some(("tcp", addr)) if addr.contains(':') => Ok(addr.to_string()),
        Some(("tcp", addr)) => Err(format!(
            "--control tcp:{addr}: write tcp:<host:port>, e.g. tcp:127.0.0.1:5557"
        )),
        _ => Err(format!(
            "`{text}` is not a control channel (tcp:<host:port>; --usb-script is the \
             deterministic form)"
        )),
    }
}

/// The `--uart0-script` grammar.
///
/// Two kinds of line, each ending in the bytes to send — a double-quoted
/// string with `\n \r \t \0 \\ \" \xNN` escapes, or whitespace-separated hex
/// bytes:
///
/// ```text
/// # at 1500 ms of EMULATED time
/// 1500 "M!{...}\n"
/// 2000 4d 21 0a
///
/// # 5 ms after the device says this, whatever cycle that lands on
/// after "[RECOVERY] boot complete" +5ms "M!{...}\n"
/// after "\"stopAllProjects\"" "M!{...}\n"
///
/// # 2 ms after the previous chunk finished — a host that paces itself
/// then +2ms "…the next 64 bytes…"
/// ```
///
/// `after` is what makes a walk a walk: a host client sends its next
/// request when the answer to the last one arrives, not at a wall-clock
/// offset. The wait is still resolved entirely in guest time (see
/// [`ScriptedSource`]), so two runs of the same script against the same
/// image deliver the same bytes at the same cycles.
///
/// The needle is matched against the device's UART0 output *after* the
/// previous step, so the same line can be waited for twice.
///
/// The grammar itself lives in [`lp_emu_esp32c6::control`], because
/// `--usb-script` is the same grammar with the control words added and one
/// walk file has to replay on either link (M6 P5).
fn parse_uart0_script(text: &str) -> Result<ScriptedSource, String> {
    lp_emu_esp32c6::control::parse_byte_script(text)
}

/// `TIMED_OUT@120` → (120 ms, "TIMED_OUT").
fn parse_probe(text: &str) -> Result<(u64, String), String> {
    let (symbol, at) = text
        .rsplit_once('@')
        .ok_or_else(|| format!("--probe `{text}` is not <symbol>@<ms>"))?;
    let ms: u64 = at
        .trim_end_matches("ms")
        .parse()
        .map_err(|e| format!("--probe `{text}`: {e}"))?;
    Ok((ms, symbol.to_string()))
}

fn print_map() {
    println!("ESP32-C6 memory map (esp-hal-1.1.1/ld/esp32c6/memory.x)");
    for span in memmap::RAM_SPANS {
        println!(
            "  {:<12} 0x{:08x}..0x{:08x}  {:>9} bytes",
            span.name,
            span.base,
            span.end(),
            span.len
        );
    }
    for span in memmap::MMIO_WINDOWS {
        println!(
            "  {:<12} 0x{:08x}..0x{:08x}  MMIO window",
            span.name,
            span.base,
            span.end()
        );
    }
    println!(
        "  (unmapped)   0x{:08x}              MEM_INTERNAL2, inside the MMIO window",
        memmap::MEM_INTERNAL2_BASE
    );
}

fn print_hooks(machine: &Esp32C6Machine) {
    if machine.hooks().is_empty() {
        println!(
            "ROM hook table: empty.\n\
             \n\
             That is the design, not an omission: the ROM runs for real, and a hook is added\n\
             only when a real ROM path cannot be made to work by seeding a register a\n\
             peripheral model owns. See lp-emu/esp/lp-emu-esp32c6/src/rom.rs."
        );
        return;
    }
    println!("ROM hook table ({} entries):", machine.hooks().len());
    for hook in machine.hooks().hooks() {
        println!(
            "  {:#010x}  {:<28} displaced {:#010x}",
            hook.address, hook.symbol, hook.original
        );
    }
}

fn print_load_report(machine: &Esp32C6Machine) {
    eprintln!(
        "rom: {} segments placed, {} symbols",
        machine.rom_segments().len(),
        machine.rom().symbols().len()
    );
    for seg in machine.rom_segments() {
        eprintln!(
            "  0x{:08x} filesz 0x{:<6x} memsz 0x{:<6x} {} -> {}",
            seg.vaddr,
            seg.filesz,
            seg.memsz,
            if seg.execute { "x" } else { "-" },
            seg.regions.join(", ")
        );
    }
    eprintln!(
        "rom data: {} sections seeded, {} bytes (the ROM startup's .data copy, from the ELF)",
        machine.rom_data().len(),
        machine.rom_data().iter().map(|s| s.len).sum::<u32>()
    );
    if let Some(app) = machine.app() {
        eprintln!(
            "app: {} segments placed, entry 0x{:08x} ({})",
            machine.app_segments().len(),
            app.entry,
            machine
                .symbolize(app.entry)
                .unwrap_or_else(|| "?".to_string())
        );
        for seg in machine.app_segments() {
            eprintln!(
                "  0x{:08x} filesz 0x{:<6x} memsz 0x{:<6x} {} -> {}",
                seg.vaddr,
                seg.filesz,
                seg.memsz,
                if seg.execute { "x" } else { "-" },
                seg.regions.join(", ")
            );
        }
    }
}

/// The RMT's own reading of the refill race, per channel, at exit.
///
/// Two histograms in the shape the `[WS281X]` telemetry line prints its own —
/// nine buckets, eighths of a half-window, the last one "≥ half" — so the two
/// can be read side by side. **Reported, never gated** (D13/PD9): the entry
/// half is a floor, because the emulated ISR path is RAM-resident and the
/// machine has no flash-miss cost, and silicon's own entry delay is mostly
/// those misses.
///
/// Silent for a run whose guest never started a channel, which is every run
/// that does not load a project or drive a strip.
fn report_refill_lag(machine: &Esp32C6Machine) {
    for ch in 0..lp_emu_esp32c6::periph::rmt::TX_CHANNELS {
        let s = machine.rmt_refill_stats(ch);
        if s.refills == 0 && s.unanswered == 0 {
            continue;
        }
        eprintln!(
            "rmt refill ch{ch}: {} measured, half={} words; entry max {} hist {}; \
             fill max {} hist {}{}",
            s.refills,
            s.half_words,
            s.entry_max,
            RefillStats::hist_string(&s.entry_hist),
            s.fill_max,
            RefillStats::hist_string(&s.fill_hist),
            match s.unanswered {
                0 => String::new(),
                n => format!(
                    "; {n} threshold(s) the guest never answered before the frame ended \
                     (the last one of a frame is `finish`'s, not `refill`'s)"
                ),
            }
        );
    }
}

fn report(machine: &mut Esp32C6Machine, outcome: &Outcome) {
    let cycles = machine.cycles();
    eprintln!(
        "\nstopped after {} cycles ({} us emulated, {} instructions, grade {})",
        cycles,
        machine.micros(),
        machine.instructions(),
        machine.time_grade().configuration()
    );
    eprintln!(
        "unmapped: {} reads, {} writes, {} distinct sites; {} idle skips (wfi)",
        machine.bus.unmapped_reads(),
        machine.bus.unmapped_writes(),
        machine.bus.unmapped_sites(),
        machine.idle_skips()
    );
    if let Some(level) = machine.bus.strict_grade() {
        // Which blocks the level actually covered. A run that passed says so
        // about the blocks it checked and about no others.
        let scope = machine.bus.blocks_in_strict_grade_scope();
        eprintln!(
            "strict-grade {level}: checked {} ({}); every other block publishes no grade table \
             and was passed over",
            scope.len(),
            if scope.is_empty() {
                "none".to_string()
            } else {
                scope.join(", ")
            }
        );
    }
    eprintln!(
        "uart0: {} bytes left the wire{}",
        machine.uart0().len(),
        match machine.uart0_tcp() {
            Some(tcp) => format!(
                " ({} listened, {} client(s) attached)",
                tcp.local_addr(),
                tcp.clients_seen()
            ),
            None => String::new(),
        }
    );
    let census = machine.flash_census();
    if census.commands() > 0 || census.status_reads > 0 || machine.cache_fills() > 0 {
        eprintln!(
            "flash: {census}; {} page(s) filled into the cache window",
            machine.cache_fills()
        );
    }
    match machine.flush_flash() {
        Ok(true) => eprintln!("flash: image written back"),
        Ok(false) => {}
        Err(e) => eprintln!("flash: could not write the image back: {e}"),
    }
    eprintln!(
        "usb-sj: host {} at power-on; {} bytes reached the host{}",
        machine.usb_host(),
        machine.usb_sj().len(),
        match machine.usb_sj_tcp() {
            Some(tcp) => format!(
                " ({} listened, {} client(s) attached)",
                tcp.local_addr(),
                tcp.clients_seen()
            ),
            None => String::new(),
        }
    );
    if let Some(tcp) = machine.control_tcp() {
        eprintln!(
            "control: {} listened, {} client(s) attached, {} command(s) applied",
            tcp.local_addr(),
            tcp.clients_seen(),
            machine.control_lines()
        );
    } else if machine.control_lines() > 0 || machine.scripted_commands_left() > 0 {
        eprintln!(
            "control: {} scripted command(s) applied, {} never came due",
            machine.control_lines(),
            machine.scripted_commands_left()
        );
    }
    for line in machine.pin_summaries() {
        eprintln!("{line}");
    }
    report_refill_lag(machine);
    let tried = machine.usb_sj_tried();
    if !tried.is_empty() {
        eprintln!(
            "usb-sj tried: the guest handed {} bytes to the IN endpoint that no host took:\n{}",
            tried.len(),
            tried
                .text()
                .lines()
                .map(|l| format!("  | {l}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    match outcome {
        Outcome::ExitMatched { .. } => eprintln!("--exit-on matched"),
        Outcome::Deadline { .. } => eprintln!("emulated timeout reached, no fault"),
        Outcome::Reset {
            cycle,
            source,
            strap,
        } => eprintln!(
            "RESET requested by {source} at cycle {cycle} ({} us), strap = {strap} — the chip \
             would reboot; the emulator reports it (M7 owns the boot chain)",
            cycle / memmap::CYCLES_PER_US
        ),
        Outcome::Fault { pc, fault, .. } => eprintln!(
            "FAULT {fault:?} at pc={pc:#010x} {}",
            machine
                .symbolize(*pc)
                .map(|s| format!("({s})"))
                .unwrap_or_default()
        ),
        Outcome::StrictBus { violation } => {
            let symbol = machine
                .symbolize(violation.pc)
                .map(|s| format!(" ({s})"))
                .unwrap_or_default();
            let grade = violation
                .grade
                .map(|g| format!(" of a register graded {g}, below --strict-grade"))
                .unwrap_or_default();
            eprintln!(
                "STRICT-BUS {:?}{}{grade} of {} bytes at {:#010x} from pc={:#010x}{symbol} at \
                 cycle {}",
                violation.access,
                if violation.grade.is_some() {
                    ""
                } else if violation.in_mmio_window {
                    " inside a declared MMIO window — an unmodelled block"
                } else {
                    ""
                },
                violation.width.bytes(),
                violation.address,
                violation.pc,
                violation.cycle,
            );
        }
        Outcome::WallTimeout { .. } => {
            eprintln!("WALL TIMEOUT — the host-clock safety net, not a guest event")
        }
        Outcome::Breakpoint { pc, .. } => {
            let regs = machine.registers();
            eprintln!(
                "BREAK at {pc:#010x} {}",
                machine.symbolize(*pc).unwrap_or_default()
            );
            eprintln!(
                "  a0={:#010x} a1={:#010x} a2={:#010x} a3={:#010x}\n  a4={:#010x} a5={:#010x} \
                 a6={:#010x} a7={:#010x}\n  sp={:#010x} s0={:#010x} ra={:#010x}",
                regs[10],
                regs[11],
                regs[12],
                regs[13],
                regs[14],
                regs[15],
                regs[16],
                regs[17],
                regs[2],
                regs[8],
                regs[1]
            );
            let (mcause, mepc, mtval) = machine.trap_csrs();
            eprintln!(
                "  mcause={mcause:#010x} mepc={mepc:#010x} {} mtval={mtval:#010x}",
                machine.symbolize(mepc).unwrap_or_default()
            );
            for (name, i) in [("a0", 10), ("a1", 11), ("a2", 12), ("a3", 13)] {
                if let Some(text) = machine.peek_string(regs[i], 120) {
                    eprintln!("  {name} as text: {text:?}");
                }
            }
        }
    }
    if matches!(
        outcome,
        Outcome::Fault { .. } | Outcome::StrictBus { .. } | Outcome::Breakpoint { .. }
    ) {
        eprintln!("backtrace (s0 frame-pointer chain, innermost first):");
        for (i, (address, symbol)) in machine.backtrace().into_iter().enumerate() {
            eprintln!("  #{i:<2} {address:#010x} {symbol}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::ByteSource;

    #[test]
    fn durations_are_emulated_and_carry_their_unit() {
        assert_eq!(parse_duration_us("5s").unwrap(), 5_000_000);
        assert_eq!(parse_duration_us("1500ms").unwrap(), 1_500_000);
        assert_eq!(parse_duration_us("900us").unwrap(), 900);
        // A bare number is refused: "100" could be anything, and guessing
        // would silently run a thousand times too long or too short.
        assert!(parse_duration_us("100").is_err());
    }

    #[test]
    fn a_trace_filter_never_swallows_the_next_flag() {
        let a = parse(vec![
            "--trace".into(),
            "--strict-bus".into(),
            "--elf".into(),
            "x".into(),
        ])
        .unwrap();
        assert!(a.trace && a.strict && a.trace_blocks.is_empty());
        assert_eq!(a.elf, Some("x".into()));

        let b = parse(vec!["--trace".into(), "TIMG0,UART0".into()]).unwrap();
        assert_eq!(b.trace_blocks, ["TIMG0", "UART0"]);
    }

    #[test]
    fn probes_and_uart_destinations_parse_the_way_the_usage_spells_them() {
        assert_eq!(
            parse_probe("TIMED_OUT@120").unwrap(),
            (120, "TIMED_OUT".to_string())
        );
        assert_eq!(parse_probe("x@5ms").unwrap(), (5, "x".to_string()));
        assert!(parse_probe("TIMED_OUT").is_err());

        assert!(matches!(parse_uart0("stdout").unwrap(), Uart0Sink::Stdout));
        assert!(matches!(
            parse_uart0("file:/tmp/u.log").unwrap(),
            Uart0Sink::File(_)
        ));
        assert!(matches!(
            parse_uart0("tcp:127.0.0.1:9000").unwrap(),
            Uart0Sink::Tcp(addr) if addr == "127.0.0.1:9000"
        ));
        assert!(
            parse_uart0("tcp:9000").is_err(),
            "host:port, not a bare port"
        );
        assert!(matches!(parse_usb_sj("stderr").unwrap(), UsbSjSink::Stderr));
        assert!(matches!(
            parse_usb_sj("file:/tmp/usb.log").unwrap(),
            UsbSjSink::File(_)
        ));
    }

    #[test]
    fn the_usb_host_and_strict_grade_flags_take_the_spelled_values_only() {
        let a = parse(vec![
            "--usb-host".into(),
            "attached-idle".into(),
            "--strict-grade".into(),
            "documented".into(),
            "--usb-sj-tried".into(),
            "stderr".into(),
        ])
        .unwrap();
        assert_eq!(a.usb_host, UsbHost::Attached { draining: false });
        assert_eq!(a.strict_grade, Some(RegGrade::Documented));
        assert!(matches!(a.usb_sj_tried, UsbSjSink::Stderr));
        assert_eq!(parse(vec![]).unwrap().usb_host, UsbHost::Absent);
        assert!(parse(vec!["--usb-host".into(), "draining".into()]).is_err());
        assert!(parse(vec!["--strict-grade".into(), "strict".into()]).is_err());
    }

    #[test]
    fn the_two_listeners_and_the_coupling_flag_parse_the_way_the_usage_spells_them() {
        let a = parse(vec![
            "--usb-sj".into(),
            "tcp:127.0.0.1:5556".into(),
            "--control".into(),
            "tcp:127.0.0.1:5557".into(),
            "--usb-sj-drain".into(),
            "manual".into(),
            "--usb-script".into(),
            "s.txt".into(),
            // Repeatable: the cable's schedule and the wire's conversation
            // are two files.
            "--usb-script".into(),
            "walk.script".into(),
        ])
        .unwrap();
        assert!(matches!(a.usb_sj, UsbSjSink::Tcp(addr) if addr == "127.0.0.1:5556"));
        assert_eq!(a.control.as_deref(), Some("127.0.0.1:5557"));
        assert_eq!(a.usb_sj_drain, UsbSjDrain::Manual);
        assert_eq!(
            a.usb_script,
            vec![PathBuf::from("s.txt"), PathBuf::from("walk.script")]
        );

        // The default is the coupling every host program expects.
        assert_eq!(parse(vec![]).unwrap().usb_sj_drain, UsbSjDrain::Auto);
        assert!(parse(vec![]).unwrap().control.is_none());

        // A bare port is refused on both sockets, as it is on --uart0.
        assert!(parse(vec!["--usb-sj".into(), "tcp:5556".into()]).is_err());
        assert!(parse(vec!["--control".into(), "tcp:5557".into()]).is_err());
        // The control channel has no in-memory form: it would be a flag with
        // no effect, and --usb-script is the file-shaped way to say it.
        assert!(parse(vec!["--control".into(), "memory".into()]).is_err());
        assert!(parse(vec!["--usb-sj-drain".into(), "always".into()]).is_err());
    }

    #[test]
    fn a_uart0_script_is_milliseconds_then_a_string_or_hex_bytes() {
        let src = parse_uart0_script(
            "# a comment\n\
             1500 \"M!{\\\"id\\\":1}\\n\"\n\
             \n\
             2000ms 4d 21 0a\n\
             2500 \"\\x41\\t\"\n",
        )
        .unwrap();
        // `M!{"id":1}` is ten bytes plus the newline; then three hex bytes;
        // then `A` and a tab.
        assert_eq!(src.remaining(), 11 + 3 + 2);
        assert_eq!(
            src.next_ready(),
            Some(1_500 * 1_000 * memmap::CYCLES_PER_US)
        );
        assert!(parse_uart0_script("abc \"x\"").is_err());
        assert!(parse_uart0_script("10 \"unterminated").is_err());
        assert!(parse_uart0_script("10 zz").is_err());
        assert!(parse_uart0_script("10 \"\\q\"").is_err());
        assert_eq!(
            lp_emu_esp32c6::control::unescape("a\\nb\\x00c\\\\").unwrap(),
            b"a\nb\0c\\"
        );
        // A control word is the other flag's business, and the error says so
        // rather than dropping the line.
        let err = parse_uart0_script("10 attach").unwrap_err();
        assert!(err.contains("--usb-script"), "{err}");
    }

    #[test]
    fn an_unknown_flag_is_an_error_and_not_a_positional() {
        assert!(parse(vec!["--nope".into()]).is_err());
        assert!(parse(vec!["--elf".into()]).is_err(), "--elf needs a value");
    }
}
