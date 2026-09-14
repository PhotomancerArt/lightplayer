//! `lp-emu-esp32s3` — run an ESP32-S3 image on the machine.
//!
//! Every timeout is **emulated** time, so a run is the same run on a laptop
//! and on a loaded CI box (PD9). `--wall-timeout` is the single wall-clock
//! input and it is a safety net: it can end a run, never change one.
//!
//! The argument parser is written out by hand rather than pulled from a
//! crate, for both other machines' reasons: the flag list is fixed and small,
//! the exit codes are a contract, and the error text is part of what a
//! bring-up session reads.
//!
//! ⚠️ **An unrecognised flag is an error.** A door a phase has not opened is
//! deliberately *not* stubbed with a no-op, so the phase that adds one is
//! visible in the diff instead of silently changing what an old command line
//! meant. P04 added the peripheral flags (`--efuse-mac`, `--efuse-rev`);
//! P05 the link's; P06 the flash, cache and ROM-console ones (`--boot-mode`,
//! `--flash`, `--flash-copy`, `--merged`, `--flash-len`, `--cache-off-fetch`,
//! `--uart0`, `--strap`); P07 the pads'.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use lp_emu_esp32s3::cache::CacheOffPolicy;
use lp_emu_esp32s3::control::parse_usb_script;
use lp_emu_esp32s3::flash::FlashBacking;
use lp_emu_esp32s3::loader::EfuseIdentity;
use lp_emu_esp32s3::machine::{
    AppSource, BootMode, CORE_QUANTUM_DEFAULT, CORES, CPENABLE_RESET_DEFAULT, Esp32S3Builder,
    FrameSink, Machine, Outcome, PERIPHERAL_REGISTRATION_ORDER, PinLogSink, RomSource,
    StopCondition, StripConfig, TimeGrade, UsbHost, UsbSjDrain, UsbSjSink,
};
use lp_emu_esp32s3::{bus_setup, memmap};
use lp_ws281x::{ChannelTiming, ColorOrder};

const USAGE: &str = "\
lp-emu-esp32s3 — the ESP32-S3 (LX7) machine

USAGE:
    lp-emu-esp32s3 --elf <app.elf> [options]
    lp-emu-esp32s3 --boot-mode rom-up --merged <chip.bin> [options]
    lp-emu-esp32s3 --map

OPTIONS:
    --elf <path>            the application image. Under --boot-mode direct
                            (the default) its PT_LOADs are placed by vaddr,
                            its flash-resident half is staged in the chip
                            with the MMU programmed for it, and the hart
                            starts at its entry with the boot state the IDF
                            bootloader would have left. Under rom-up it is a
                            symbol table only
    --boot-mode direct|rom-up
                            direct: the above. rom-up: start at the mask
                            ROM's reset vector with the architectural reset
                            state and seed NOTHING — the flash chip holds a
                            whole merged image (--merged) and the real ROM
                            and the real ESP-IDF second-stage bootloader do
                            the rest [direct]
    --rom <path>            a mask ROM ELF (default: the vendored ESP32-S3
                            rev0 image, compiled in). Loaded in EVERY
                            configuration: on this chip the ROM is most of
                            the dynamic instruction count, not a formality
    --flash <path>          back the flash chip with this file: it is read at
                            start and written back at the end of the run. A
                            path that does not exist is created blank
    --flash-copy <path>     read the file once and never write it back — a
                            scratch copy of a known image
    --merged <path>         --flash-copy, spelled for what a ROM-up boot
                            wants: the whole 8 MiB `espflash save-image
                            --chip esp32s3 --merge` writes. Sets --flash-len
                            to the file's length unless told otherwise
    --flash-len <bytes>     the chip's size [8388608, the S3 board's 8 MB]
    --cache-off-fetch stop|permit
                            D4. `stop` (the default, and every gate run's)
                            refuses a fetch or read through a flash window
                            while that window's cache is disabled, exit 6;
                            `permit` does not check at all
    --uart0 stderr|memory|file:<path>|tcp:<host:port>
                            where the mask ROM's UART0 console goes: the
                            reset banner and the bootloader's log on a
                            rom-up boot. tcp: LISTENS, and the client's bytes
                            are the wire's source [memory]
    --rmt-logs              keep the RMT's per-channel pulse and word logs.
                            OFF by default: a 256-LED frame is 6,146 words
                            and 12,292 pulses, and a run that only wants a
                            boot has no use for them. The refill-lag summary
                            below the run report is collected either way — it
                            is REPORTED, never gated (PD9)
    --dump-frames <spec>    where decoded WS281x frames go: `-` or `stdout`,
                            or `file:<path>`. One `ws281x-frame` JSON line
                            per frame as it is decoded, carrying BOTH the
                            wire bytes and the `rgb` unpermutation, so a
                            wrong order assumption is a visible difference
                            between two fields rather than a silent one
                            inside `rgb`. Frames are kept in memory either
                            way [memory]
    --pin-log <path>        the raw edge stream, one line per edge:
                            `<us> <pad> <level> cyc=<cycle>`, with a
                            `# route` note whenever the matrix moves a pad.
                            Off by default — a 256-LED frame is 12,292
                            edges. Capped at 2,000,000 lines with a note
    --strip-timing ws2812|ws2811
                            the wire timing every routed pad is decoded as
                            [ws2812]
    --strip-order rgb|rbg|grb|gbr|brg|bgr
                            the byte order on the wire, which is what the
                            record's `rgb` field is unpermuted with [grb]
    --strap <word>          what GPIO.strap reads — the strapping pins as the
                            pads were latched at reset. The default is the
                            smallest word the ROM's own decode calls
                            SPI_FAST_FLASH_BOOT (bit 3); no S3 board's word
                            has been captured yet (P09) [0x8]
    --strict-bus            every access to an address nothing claims is a
                            STOP instead of a silent zero. Every block the
                            direct load and the ROM-up boot touch is
                            modelled through P06; a strict run of the shipped
                            image reaches the server loop with unmapped=0
    --efuse-mac <aa:bb:cc:dd:ee:ff>
                            the MAC the eFuse block answers. ⚠️ The default
                            is the PAC's zero, NOT a plausible board: no S3
                            fuse dump has been read yet (P09) [00:00:00:00:00:00]
    --efuse-rev <major.minor>
                            the wafer version the eFuse block answers, in the
                            S3's own split (minor across two words) [0.0]
    --core-quantum <cycles> the upper bound on one core's window. Every core
                            that is not held gets a window of at most this
                            many cycles per loop iteration, core 0 then core
                            1, on one guest clock; due events fire between
                            windows. A run PARAMETER, recorded in the run
                            report [256]
    --cpenable-reset <n>    what CPENABLE holds when a core comes out of
                            reset. The default is the ISA's generic reset,
                            NOT the classic's measured 0xff: the S3
                            firmware's own fpu.rs records its 0xff reading as
                            a fact about that boot chain rather than about
                            the architecture. P09's silicon capture is what
                            would change the default [0]
    --time-grade t1         t1 = cycles are instructions. The only grade this
                            machine defines; there is no measured LX7
                            per-instruction-class table in this repo [t1]
    --timeout <5s|1500ms|900us>
                            EMULATED time to run for [100ms]
    --wall-timeout <s>      host-clock safety net; exits 4
    --break-at <symbol>     stop at the symbol's first instruction, with
                            every register as the caller left it
    --probe <cycle>:<name>  print the word at symbol <name> when guest time
                            reaches <cycle>
    --probe <name>@<ms>     the same thing said in EMULATED milliseconds,
                            which is how the payload registry stores a probe.
                            Both forms work; `@` wins when a value carries one
    --trace <path|->        write the bus trace here
    --trace-block <name>    only trace this block (repeatable)
    --console <path>        write everything the console SAID to this file
                            when the run ends. On this chip the console IS
                            the link, so this is the usb-sj stream: what a
                            HOST RECEIVED, never what the guest tried
    --exit-on <substr>      stop at the end of the line this appears on, on
                            the console. Exit code 0
    --usb-host absent|attached|attached-idle
                            the host's side at power-on: no cable, a cable
                            with an application draining the port, or a cable
                            with the port closed [absent]
    --usb-sj stderr|file:<path>|tcp:<host:port>
                            where the usb-sj stream goes: the IN-endpoint
                            packets a draining host took [kept in memory,
                            and --console writes it]. tcp: LISTENS, and the
                            client's bytes are the OUT endpoint's source
    --usb-sj-tried stderr|file:<path>
                            where the OBSERVATION stream goes: bytes the
                            guest handed over that no host took. Never tcp:
                            it is an observation, not a link
    --usb-sj-drain auto|manual
                            whether a client on the byte socket counts as an
                            application opening the port, and disconnecting
                            as closing it. `manual` leaves open/close to the
                            control channel [auto]. A cable is never implied
                            — attach/detach are control commands
    --control tcp:<host:port>
                            LISTEN for the host control channel: one line
                            per command, one reply line per command, applied
                            at the next slice boundary. attach, detach, open,
                            close, dtr, rts, signals, reset, download-mode,
                            state, usb-write
    --usb-script <file>     scripted host input on the USB link: bytes at
                            declared EMULATED times, plus the control words
                            above, one entry per line. Repeatable. Guest
                            time, so two runs deliver the same bytes at the
                            same cycles
    --reboot-on-reset       PERFORM a reset request instead of reporting it:
                            the machine goes back to its power-on state and
                            runs again. Under rom-up the strap is re-latched
                            into GPIO.strap and the ROM reads it; under
                            direct the load is replayed and the strap is
                            recorded only
    --seed <n>              the machine's PRNG seed [0]
    --hooks                 list the ROM hook table and exit. It is EMPTY,
                            and stays empty: try the real ROM path first
    --map                   print the memory map and exit
    -h, --help              this

EXIT CODES (a cross-machine contract — one table for all three machines):
    0 --exit-on matched, or the        2 the hart faulted, or the RWDT
      deadline was reached                expired without --reboot-on-reset
    3 a strict-bus refusal              4 the wall-clock net fired
    5 a --break-at was reached          6 a cache-off fetch (D4)
    7 a flash-MMU divergence between two cores' tables — NOT APPLICABLE on
      this machine: one core runs and ONE table serves both buses, so there
      is no second table to diverge from. Reserved across the family, never
      emitted here, and there is no --app-mmu-divergence flag to reserve
";

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .init();
    match run() {
        Ok(code) => code,
        Err(message) => {
            eprintln!("lp-emu-esp32s3: {message}");
            ExitCode::from(64)
        }
    }
}

#[derive(Debug, Default)]
struct Args {
    elf: Option<PathBuf>,
    rom: Option<PathBuf>,
    boot_mode: BootMode,
    flash: Option<FlashBacking>,
    flash_len: Option<u32>,
    cache_off: CacheOffPolicy,
    uart0: UsbSjSink,
    strap: Option<u32>,
    strict: bool,
    core_quantum: Option<u64>,
    cpenable_reset: Option<u32>,
    time_grade: TimeGrade,
    timeout: Option<Duration>,
    wall_timeout: Option<Duration>,
    break_at: Vec<String>,
    probes: Vec<(u64, String)>,
    trace: Option<String>,
    trace_blocks: Vec<String>,
    console: Option<PathBuf>,
    exit_on: Option<String>,
    usb_sj: UsbSjSink,
    usb_sj_tried: UsbSjSink,
    usb_host: UsbHost,
    usb_sj_drain: UsbSjDrain,
    usb_script: Vec<PathBuf>,
    control: Option<String>,
    reboot_on_reset: bool,
    seed: u64,
    efuse: EfuseIdentity,
    hooks: bool,
    map: bool,
    help: bool,
    // ---- the pads (M6 P07) ----
    rmt_logs: bool,
    dump_frames: FrameSink,
    pin_log: PinLogSink,
    strip_timing: Option<ChannelTiming>,
    strip_order: Option<ColorOrder>,
}

fn run() -> Result<ExitCode, String> {
    let args = parse(std::env::args().skip(1).collect())?;
    if args.help {
        print!("{USAGE}");
        return Ok(ExitCode::SUCCESS);
    }
    if args.map {
        print_map();
        return Ok(ExitCode::SUCCESS);
    }
    if args.elf.is_none() && !args.hooks && args.boot_mode == BootMode::Direct {
        return Err(
            "--elf is required under --boot-mode direct: this machine direct-loads an \
             application. `--boot-mode rom-up --merged <chip.bin>` boots from the reset \
             vector instead; `--map` and `--help` need no image."
                .into(),
        );
    }
    if args.boot_mode == BootMode::RomUp && args.flash.is_none() {
        return Err(
            "--boot-mode rom-up needs a chip to boot from: --merged <chip.bin> (or --flash / \
             --flash-copy). A blank chip has no bootloader and the ROM would only say so."
                .into(),
        );
    }

    let mut builder = Esp32S3Builder::new()
        .time_grade(args.time_grade)
        .strict(args.strict)
        .boot_mode(args.boot_mode)
        .cache_off_fetch(args.cache_off)
        .uart0(args.uart0.clone())
        .core_quantum(args.core_quantum.unwrap_or(CORE_QUANTUM_DEFAULT))
        .cpenable_reset(args.cpenable_reset.unwrap_or(CPENABLE_RESET_DEFAULT))
        .efuse(args.efuse)
        .usb_sj(args.usb_sj.clone())
        .usb_sj_tried(args.usb_sj_tried.clone())
        .usb_host(args.usb_host)
        .usb_sj_drain(args.usb_sj_drain)
        .reboot_on_reset(args.reboot_on_reset)
        .rmt_logs(args.rmt_logs)
        .dump_frames(args.dump_frames.clone())
        .pin_log(args.pin_log.clone())
        .strip(StripConfig {
            timing: args.strip_timing.unwrap_or(StripConfig::default().timing),
            order: args.strip_order.unwrap_or(StripConfig::default().order),
        })
        .seed(args.seed);
    if let Some(word) = args.strap {
        builder = builder.strap(word);
    }
    if let Some(backing) = args.flash.clone() {
        // A merged image says how big the part it was built for is; taking
        // the file's length as the chip's is what keeps a `--merged` run
        // from silently truncating one.
        if args.flash_len.is_none()
            && let FlashBacking::File(p) | FlashBacking::Copy(p) = &backing
            && let Ok(meta) = std::fs::metadata(p)
            && meta.len() > 0
        {
            builder = builder.flash_len(meta.len() as u32);
        }
        builder = builder.flash(backing);
    }
    if let Some(len) = args.flash_len {
        builder = builder.flash_len(len);
    }
    if let Some(addr) = args.control.clone() {
        builder = builder.control(addr);
    }
    if !args.usb_script.is_empty() {
        let mut commands = Vec::new();
        let mut bytes = lp_emu_esp_common::ScriptedSource::new();
        for path in &args.usb_script {
            let text = std::fs::read_to_string(path)
                .map_err(|e| format!("--usb-script {}: {e}", path.display()))?;
            let script = parse_usb_script(&text).map_err(|e| format!("{}: {e}", path.display()))?;
            commands.extend(script.commands);
            bytes.extend(script.bytes);
        }
        commands.sort_by_key(|(at, _)| *at);
        println!(
            "usb script: {} byte(s) of host input in {} chunk(s) and {} control command(s)",
            bytes.remaining(),
            bytes.chunks(),
            commands.len()
        );
        builder = builder.usb_script(commands);
        builder = builder.usb_script_source(bytes);
    }
    if let Some(path) = args.rom {
        builder = builder.rom(RomSource::Path(path));
    }
    if let Some(path) = args.elf.clone() {
        builder = builder.app(AppSource::Path(path));
    }
    if let Some(spec) = &args.trace {
        let sink: Box<dyn std::io::Write + Send> = if spec == "-" {
            Box::new(std::io::stderr())
        } else {
            Box::new(std::fs::File::create(spec).map_err(|e| format!("--trace {spec}: {e}"))?)
        };
        builder = builder.trace(sink, args.trace_blocks.clone());
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
        println!("break-at {symbol} @ {at:#010x}");
    }

    print_build_report(&machine);

    let timeout = args.timeout.unwrap_or(Duration::from_millis(100));
    let stop = StopCondition {
        stop_cycle: Some(emulated_cycles(timeout)),
        wall_timeout: args.wall_timeout,
        probes: args.probes.clone(),
        exit_on: args.exit_on.clone(),
    };

    let outcome = machine.run_until(&stop);
    machine.bus_mut().host.flush_all();
    // ⚠️ Before the report, and before anything reads the last frame: a frame
    // is not closed until something follows its latch, so a run that stopped
    // mid-frame has one still open. `flush_frames` reports it **incomplete**
    // rather than inventing a reset gap.
    machine.flush_frames();
    print_outcome(&mut machine, &outcome);
    print_run_summary(&machine);
    print_pin_report(&machine);
    print_refill_lag(&machine);
    if let Some(path) = &args.console
        && let Err(e) = std::fs::write(path, machine.console().bytes())
    {
        eprintln!("lp-emu-esp32s3: --console {}: {e}", path.display());
    }
    match machine.flush_flash() {
        Ok(true) => println!("flash: written back"),
        Ok(false) => {}
        Err(e) => eprintln!("lp-emu-esp32s3: writing the flash image back: {e}"),
    }
    Ok(ExitCode::from(outcome.exit_code() as u8))
}

/// One line per routed pad at exit: frames, complete frames, bit errors,
/// LEDs and edges.
///
/// Silent for a run that routed nothing, which is every run that does not
/// load a project. ⚠️ The numbers here are one model read three ways — the
/// RMT's words, the fabric's edges and the decoder's frames — and not a
/// measurement. No S3 silicon has been read.
fn print_pin_report(machine: &Machine) {
    for line in machine.pin_report() {
        println!("{line}");
    }
}

/// The RMT's own reading of the refill race, per channel, at exit.
///
/// Two histograms in the shape the guest's `[WS281X]` telemetry line prints
/// its own — nine buckets, eighths of a half-window, the last one "≥ half" —
/// so the two can be read side by side. **Reported, never gated** (PD9): the
/// entry half is a floor, because the emulated ISR path is RAM-resident and
/// the machine has no flash-miss cost, and silicon's own entry delay is
/// mostly those misses.
///
/// Silent for a run whose guest never started a channel.
fn print_refill_lag(machine: &Machine) {
    use lp_emu_esp32s3::periph::rmt::{RefillStats, TX_CHANNELS};
    for ch in 0..TX_CHANNELS {
        let s = machine.rmt_refill_stats(ch);
        if s.refills == 0 && s.unanswered == 0 {
            continue;
        }
        println!(
            "rmt refill ch{ch}: {} measured, half={} words; entry max {} hist {}; \
             fill max {} hist {}{}",
            s.refills,
            s.half_words,
            s.entry_max,
            RefillStats::hist_string(&s.entry_hist),
            s.fill_max,
            RefillStats::hist_string(&s.fill_hist),
            if s.unanswered == 0 {
                String::new()
            } else {
                format!(
                    "; {} unanswered (the last one of a frame is `finish`'s, not `refill`'s)",
                    s.unanswered
                )
            },
        );
    }
}

/// The one line a gate reads: what the run cost and what it refused.
///
/// `unmapped=0` is a binary condition a transcript can carry; before a line
/// like this it was only ever readable as the absence of a `WARN` in a log.
/// `instructions` and `cycles` are the determinism pin's other two numbers
/// (`tests/determinism.rs`): a run is a pure function of its instruction
/// stream, so two runs of the same image agree on all three or the run is not
/// deterministic.
///
/// `idle` counts the deterministic idle skips. `fence` is the shared bus's
/// `missing_fence_reports`: guest code executed from bytes the guest itself
/// wrote without publishing them — **reported, never gated**, and on this
/// chip it is the counter the JIT path will move.
fn print_run_summary(machine: &Machine) {
    let bus = machine.bus();
    let per_core: Vec<String> = (0..CORES)
        .map(|c| format!("core{c}={}", machine.core_instructions(c)))
        .collect();
    // The link, in one line a gate can read: where the host was at
    // power-on, how many bytes reached it, and — the state a gate must be
    // able to tell apart from "the firmware crashed" — how many the guest
    // handed over that nobody took.
    let tried = machine.usb_sj_tried();
    println!(
        "usb-sj: host {} at power-on; {} bytes reached the host; {} bytes were merely tried{}",
        machine.usb_host(),
        machine.usb_sj().len(),
        tried.len(),
        match machine.usb_sj_tcp() {
            Some(tcp) => format!(
                " (listening on {}, client {})",
                tcp.local_addr(),
                if tcp.client_connected() {
                    "connected"
                } else {
                    "absent"
                }
            ),
            None => String::new(),
        }
    );
    if machine.control_tcp().is_some() || machine.control_lines() > 0 {
        println!(
            "control: {} command(s) applied, {} scripted command(s) never came due",
            machine.control_lines(),
            machine.scripted_commands_left()
        );
    }
    if machine.reboots() > 0 {
        println!(
            "reboots: {} performed (--reboot-on-reset); last strap {}",
            machine.reboots(),
            machine.strap()
        );
    }
    {
        let chip = machine.flash().lock().expect("flash poisoned");
        println!(
            "flash: {} ({} MiB), backing {:?}, jedec {:#010x}; {}; cache fills {}",
            chip.len(),
            chip.len() >> 20,
            chip.backing(),
            chip.jedec_id(),
            chip.command_census(),
            machine.cache_fills(),
        );
    }
    if !machine.uart0().is_empty() {
        println!("uart0: {} bytes on the wire", machine.uart0().len());
    }
    println!(
        "run: cycles={} instructions={} ({}) idle={} unmapped={} (reads {}, writes {}, {} \
         sites) fence={} quantum={}",
        machine.cycles(),
        machine.instructions(),
        per_core.join(" "),
        machine.idle_skips(),
        bus.unmapped_reads() + bus.unmapped_writes(),
        bus.unmapped_reads(),
        bus.unmapped_writes(),
        bus.unmapped_sites(),
        bus.missing_fence_reports(),
        machine.core_quantum(),
    );
}

/// The whole map, and the alias mirror beside it.
///
/// The alias line is this binary's job rather than P02's: `SocBus::ram_aliases`
/// exists on the shared bus, but P02 had no chip crate to print it from.
fn print_map() {
    println!("ESP32-S3 memory map — every base cites lp_emu_esp32s3::memmap");
    for span in memmap::RAM_SPANS {
        println!(
            "  {:<20} {:#010x}..{:#010x}  {:>10} B",
            span.name,
            span.base,
            span.end(),
            span.len
        );
    }
    println!(
        "  the SRAM1 mirror: ONE store, TWO doors — the I-bus view is a RAM alias, not a \
         region (M6 P02, ruling DD81). A JIT'd shader is written through the D-bus door and \
         fetched through this one."
    );
    let bus = bus_setup::build();
    for (base, len, target) in bus.ram_aliases() {
        let name = memmap::RAM_ALIASES
            .iter()
            .find(|(s, _)| s.base == base)
            .map(|(s, _)| s.name)
            .unwrap_or("?");
        println!(
            "  {name:<20} {base:#010x}..{:#010x}  -> {target:#010x} (+{:#x})",
            base + len,
            base - target
        );
    }
    for span in memmap::MMIO_WINDOWS {
        println!(
            "  {:<20} {:#010x}..{:#010x}  declared; the blocks below are modelled and a \
             strict run stops at the first one that is not",
            span.name,
            span.base,
            span.end()
        );
    }
    println!("  modelled (M6 P04), in registration order — the order the direct load met them:");
    println!("    {}", PERIPHERAL_REGISTRATION_ORDER.join(" "));
    println!("  the blocks the image's own MMIO census names, in address order:");
    for (name, base) in peripheral_census() {
        let state = if PERIPHERAL_REGISTRATION_ORDER.contains(&name) {
            "modelled"
        } else {
            "NOT modelled — a strict run stops here"
        };
        println!("    {name:<18} {base:#010x}  {state}");
    }
    println!("  deliberately unmapped:");
    for (span, why) in bus_setup::deliberately_unmapped() {
        println!(
            "  {:<20} {:#010x}..{:#010x}\n    {why}",
            span.name,
            span.base,
            span.end()
        );
    }
    println!(
        "  cores: {CORES} slots, slot 1 HELD ({:#010x} SYSTEM.core_1_control_0, PAC reset \
         0x04 = reseting|!clkgate_en). PRID {:#06x} / {:#06x}, told apart by bit 13.",
        memmap::SYSTEM_CORE_1_CONTROL_0,
        memmap::PRID_CORE0,
        memmap::PRID_CORE1
    );
    println!(
        "  clock: {} Hz, {} cycles/us",
        memmap::CPU_HZ,
        memmap::CYCLES_PER_US
    );
}

/// The blocks `m6/notes.md` §2.4's census found the shipped image touching,
/// in address order. Printing only — P04 registers them in the order the boot
/// *meets* them, which is a different order and is that phase's ledger.
fn peripheral_census() -> Vec<(&'static str, u32)> {
    use memmap::periph as p;
    let mut all = vec![
        ("UART0", p::UART0),
        ("SPI1", p::SPI1),
        ("SPI0", p::SPI0),
        ("GPIO", p::GPIO),
        ("FE2", p::FE2),
        ("FE", p::FE),
        ("EFUSE", p::EFUSE),
        ("RTC_CNTL", p::RTC_CNTL),
        ("IO_MUX", p::IO_MUX),
        ("I2C_ANA_MST", p::I2C_ANA_MST),
        ("RMT", p::RMT),
        ("NRX", p::NRX),
        ("BB", p::BB),
        ("TIMG0", p::TIMG0),
        ("TIMG1", p::TIMG1),
        ("SYSTIMER", p::SYSTIMER),
        ("APB_CTRL", p::APB_CTRL),
        ("USB_DEVICE", p::USB_DEVICE),
        ("SHA", p::SHA),
        ("SYSTEM", p::SYSTEM),
        ("SENSITIVE", p::SENSITIVE),
        ("INTERRUPT_CORE0", p::INTERRUPT_CORE0),
        ("INTERRUPT_CORE1", p::INTERRUPT_CORE1),
        ("EXTMEM", p::EXTMEM),
    ];
    all.sort_by_key(|(_, base)| *base);
    all
}

fn print_hooks(machine: &Machine) {
    let hooks = machine.hooks();
    if hooks.is_empty() {
        println!(
            "the ROM hook table is EMPTY, and that is the default. A hook is a last resort: \
             try the real ROM path first, and add one only when it cannot be made to work by \
             giving a register a peripheral model owns its documented reset value."
        );
        return;
    }
    for hook in hooks.hooks() {
        println!(
            "  {:#010x} {} (displaced {:02x?})",
            hook.address, hook.symbol, hook.original
        );
    }
}

fn print_build_report(machine: &Machine) {
    let image = machine.rom_data_image();
    println!(
        "rom: {} PT_LOADs placed, {} empty; {} non-alloc sections seeded ({} B)",
        machine.rom_segments().len(),
        lp_emu_esp32s3::rom::empty_segments(machine.rom()),
        image.sections,
        image.bytes,
    );
    println!(
        "boot: {} (strap {:#x}, cache-off-fetch {})",
        machine.boot_mode().as_str(),
        machine.strap_word(),
        machine
            .cache()
            .lock()
            .expect("cache poisoned")
            .policy()
            .as_str(),
    );
    println!("app: {} segments placed", machine.app_segments().len());
    for seg in machine.app_segments().iter().filter(|s| s.relocated()) {
        println!(
            "app: segment vaddr={:#010x} paddr={:#010x} placed by vaddr (memsz {:#x})",
            seg.vaddr, seg.paddr, seg.memsz
        );
    }
    if let Some(seed) = machine.flash_seed() {
        println!(
            "flash chip: rom_spiflash_legacy_data -> {:#010x} chip_size {:#x} -> {:#x} ({} MiB)",
            seed.chip,
            seed.previous,
            seed.chip_size,
            seed.chip_size >> 20
        );
    }
    let staging = machine.flash_staging();
    if !staging.pages.is_empty() {
        let first = staging.pages.first().expect("pages");
        let last = staging.pages.last().expect("pages");
        println!(
            "flash staging: {} pages, {} B, factory {:#010x}..{:#010x}; MMU entries {}..={}",
            staging.pages.len(),
            staging.bytes,
            first.paddr,
            last.paddr + lp_emu_esp32s3::cache::PAGE_LEN,
            first.index,
            last.index,
        );
    }
    if let Some(frame) = machine.boot_frame() {
        println!(
            "boot frame: a1={:#010x}, save area [a1-16..a1) = {:#010x} {:#010x} {:#010x} \
             {:#010x}, PS.OWB={}",
            frame.sp,
            frame.save_area[0],
            frame.save_area[1],
            frame.save_area[2],
            frame.save_area[3],
            frame.owb,
        );
    }
    println!(
        "reset state: CPENABLE={:#010x} (a PARAMETER — the classic's 0xff is the classic's \
         measurement; P09 pins this chip's), unsupported-opcode stop={}",
        machine.cpenable_reset(),
        machine.strict_unsupported(),
    );
    println!("time grade: t1 ({})", machine.time_grade().configuration());
    for line in machine.core_report() {
        println!("{line}");
    }
}

fn print_outcome(machine: &mut Machine, outcome: &Outcome) {
    let cycle = outcome.cycle();
    let micros = cycle / memmap::CYCLES_PER_US;
    match outcome {
        Outcome::Deadline { .. } => {
            println!("DEADLINE cycle={cycle} ({micros} us emulated)");
            for core in 0..CORES {
                let pc = machine.harts[core].pc();
                let sym = machine.symbolize(pc).unwrap_or_else(|| "?".into());
                let state = if machine.core_stalled(core) {
                    "held"
                } else if machine.parked(core) {
                    "parked(waiti)"
                } else {
                    "running"
                };
                println!("  core {core}: {state} pc={pc:#010x} ({sym})");
                if machine.core_stalled(core) {
                    continue;
                }
                // ⚠️ The pc alone is a trap on this hart: the machine polls at
                // the END of every window, so a guest that is taking
                // interrupts at all is always sampled just after one was
                // taken and always reports a vector. `PS`, `INTENABLE` and
                // the pending mask are what separate "spinning in a vector"
                // from "idling with an interrupt in flight".
                let ps = machine.harts[core].ps();
                machine.bus_mut().set_hart(core);
                let external = machine.bus().pending_cpu_interrupt_mask();
                let ints = machine.harts[core].interrupts_mut();
                let (intenable, pending) = (ints.intenable, ints.pending());
                println!(
                    "    PS={ps:#010x} (INTLEVEL={}, EXCM={}) INTENABLE={intenable:#010x} \
                     pending={pending:#010x} external={external:#010x}",
                    ps & 0xf,
                    u8::from(ps & lp_xt_emu::mach::sr::PS_EXCM != 0),
                );
            }
        }
        Outcome::ExitMatched { .. } => {
            println!("--exit-on matched at cycle={cycle} ({micros} us emulated)");
        }
        Outcome::Breakpoint { core, pc, .. } => {
            let sym = machine.symbolize(*pc).unwrap_or_else(|| "?".into());
            println!("BREAKPOINT core={core} pc={pc:#010x} ({sym}) cycle={cycle}");
        }
        Outcome::WallTimeout { .. } => {
            println!("WALL TIMEOUT cycle={cycle} ({micros} us emulated)");
        }
        Outcome::Reset { source, strap, .. } => {
            println!(
                "RESET cycle={cycle} ({micros} us emulated): {source} asked for a chip reset \
                 into strap {strap}. Pass --reboot-on-reset to make the machine perform it; \
                 without it the run ends here, and on silicon the chip would reboot."
            );
        }
        Outcome::CacheOffFetch {
            pc, access, cycle, ..
        } => {
            println!("{}", machine.cache_off_message(*cycle, *pc, access));
        }
        Outcome::Fault {
            core, pc, fault, ..
        } => {
            let core = *core;
            let sym = machine.symbolize(*pc).unwrap_or_else(|| "?".into());
            println!("FAULT core={core} pc={pc:#010x} ({sym}) cycle={cycle}: {fault:?}");
            // The exception registers, because a fault *inside a vector* says
            // nothing about what asked for it: the earliest cause is the root.
            let sr = machine.harts[core].sr();
            let epc1 = sr.epc[1];
            let (exccause, excvaddr, ps) = (sr.exccause, sr.excvaddr, machine.harts[core].ps());
            let name = match machine.symbolize(epc1) {
                Some(n) => format!(" ({n})"),
                None => String::new(),
            };
            println!(
                "  EXCCAUSE={exccause} EXCVADDR={excvaddr:#010x} EPC1={epc1:#010x}{name} \
                 PS={ps:#010x}"
            );
        }
        Outcome::StrictBus { violation } => {
            let sym = machine
                .symbolize(violation.pc)
                .unwrap_or_else(|| "?".into());
            let where_ = if violation.in_mmio_window {
                "inside the declared peripheral window — an UNMODELLED BLOCK. M6 P04 models \
                 every block before the console; a stop here is the next thing the boot needs \
                 and the next phase's first entry (the console is P05's, flash and the cache \
                 P06's, the pads P07's)"
                    .to_string()
            } else if let Some((span, why)) = bus_setup::unmapped_window(violation.address) {
                format!(
                    "inside `{}`, which this machine deliberately does not map: {why}",
                    span.name
                )
            } else if violation.address >= memmap::RTC_FAST_BASE
                && violation.address < memmap::RTC_FAST_BASE + memmap::RTC_FAST_LEN
            {
                "inside RTC fast memory, which IS mapped — so this is a width or alignment \
                 refusal, not an unmapped address"
                    .to_string()
            } else {
                "outside every region and every declared window — a MEMORY-MAP question, not \
                 a peripheral one"
                    .to_string()
            };
            // DD81: a fault through the SRAM1 alias reports the canonical
            // address, so nothing downstream could name the door. This is the
            // one line that can.
            let door = bus_setup::ram_alias_of(violation.address)
                .map(|(span, canonical)| {
                    format!(
                        "\n  door    = `{}` ({:#010x} canonical); note that a fault raised \
                         through an alias reports the CANONICAL address (DD81), so this line \
                         appears only for an address that is itself an alias address",
                        span.name, canonical
                    )
                })
                .unwrap_or_default();
            println!(
                "STRICT BUS STOP\n  \
                 pc      = {:#010x} ({sym})\n  \
                 cycle   = {} ({} us emulated)\n  \
                 access  = {:?} {:?} at {:#010x}\n  \
                 where   = {where_}{door}",
                violation.pc,
                violation.cycle,
                violation.cycle / memmap::CYCLES_PER_US,
                violation.access,
                violation.width,
                violation.address,
            );
            println!(
                "  the earliest strict stop is the root: model THIS block, with the pin \
                 cited, and run again."
            );
        }
    }
}

fn parse(argv: Vec<String>) -> Result<Args, String> {
    let mut args = Args::default();
    let mut it = argv.into_iter();
    while let Some(flag) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("`{flag}` needs a value"));
        match flag.as_str() {
            "-h" | "--help" => args.help = true,
            "--map" => args.map = true,
            "--hooks" => args.hooks = true,
            "--strict-bus" => args.strict = true,
            "--elf" => args.elf = Some(PathBuf::from(value()?)),
            "--rom" => args.rom = Some(PathBuf::from(value()?)),
            "--boot-mode" => {
                let v = value()?;
                args.boot_mode = BootMode::parse(&v)
                    .ok_or_else(|| format!("--boot-mode {v}: expected direct or rom-up"))?;
            }
            "--flash" => args.flash = Some(FlashBacking::File(PathBuf::from(value()?))),
            "--flash-copy" | "--merged" => {
                args.flash = Some(FlashBacking::Copy(PathBuf::from(value()?)))
            }
            "--flash-len" => {
                let v = value()?;
                let n = v
                    .strip_prefix("0x")
                    .map(|h| u32::from_str_radix(h, 16))
                    .unwrap_or_else(|| v.parse())
                    .map_err(|_| format!("--flash-len {v}: not a byte count"))?;
                args.flash_len = Some(n);
            }
            "--cache-off-fetch" => args.cache_off = CacheOffPolicy::parse(&value()?)?,
            "--uart0" => args.uart0 = parse_usb_sj(&value()?, "--uart0", true)?,
            "--rmt-logs" => args.rmt_logs = true,
            "--dump-frames" => {
                let spec = value()?;
                args.dump_frames = match spec.as_str() {
                    "-" | "stdout" => FrameSink::Stdout,
                    "memory" => FrameSink::Memory,
                    other => match other.split_once(':') {
                        Some(("file", path)) => FrameSink::File(PathBuf::from(path)),
                        _ => FrameSink::File(PathBuf::from(other)),
                    },
                };
            }
            "--pin-log" => args.pin_log = PinLogSink::File(PathBuf::from(value()?)),
            "--strip-timing" => {
                let text = value()?;
                args.strip_timing = Some(
                    StripConfig::parse_timing(&text)
                        .ok_or_else(|| format!("--strip-timing {text}: ws2812|ws2811"))?,
                );
            }
            "--strip-order" => {
                let text = value()?;
                args.strip_order = Some(
                    StripConfig::parse_order(&text)
                        .ok_or_else(|| format!("--strip-order {text}: rgb|rbg|grb|gbr|brg|bgr"))?,
                );
            }
            "--strap" => {
                let v = value()?;
                let n = v
                    .strip_prefix("0x")
                    .map(|h| u32::from_str_radix(h, 16))
                    .unwrap_or_else(|| v.parse())
                    .map_err(|_| format!("--strap {v}: not a number"))?;
                args.strap = Some(n);
            }
            "--core-quantum" => {
                let v = value()?;
                let n: u64 = v
                    .parse()
                    .map_err(|_| format!("--core-quantum {v}: not a cycle count"))?;
                if n == 0 {
                    return Err("--core-quantum 0: a zero-cycle window runs nothing".into());
                }
                args.core_quantum = Some(n);
            }
            "--cpenable-reset" => {
                let v = value()?;
                let n = v
                    .strip_prefix("0x")
                    .map(|h| u32::from_str_radix(h, 16))
                    .unwrap_or_else(|| v.parse())
                    .map_err(|_| format!("--cpenable-reset {v}: not a number"))?;
                args.cpenable_reset = Some(n);
            }
            "--time-grade" => args.time_grade = TimeGrade::parse(&value()?)?,
            "--timeout" => args.timeout = Some(parse_duration(&value()?)?),
            "--wall-timeout" => args.wall_timeout = Some(parse_duration(&value()?)?),
            "--break-at" => args.break_at.push(value()?),
            "--probe" => args.probes.push(parse_probe(&value()?)?),
            "--trace" => args.trace = Some(value()?),
            "--trace-block" => args.trace_blocks.push(value()?),
            "--console" => args.console = Some(PathBuf::from(value()?)),
            "--exit-on" => args.exit_on = Some(value()?),
            "--usb-sj" => args.usb_sj = parse_usb_sj(&value()?, "--usb-sj", true)?,
            "--usb-sj-tried" => {
                args.usb_sj_tried = parse_usb_sj(&value()?, "--usb-sj-tried", false)?
            }
            "--usb-host" => {
                let text = value()?;
                args.usb_host = UsbHost::parse(&text).ok_or_else(|| {
                    format!("--usb-host `{text}`: expected absent, attached or attached-idle")
                })?;
            }
            "--usb-sj-drain" => {
                let text = value()?;
                args.usb_sj_drain = UsbSjDrain::parse(&text)
                    .ok_or_else(|| format!("--usb-sj-drain `{text}`: expected auto or manual"))?;
            }
            "--usb-script" => args.usb_script.push(value()?.into()),
            "--control" => {
                let text = value()?;
                args.control = Some(parse_control(&text)?);
            }
            "--reboot-on-reset" => args.reboot_on_reset = true,
            "--seed" => {
                let v = value()?;
                args.seed = v.parse().map_err(|_| format!("--seed {v}: not a number"))?;
            }
            "--efuse-mac" => {
                let v = value()?;
                args.efuse.mac =
                    EfuseIdentity::parse_mac(&v).map_err(|e| format!("--efuse-mac: {e}"))?;
            }
            "--efuse-rev" => {
                let v = value()?;
                let (major, minor) =
                    EfuseIdentity::parse_rev(&v).map_err(|e| format!("--efuse-rev: {e}"))?;
                args.efuse.wafer_major = major;
                args.efuse.wafer_minor = minor;
            }
            other => {
                return Err(format!(
                    "unrecognised flag `{other}`. A door a later phase adds is absent rather \
                     than accepted-and-ignored, so the phase that adds one is visible in the \
                     diff. `--help` lists what exists."
                ));
            }
        }
    }
    Ok(args)
}

/// `--usb-sj` / `--usb-sj-tried`: where a link stream goes.
///
/// `-` is stderr, spelled as the other machines spell it, so one command
/// line drives all three. Only the delivered stream may be a socket: the
/// observation stream is an observation, not a link, and refusing it here is
/// how a reader finds that out.
fn parse_usb_sj(text: &str, flag: &str, allow_tcp: bool) -> Result<UsbSjSink, String> {
    match text {
        "stderr" | "-" => Ok(UsbSjSink::Stderr),
        "memory" => Ok(UsbSjSink::Memory),
        other => {
            if let Some(path) = other.strip_prefix("file:") {
                return Ok(UsbSjSink::File(PathBuf::from(path)));
            }
            if let Some(addr) = other.strip_prefix("tcp:") {
                if !allow_tcp {
                    return Err(format!(
                        "{flag} tcp:{addr}: only --usb-sj may be a socket — the observation \
                         stream is an observation, not a link"
                    ));
                }
                if !addr.contains(':') {
                    return Err(format!(
                        "{flag} tcp:{addr}: write tcp:<host:port>, e.g. tcp:127.0.0.1:5556"
                    ));
                }
                return Ok(UsbSjSink::Tcp(addr.to_string()));
            }
            Err(format!(
                "{flag} `{other}`: expected stderr, memory, file:<path> or tcp:<host:port>"
            ))
        }
    }
}

/// `--control tcp:<host:port>`. Only `tcp:` exists: a control channel with
/// nobody on the other end would be a flag with no effect, and a script
/// (`--usb-script`) is the file-shaped way to say the same thing.
fn parse_control(text: &str) -> Result<String, String> {
    match text.strip_prefix("tcp:") {
        Some(addr) if addr.contains(':') => Ok(addr.to_string()),
        Some(addr) => Err(format!(
            "--control tcp:{addr}: write tcp:<host:port>, e.g. tcp:127.0.0.1:5557"
        )),
        None => Err(format!(
            "`{text}` is not a control channel (tcp:<host:port>; --usb-script is the \
             file-shaped way to drive the link deterministically)"
        )),
    }
}

/// `--probe`, in either of its two spellings, as `(cycle, symbol)`.
///
/// `<cycle>:<symbol>` is an absolute guest cycle, which is the unit the run
/// loop schedules in. `<symbol>@<ms>` is emulated milliseconds, which is how
/// the payload registry stores a probe and how the C6 binary spells it; both
/// machines are handed one command line by `lp-emu-validate`, so teaching the
/// binary the second spelling is cheaper and more honest than a per-chip
/// formatter in the runner (M5 ruling R2).
///
/// The two are told apart by which separator appears, and `@` is checked
/// first: a symbol may contain `:` (a Rust path), a cycle count may not
/// contain `@`.
fn parse_probe(v: &str) -> Result<(u64, String), String> {
    if let Some((name, ms)) = v.rsplit_once('@') {
        let ms: u64 = ms
            .parse()
            .map_err(|_| format!("--probe {v}: `{ms}` is not a count of milliseconds"))?;
        if name.is_empty() {
            return Err(format!("--probe {v}: no symbol before the `@`"));
        }
        return Ok((emulated_cycles(Duration::from_millis(ms)), name.to_string()));
    }
    let (cycle, name) = v
        .split_once(':')
        .ok_or_else(|| format!("--probe {v}: expected <cycle>:<symbol> or <symbol>@<ms>"))?;
    let cycle: u64 = cycle
        .parse()
        .map_err(|_| format!("--probe {v}: `{cycle}` is not a cycle count"))?;
    if name.is_empty() {
        return Err(format!("--probe {v}: no symbol after the `:`"));
    }
    Ok((cycle, name.to_string()))
}

/// `5s`, `1500ms`, `900us`. **Emulated** time everywhere except
/// `--wall-timeout`, which reads the same syntax against the host clock.
fn parse_duration(text: &str) -> Result<Duration, String> {
    let (number, unit) = text
        .find(|c: char| c.is_ascii_alphabetic())
        .map(|i| text.split_at(i))
        .unwrap_or((text, "s"));
    let n: u64 = number
        .parse()
        .map_err(|_| format!("`{text}`: `{number}` is not a number"))?;
    match unit {
        "s" => Ok(Duration::from_secs(n)),
        "ms" => Ok(Duration::from_millis(n)),
        "us" => Ok(Duration::from_micros(n)),
        other => Err(format!("`{text}`: unknown unit `{other}` (s, ms, us)")),
    }
}

/// Emulated cycles for a duration, at [`memmap::CPU_HZ`].
fn emulated_cycles(d: Duration) -> u64 {
    d.as_micros() as u64 * memmap::CYCLES_PER_US
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_probe_spellings_parse() {
        assert_eq!(
            parse_probe("120:TIMED_OUT").unwrap(),
            (120, "TIMED_OUT".to_string())
        );
        assert_eq!(
            parse_probe("TIMED_OUT@120").unwrap(),
            (
                emulated_cycles(Duration::from_millis(120)),
                "TIMED_OUT".to_string()
            )
        );
        // And that conversion is the unit the run loop schedules in.
        assert_eq!(
            parse_probe("TIMED_OUT@120").unwrap().0,
            120 * 1000 * memmap::CYCLES_PER_US
        );
    }

    /// A symbol may carry a `:` (a Rust path); a cycle count may not carry an
    /// `@`. So `@` decides, and it is checked first.
    #[test]
    fn an_at_sign_decides_which_spelling_it_is() {
        let (cycle, name) = parse_probe("lp_fw::state::TIMED_OUT@5").unwrap();
        assert_eq!(name, "lp_fw::state::TIMED_OUT");
        assert_eq!(cycle, emulated_cycles(Duration::from_millis(5)));
    }

    #[test]
    fn a_probe_that_is_neither_spelling_names_both() {
        let err = parse_probe("TIMED_OUT").unwrap_err();
        assert!(err.contains("<cycle>:<symbol>"), "{err}");
        assert!(err.contains("<symbol>@<ms>"), "{err}");
        assert!(parse_probe("TIMED_OUT@soon").unwrap_err().contains("soon"));
        assert!(parse_probe("soon:TIMED_OUT").unwrap_err().contains("soon"));
        assert!(parse_probe("@5").unwrap_err().contains("no symbol"));
        assert!(parse_probe("5:").unwrap_err().contains("no symbol"));
    }

    /// **An unrecognised flag is an error**, and the message says why rather
    /// than only that. A door a later phase opens must be visible in that
    /// phase's diff.
    #[test]
    fn an_unknown_flag_is_an_error_that_explains_itself() {
        let err = parse(vec!["--pin".into(), "gpio5".into()]).unwrap_err();
        assert!(err.contains("unrecognised flag `--pin`"), "{err}");
        assert!(err.contains("visible in the diff"), "{err}");
        // Including a flag only the classic has: one core, one table, no
        // `--app-mmu-divergence` on this machine.
        assert!(parse(vec!["--app-mmu-divergence".into(), "permit".into()]).is_err());
        assert!(parse(vec!["--uart0-baud".into(), "115200".into()]).is_err());
    }

    /// P06's doors: the flash, the boot mode, D4's policy, the ROM's
    /// console and the strap.
    #[test]
    fn the_flash_and_boot_doors_parse() {
        let a = parse(vec![
            "--boot-mode".into(),
            "rom-up".into(),
            "--merged".into(),
            "chip.bin".into(),
            "--cache-off-fetch".into(),
            "permit".into(),
            "--strap".into(),
            "0x8".into(),
            "--flash-len".into(),
            "0x800000".into(),
        ])
        .unwrap();
        assert_eq!(a.boot_mode, BootMode::RomUp);
        assert_eq!(a.flash, Some(FlashBacking::Copy(PathBuf::from("chip.bin"))));
        assert_eq!(a.cache_off, CacheOffPolicy::Permit);
        assert_eq!(a.strap, Some(8));
        assert_eq!(a.flash_len, Some(8 << 20));
        assert!(parse(vec!["--boot-mode".into(), "sideways".into()]).is_err());
        assert!(parse(vec!["--cache-off-fetch".into(), "maybe".into()]).is_err());
        let b = parse(vec!["--flash".into(), "chip.bin".into()]).unwrap();
        assert_eq!(b.flash, Some(FlashBacking::File(PathBuf::from("chip.bin"))));
        assert_eq!(b.boot_mode, BootMode::Direct, "direct is the default");
        assert_eq!(b.cache_off, CacheOffPolicy::Stop, "stop is the default");
    }

    /// `t2` and `t3` are refused with the reason, not accepted quietly.
    #[test]
    fn only_t1_exists_and_the_refusal_says_why() {
        let err = parse(vec!["--time-grade".into(), "t2".into()]).unwrap_err();
        assert!(err.contains("no measured"), "{err}");
        let err = parse(vec!["--time-grade".into(), "t9".into()]).unwrap_err();
        assert!(err.contains("unknown time grade"), "{err}");
        assert!(parse(vec!["--time-grade".into(), "t1".into()]).is_ok());
    }

    /// A zero window runs nothing, and saying so beats a run that never
    /// advances.
    #[test]
    fn a_zero_core_quantum_is_refused() {
        let err = parse(vec!["--core-quantum".into(), "0".into()]).unwrap_err();
        assert!(err.contains("runs nothing"), "{err}");
        assert_eq!(
            parse(vec!["--core-quantum".into(), "64".into()])
                .unwrap()
                .core_quantum,
            Some(64)
        );
    }

    /// `--cpenable-reset` takes decimal or hex, because the value a reader
    /// will want to try is `0xff` and typing 255 to test a hypothesis about
    /// a register is a papercut with a wrong answer at the end of it.
    #[test]
    fn cpenable_reset_parses_both_bases() {
        assert_eq!(
            parse(vec!["--cpenable-reset".into(), "0xff".into()])
                .unwrap()
                .cpenable_reset,
            Some(0xff)
        );
        assert_eq!(
            parse(vec!["--cpenable-reset".into(), "0".into()])
                .unwrap()
                .cpenable_reset,
            Some(0)
        );
    }

    #[test]
    fn durations_read_the_three_units() {
        assert_eq!(parse_duration("5s").unwrap(), Duration::from_secs(5));
        assert_eq!(
            parse_duration("1500ms").unwrap(),
            Duration::from_millis(1500)
        );
        assert_eq!(parse_duration("900us").unwrap(), Duration::from_micros(900));
        assert!(parse_duration("5m").unwrap_err().contains("unknown unit"));
    }
}
