//! `lp-emu-esp32v3` — run a classic ESP32 image on the machine.
//!
//! Every timeout is **emulated** time, so a run is the same run on a laptop
//! and on a loaded CI box (PD9). `--wall-timeout` is the single wall-clock
//! input and it is a safety net: it can end a run, never change one.
//!
//! The argument parser is written out by hand rather than pulled from a
//! crate, for the C6's reasons: the flag list is fixed and small, the exit
//! codes are a contract (0 / 2 / 3 / 4 / 5), and the error text is part of
//! what a bring-up session reads.
//!
//! **An unrecognised flag is an error.** A door a phase has not opened is
//! deliberately *not* stubbed with a no-op, so the phase that adds one is
//! visible in the diff instead of silently changing what an old command line
//! meant. `--cache-off-fetch` arrived with P4 and D4; P6 added the six the
//! console and the cable need (`--uart0`, `--uart0-script`, `--uart0-baud`,
//! `--control`, `--control-script`, `--exit-on`); P7 added the four the flash
//! chip needs (`--flash`, `--flash-copy`, `--merged`, `--flash-len`).

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use lp_emu_esp32v3::cache::CacheOffPolicy;
use lp_emu_esp32v3::control;
use lp_emu_esp32v3::flash::FlashBacking;
use lp_emu_esp32v3::loader::EfuseIdentity;
use lp_emu_esp32v3::machine::{
    AppSource, BootMode, Esp32V3Builder, Machine, Outcome, RomSource, StopCondition, TimeGrade,
    Uart0Sink,
};
use lp_emu_esp32v3::{bus_setup, memmap};

const USAGE: &str = "\
lp-emu-esp32v3 — the classic ESP32 (v3, LX6) machine

USAGE:
    lp-emu-esp32v3 --elf <app.elf> [options]
    lp-emu-esp32v3 --boot-mode rom-up [options]

OPTIONS:
    --elf <path>            the application image to direct-load. Its
                            PT_LOADs are placed by vaddr and the hart starts
                            at its entry with the boot state a bootloader
                            would have left. With --boot-mode rom-up it is
                            never loaded: it is the symbol table --probe and
                            --break-at read
    --boot-mode direct|rom-up
                            direct = place --elf's segments and start at its
                            entry; rom-up = start at the mask ROM's RESET
                            VECTOR (0x40000400) and let the real ROM run
                            [direct, or rom-up when there is no --elf]
    --rom <path>            a mask ROM ELF (default: the vendored ESP32
                            rev300 image, compiled in)
    --strict-bus            every access to an address nothing claims is a
                            STOP instead of a silent zero. Every block is an
                            accept-and-remember probe (M3 P3), so a boot
                            reaches its first spin on a register only a
                            model can answer — see the phase report for
                            which phase owns which
    --cache-off-fetch stop|permit
                            D4. `stop` (the default, and what every gate run
                            uses) ends the run the first time a core reaches
                            through a flash window with its own read cache
                            disabled, naming the access and the write that
                            disabled the cache. It claims nothing else: not
                            the stall duration, not that silicon would crash,
                            not that the access is a bug. `permit` does not
                            check at all, and continues [stop]
    --flash <path>          back the flash chip with this file: it is read at
                            start and written back when the run ends. A file
                            that does not exist is created blank
    --flash-copy <path>     read the file once and never write it back — a
                            scratch copy of a known image
    --merged <path>         --flash-copy, spelled for what a ROM-up boot
                            wants: the whole 4 MiB `espflash save-image
                            --chip esp32 --merge` chip. The chip's length is
                            taken from the file
    --flash-len <bytes>     the chip's size [4194304, the desk board's 4 MB]
    --time-grade t1         t1 = cycles are instructions. The only grade this
                            machine defines; see --help output for why [t1]
    --timeout <5s|1500ms|900us>
                            EMULATED time to run for [100ms]
    --wall-timeout <s>      host-clock safety net; exits 4
    --break-at <symbol>     stop at the symbol's first instruction, with
                            every register as the caller left it
    --probe <cycle>:<name>  print the word at symbol <name> when guest time
                            reaches <cycle>
    --trace <path|->        write the bus trace here
    --trace-block <name>    only trace this block (repeatable)
    --uart0 <spec>          where UART0's bytes go: `-` or `stdout`,
                            `file:<path>`, or `tcp:<addr>` to LISTEN for one
                            client at a time (whose bytes are UART0's RX).
                            They are always also kept in memory [memory]
    --uart0-script <path>   deterministic host input on the wire: bytes at
                            declared EMULATED times, an after-the-device-said-it
                            form and a then-+<ms> form. The deterministic
                            path — a live socket lets the host clock decide
                            when a byte lands, a script does not
    --uart0-baud <n>        the rate the host at the other end of the cable
                            sends at [115200]. It changes what the auto-baud
                            counters report and NOTHING else; it never
                            overrides what the guest writes to clkdiv
    --control <tcp:addr>    LISTEN for a control-channel client: the CH340
                            cable's own socket (attach/detach/open/close/
                            dtr/rts/signals/reset/download-mode/state). One
                            reply line per command
    --control-script <path> the same verbs at declared EMULATED times — the
                            deterministic twin of a control client
    --reboot-on-reset       the auto-reset circuit releasing EN reboots the
                            machine instead of ending the run. Off by
                            default: a reboot costs a copy of guest memory
                            taken at build time
    --exit-on <line>        stop when this appears on a COMPLETE line of
                            UART0's output; exits 0
    --seed <n>              the machine's PRNG seed [0]
    --efuse-mac <a:b:..>    the MAC the eFuse block answers [30:76:f5:ec:f6:34, the desk board]
    --efuse-rev <maj.min>   the chip revision it answers [3.1, the desk board]
    --hooks                 list the ROM hook table and exit. It is EMPTY,
                            and stays empty: try the real ROM path first
    --map                   print the memory map and exit
    -h, --help              this

EXIT CODES:
    0 the deadline was reached, or --exit-on matched
    2 the hart faulted, or the cable reset the chip without --reboot-on-reset
    3 a strict-bus refusal       4 the wall-clock net fired
    5 a --break-at was reached  6 a cache-off fetch (D4)
";

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .init();
    match run() {
        Ok(code) => code,
        Err(message) => {
            eprintln!("lp-emu-esp32v3: {message}");
            ExitCode::from(64)
        }
    }
}

#[derive(Debug, Default)]
struct Args {
    elf: Option<PathBuf>,
    rom: Option<PathBuf>,
    boot_mode: Option<BootMode>,
    strict: bool,
    cache_off: CacheOffPolicy,
    time_grade: TimeGrade,
    timeout: Option<Duration>,
    wall_timeout: Option<Duration>,
    break_at: Vec<String>,
    probes: Vec<(u64, String)>,
    trace: Option<String>,
    trace_blocks: Vec<String>,
    uart0: Uart0Sink,
    uart0_script: Option<PathBuf>,
    uart0_baud: Option<u64>,
    control: Option<String>,
    control_script: Option<PathBuf>,
    reboot_on_reset: bool,
    exit_on: Option<String>,
    seed: u64,
    efuse: EfuseIdentity,
    flash: Option<FlashBacking>,
    flash_len: Option<u32>,
    hooks: bool,
    map: bool,
    help: bool,
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

    let boot_mode = args.boot_mode.unwrap_or(if args.elf.is_some() {
        BootMode::Direct
    } else {
        BootMode::RomUp
    });
    if boot_mode == BootMode::Direct && args.elf.is_none() {
        return Err("--boot-mode direct needs an --elf".into());
    }

    let mut builder = Esp32V3Builder::new()
        .boot_mode(boot_mode)
        .time_grade(args.time_grade)
        .strict(args.strict)
        .cache_off_fetch(args.cache_off)
        .seed(args.seed)
        .efuse(args.efuse)
        .uart0(args.uart0.clone())
        .reboot_on_reset(args.reboot_on_reset);
    if let Some(baud) = args.uart0_baud {
        builder = builder.uart0_baud(baud);
    }
    if let Some(path) = &args.uart0_script {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("--uart0-script {}: {e}", path.display()))?;
        let script = control::parse_byte_script(&text)
            .map_err(|e| format!("--uart0-script {}: {e}", path.display()))?;
        println!(
            "uart0 script: {} chunks, {} bytes",
            script.chunks(),
            script.remaining()
        );
        builder = builder.uart0_script(script);
    }
    if let Some(addr) = &args.control {
        builder = builder.control(addr.clone());
    }
    if let Some(path) = &args.control_script {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("--control-script {}: {e}", path.display()))?;
        let script = control::parse_control_script(&text)
            .map_err(|e| format!("--control-script {}: {e}", path.display()))?;
        println!("control script: {} commands", script.len());
        builder = builder.control_script(script);
    }
    if let Some(backing) = args.flash.clone() {
        // A merged image says how big the part it was built for is; taking
        // the length from the file rather than from a flag is what stops a
        // `--merged` run from silently truncating one.
        if args.flash_len.is_none()
            && let FlashBacking::File(p) | FlashBacking::Copy(p) = &backing
            && let Ok(meta) = std::fs::metadata(p)
        {
            builder = builder.flash_len(meta.len() as u32);
        }
        builder = builder.flash(backing);
    }
    if let Some(len) = args.flash_len {
        builder = builder.flash_len(len);
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

    print_build_report(&machine, boot_mode);

    let timeout = args.timeout.unwrap_or(Duration::from_millis(100));
    let stop = StopCondition {
        stop_cycle: Some(emulated_cycles(timeout)),
        exit_on: args.exit_on.clone(),
        wall_timeout: args.wall_timeout,
        probes: args.probes.clone(),
    };

    let outcome = machine.run_until(&stop);
    machine.bus_mut().host.flush_all();
    print_outcome(&mut machine, &outcome);
    {
        let chip = machine.flash().lock().expect("flash poisoned");
        println!("flash: {}", chip.command_census());
    }
    match machine.flush_flash() {
        Ok(true) => println!("flash: written back"),
        Ok(false) => {}
        Err(e) => eprintln!("lp-emu-esp32v3: writing the flash image back: {e}"),
    }
    Ok(ExitCode::from(outcome.exit_code() as u8))
}

/// `--timeout` is emulated time: `micros * 240`.
fn emulated_cycles(d: Duration) -> u64 {
    (d.as_micros() as u64).saturating_mul(memmap::CYCLES_PER_US)
}

fn print_map() {
    println!("classic ESP32 (v3) memory map — every base cites lp-emu-esp32v3::memmap");
    for span in memmap::RAM_SPANS {
        println!(
            "  {:<20} {:#010x}..{:#010x}  {:>9} B",
            span.name,
            span.base,
            span.end(),
            span.len
        );
    }
    for span in memmap::MMIO_WINDOWS {
        println!(
            "  {:<20} {:#010x}..{:#010x}  declared; the blocks below are accept-and-remember \
             (M3 P3), everything else in it is unmapped",
            span.name,
            span.base,
            span.end()
        );
    }
    match Esp32V3Builder::new().boot_mode(BootMode::RomUp).build() {
        Ok(machine) => {
            for (name, base, len) in machine.peripheral_map() {
                println!("    {name:<18} {base:#010x}..{:#010x}", base + len);
            }
            println!("  the AHB mirror: the same blocks, the same state, a second decode (DD38)");
            for (name, base, len) in machine.peripheral_alias_map() {
                println!(
                    "    {name:<18} {base:#010x}..{:#010x}  alias of {:#010x}",
                    base + len,
                    memmap::ahb_to_dport(*base).unwrap_or(0)
                );
            }
        }
        Err(e) => println!("    (could not build the boot set: {e})"),
    }
    println!("  deliberately unmapped:");
    for (span, why) in bus_setup::deliberately_unmapped() {
        println!(
            "  {:<20} {:#010x}..{:#010x}  {why}",
            span.name,
            span.base,
            span.end()
        );
    }
}

fn print_hooks(machine: &Machine) {
    let hooks = machine.hooks();
    if hooks.is_empty() {
        println!(
            "the ROM hook table is EMPTY, and that is the default. A hook is a last \
             resort: try the real ROM path first, and add one only when it cannot be made \
             to work by giving a register a peripheral model owns its documented reset value."
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

fn print_build_report(machine: &Machine, boot_mode: BootMode) {
    let image = machine.rom_data_image();
    println!(
        "rom: {} PT_LOADs placed, {} empty; {} non-alloc sections seeded ({} B)",
        machine.rom_segments().len(),
        lp_emu_esp32v3::rom::empty_segments(machine.rom()),
        image.sections,
        image.bytes,
    );
    if boot_mode == BootMode::Direct {
        println!("app: {} segments placed", machine.app_segments().len());
        for seg in machine.app_segments().iter().filter(|s| s.relocated()) {
            println!(
                "app: segment vaddr={:#010x} paddr={:#010x} placed by vaddr (memsz {:#x})",
                seg.vaddr, seg.paddr, seg.memsz
            );
        }
        if let Some(seed) = machine.flash_seed() {
            println!(
                "flash chip: {} @ {:#010x} chip_size {:#x} -> {:#x} ({} MiB)",
                lp_emu_esp32v3::loader::ROM_FLASH_CHIP_SYMBOL,
                seed.chip,
                seed.previous,
                seed.chip_size,
                seed.chip_size >> 20
            );
        }
        let staging = machine.flash_staging();
        if !staging.pages.is_empty() {
            let first = staging.pages.first().expect("non-empty");
            let last = staging.pages.last().expect("non-empty");
            println!(
                "flash staging: {} pages, {} B, factory {:#010x}..{:#010x}; \
                 {:#010x}->{:#010x} .. {:#010x}->{:#010x}",
                staging.pages.len(),
                staging.bytes,
                first.paddr,
                last.paddr + 0x1_0000,
                first.vaddr,
                first.paddr,
                last.vaddr,
                last.paddr,
            );
        }
        if let Some(frame) = machine.boot_frame() {
            println!(
                "boot frame: a1={:#010x}, save area [a1-16..a1) = {:#010x} {:#010x} {:#010x} {:#010x}",
                frame.sp,
                frame.save_area[0],
                frame.save_area[1],
                frame.save_area[2],
                frame.save_area[3]
            );
        }
    }
    {
        let chip = machine.flash().lock().expect("flash poisoned");
        println!(
            "flash: {} B ({} MiB), backing {:?}, jedec {:#010x}; cache fills {}",
            chip.len(),
            chip.len() >> 20,
            chip.backing(),
            chip.jedec_id(),
            machine.cache_fills(),
        );
    }
    println!(
        "time grade: {} ({})",
        "t1",
        machine.time_grade().configuration()
    );
    for line in machine.core_report() {
        println!("{line}");
    }
}

fn print_outcome(machine: &mut Machine, outcome: &Outcome) {
    let cycle = outcome.cycle();
    let micros = cycle / memmap::CYCLES_PER_US;
    match outcome {
        Outcome::Deadline { .. } => {
            // Where the hart was when time ran out: a boot that is spinning
            // on a register an accept block cannot answer ends here, and the
            // pc is the whole diagnosis.
            let pc = machine.harts[0].pc();
            let sym = machine.symbolize(pc).unwrap_or_else(|| "?".into());
            println!("DEADLINE cycle={cycle} ({micros} us emulated) pc={pc:#010x} ({sym})");
        }
        Outcome::Breakpoint { pc, .. } => {
            let sym = machine.symbolize(*pc).unwrap_or_else(|| "?".into());
            println!("BREAKPOINT pc={pc:#010x} ({sym}) cycle={cycle}");
        }
        Outcome::WallTimeout { .. } => {
            println!("WALL TIMEOUT cycle={cycle} ({micros} us emulated)");
        }
        Outcome::CacheOffFetch { pc, access, .. } => {
            println!("{}", machine.cache_off_message(cycle, *pc, access));
        }
        Outcome::ExitMatched { .. } => {
            println!("EXIT MATCHED cycle={cycle} ({micros} us emulated)");
        }
        Outcome::Reset { strap, .. } => {
            println!(
                "RESET cycle={cycle} ({micros} us emulated): the auto-reset circuit released \
                 EN with IO0 {} — strap {strap}. Pass --reboot-on-reset to make the machine \
                 actually reboot instead of stopping here.",
                if *strap == lp_emu_esp_common::Strap::Download {
                    "low"
                } else {
                    "high"
                }
            );
        }
        Outcome::Fault { pc, fault, .. } => {
            let sym = machine.symbolize(*pc).unwrap_or_else(|| "?".into());
            println!("FAULT pc={pc:#010x} ({sym}) cycle={cycle}: {fault:?}");
            // The exception registers, because a fault *inside a vector* says
            // nothing about what asked for it. The classic's mask ROM ends
            // its debug vector in `simcall`, so a guest that double-faults
            // reports an unsupported opcode at `_DebugExceptionVector+0x5`
            // and the useful address is `EPC1` (`m3/notes.md`: the earliest
            // cause is the root).
            let sr = machine.harts[0].sr();
            let name = |at: u32| match machine.symbolize(at) {
                Some(n) => format!(" ({n})"),
                None => String::new(),
            };
            println!(
                "  EXCCAUSE={} EXCVADDR={:#010x} EPC1={:#010x}{} PS={:#010x}",
                sr.exccause,
                sr.excvaddr,
                sr.epc[1],
                name(sr.epc[1]),
                machine.harts[0].ps(),
            );
        }
        Outcome::StrictBus { violation } => {
            let sym = machine
                .symbolize(violation.pc)
                .unwrap_or_else(|| "?".into());
            let where_ = if violation.in_mmio_window {
                match memmap::ahb_to_dport(violation.address) {
                    Some(twin) => format!(
                        "inside the declared AHB peripheral window — an UNMODELLED BLOCK; \
                         its DPORT twin is {twin:#010x} (memmap::MMIO_AHB_BASE)"
                    ),
                    None => "inside the declared DPORT peripheral window — an UNMODELLED BLOCK"
                        .to_string(),
                }
            } else if let Some((span, why)) = bus_setup::unmapped_window(violation.address) {
                format!(
                    "inside `{}`, which this machine deliberately does not map: {why}",
                    span.name
                )
            } else {
                "outside every region and every declared window".to_string()
            };
            println!(
                "STRICT BUS STOP\n  \
                 pc      = {:#010x} ({sym})\n  \
                 cycle   = {} ({} us emulated)\n  \
                 access  = {:?} {:?} at {:#010x}\n  \
                 where   = {where_}",
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
            "--cache-off-fetch" => args.cache_off = CacheOffPolicy::parse(&value()?)?,
            "--elf" => args.elf = Some(PathBuf::from(value()?)),
            "--rom" => args.rom = Some(PathBuf::from(value()?)),
            "--boot-mode" => {
                let v = value()?;
                args.boot_mode = Some(
                    BootMode::parse(&v)
                        .ok_or_else(|| format!("--boot-mode {v}: expected direct or rom-up"))?,
                );
            }
            "--time-grade" => args.time_grade = TimeGrade::parse(&value()?)?,
            "--timeout" => args.timeout = Some(parse_duration(&value()?)?),
            "--wall-timeout" => args.wall_timeout = Some(parse_duration(&value()?)?),
            "--break-at" => args.break_at.push(value()?),
            "--probe" => {
                let v = value()?;
                let (cycle, name) = v
                    .split_once(':')
                    .ok_or_else(|| format!("--probe {v}: expected <cycle>:<symbol>"))?;
                let cycle: u64 = cycle
                    .parse()
                    .map_err(|_| format!("--probe {v}: `{cycle}` is not a cycle count"))?;
                args.probes.push((cycle, name.to_string()));
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
            "--trace" => args.trace = Some(value()?),
            "--uart0" => {
                let v = value()?;
                args.uart0 = match v.as_str() {
                    "-" | "stdout" => Uart0Sink::Stdout,
                    "memory" => Uart0Sink::Memory,
                    other => match other.split_once(':') {
                        Some(("file", path)) => Uart0Sink::File(PathBuf::from(path)),
                        Some(("tcp", addr)) => Uart0Sink::Tcp(addr.to_string()),
                        _ => {
                            return Err(format!(
                                "--uart0 {other}: expected `-`, `stdout`, `memory`, \
                                 `file:<path>` or `tcp:<addr>`"
                            ));
                        }
                    },
                };
            }
            "--uart0-script" => args.uart0_script = Some(PathBuf::from(value()?)),
            "--uart0-baud" => {
                let v = value()?;
                args.uart0_baud = Some(
                    v.parse()
                        .map_err(|_| format!("--uart0-baud {v}: not a number"))?,
                );
            }
            "--control" => {
                let v = value()?;
                let addr = v.strip_prefix("tcp:").unwrap_or(&v);
                args.control = Some(addr.to_string());
            }
            "--control-script" => args.control_script = Some(PathBuf::from(value()?)),
            "--reboot-on-reset" => args.reboot_on_reset = true,
            "--exit-on" => args.exit_on = Some(value()?),
            "--trace-block" => args.trace_blocks.push(value()?),
            "--efuse-mac" => {
                let v = value()?;
                args.efuse.mac =
                    EfuseIdentity::parse_mac(&v).map_err(|e| format!("--efuse-mac {v}: {e}"))?;
            }
            "--efuse-rev" => {
                let v = value()?;
                let (major, minor) =
                    EfuseIdentity::parse_rev(&v).map_err(|e| format!("--efuse-rev {v}: {e}"))?;
                args.efuse.chip_major = major;
                args.efuse.chip_minor = minor;
            }
            "--seed" => {
                let v = value()?;
                args.seed = v.parse().map_err(|_| format!("--seed {v}: not a number"))?;
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
