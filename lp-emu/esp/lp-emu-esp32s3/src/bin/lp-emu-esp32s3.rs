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
//! meant. P04 will add the peripheral flags, P05 the link's, P06 the flash
//! and cache ones, P07 the pads'.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use lp_emu_esp32s3::machine::{
    AppSource, CORE_QUANTUM_DEFAULT, CORES, CPENABLE_RESET_DEFAULT, Esp32S3Builder, Machine,
    Outcome, RomSource, StopCondition, TimeGrade,
};
use lp_emu_esp32s3::{bus_setup, memmap};

const USAGE: &str = "\
lp-emu-esp32s3 — the ESP32-S3 (LX7) machine

USAGE:
    lp-emu-esp32s3 --elf <app.elf> [options]
    lp-emu-esp32s3 --map

OPTIONS:
    --elf <path>            the application image to direct-load. Its
                            PT_LOADs are placed by vaddr and the hart starts
                            at its entry with the boot state a bootloader
                            would have left. REQUIRED: there is no ROM-up
                            boot on this machine yet, and starting the mask
                            ROM's reset path is P06's
    --rom <path>            a mask ROM ELF (default: the vendored ESP32-S3
                            rev0 image, compiled in). Loaded in EVERY
                            configuration: on this chip the ROM is most of
                            the dynamic instruction count, not a formality
    --strict-bus            every access to an address nothing claims is a
                            STOP instead of a silent zero. NO peripheral is
                            modelled in M6 P03, so a strict run stops at the
                            FIRST block the boot touches and says which —
                            which is this phase's deliverable and P04's
                            starting ledger
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
                            when the run ends. ⚠️ EMPTY in M6 P03: the S3's
                            console is USB-Serial-JTAG and nothing drives it
                            until P05. The door exists now so P05 adds a
                            producer rather than a plumbing layer
    --seed <n>              the machine's PRNG seed [0]
    --hooks                 list the ROM hook table and exit. It is EMPTY,
                            and stays empty: try the real ROM path first
    --map                   print the memory map and exit
    -h, --help              this

EXIT CODES (a cross-machine contract — one table for all three machines):
    0 the deadline was reached          2 the hart faulted
    3 a strict-bus refusal              4 the wall-clock net fired
    5 a --break-at was reached
    6 a cache-off fetch — NOT PRODUCIBLE HERE: this machine has no cache
      model, EXTMEM is P06's
    7 a flash-MMU divergence between two cores' tables — NOT APPLICABLE on
      this machine at all: one core runs, so there is no second MMU table to
      diverge from. Reserved across the family, never emitted here
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
    seed: u64,
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
    if args.elf.is_none() && !args.hooks {
        return Err(
            "--elf is required: this machine direct-loads an application, and starting the \
             mask ROM's reset path instead is P06's. `--map` and `--help` need no image."
                .into(),
        );
    }

    let mut builder = Esp32S3Builder::new()
        .time_grade(args.time_grade)
        .strict(args.strict)
        .core_quantum(args.core_quantum.unwrap_or(CORE_QUANTUM_DEFAULT))
        .cpenable_reset(args.cpenable_reset.unwrap_or(CPENABLE_RESET_DEFAULT))
        .seed(args.seed);
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
    };

    let outcome = machine.run_until(&stop);
    machine.bus_mut().host.flush_all();
    print_outcome(&mut machine, &outcome);
    print_run_summary(&machine);
    if let Some(path) = &args.console
        && let Err(e) = std::fs::write(path, machine.console().bytes())
    {
        eprintln!("lp-emu-esp32s3: --console {}: {e}", path.display());
    }
    Ok(ExitCode::from(outcome.exit_code() as u8))
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
            "  {:<20} {:#010x}..{:#010x}  declared; NO block is modelled in M6 P03, so a \
             strict run stops at the first one the boot touches",
            span.name,
            span.base,
            span.end()
        );
    }
    println!("  the blocks the image's own MMIO census names, for P04 to model in stop order:");
    for (name, base) in peripheral_census() {
        println!("    {name:<18} {base:#010x}");
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
    println!("app: {} segments placed", machine.app_segments().len());
    for seg in machine.app_segments().iter().filter(|s| s.relocated()) {
        println!(
            "app: segment vaddr={:#010x} paddr={:#010x} placed by vaddr (memsz {:#x})",
            seg.vaddr, seg.paddr, seg.memsz
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
        Outcome::Breakpoint { core, pc, .. } => {
            let sym = machine.symbolize(*pc).unwrap_or_else(|| "?".into());
            println!("BREAKPOINT core={core} pc={pc:#010x} ({sym}) cycle={cycle}");
        }
        Outcome::WallTimeout { .. } => {
            println!("WALL TIMEOUT cycle={cycle} ({micros} us emulated)");
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
                "inside the declared peripheral window — an UNMODELLED BLOCK. M6 P03 models \
                 none, so this is the expected end of a bring-up run and P04's first entry"
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
        let err = parse(vec!["--uart0".into(), "stdout".into()]).unwrap_err();
        assert!(err.contains("unrecognised flag `--uart0`"), "{err}");
        assert!(err.contains("visible in the diff"), "{err}");
        // Including flags the *other* machines have: the S3's console is
        // USB-Serial-JTAG and `--uart0` is not a door here.
        assert!(parse(vec!["--flash".into(), "x".into()]).is_err());
        assert!(parse(vec!["--boot-mode".into(), "rom-up".into()]).is_err());
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
