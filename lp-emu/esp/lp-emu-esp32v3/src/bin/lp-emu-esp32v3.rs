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
//! **An unrecognised flag is an error.** The doors P6/P7/P8 add (`--uart0`,
//! `--uart0-script`, `--control`, `--flash`, `--cache-off-fetch`) are
//! deliberately *not* stubbed with no-ops, so the phase that adds one is
//! visible in the diff instead of silently changing what an old command line
//! meant.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use lp_emu_esp32v3::machine::{
    AppSource, BootMode, Esp32V3Builder, Machine, Outcome, RomSource, StopCondition, TimeGrade,
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
                            STOP instead of a silent zero. NO PERIPHERAL IS
                            MODELLED YET, so this stops at the first MMIO
                            access of the boot — which is the point: that
                            stop is what M3 P3 reads
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
    --seed <n>              the machine's PRNG seed [0]
    --hooks                 list the ROM hook table and exit. It is EMPTY,
                            and stays empty: try the real ROM path first
    --map                   print the memory map and exit
    -h, --help              this

EXIT CODES:
    0 the deadline was reached   2 the hart faulted
    3 a strict-bus refusal       4 the wall-clock net fired
    5 a --break-at was reached
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
    time_grade: TimeGrade,
    timeout: Option<Duration>,
    wall_timeout: Option<Duration>,
    break_at: Vec<String>,
    probes: Vec<(u64, String)>,
    trace: Option<String>,
    trace_blocks: Vec<String>,
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

    print_build_report(&machine, boot_mode);

    let timeout = args.timeout.unwrap_or(Duration::from_millis(100));
    let stop = StopCondition {
        stop_cycle: Some(emulated_cycles(timeout)),
        wall_timeout: args.wall_timeout,
        probes: args.probes.clone(),
    };

    let outcome = machine.run_until(&stop);
    print_outcome(&mut machine, &outcome);
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
            "  {:<20} {:#010x}..{:#010x}  declared, NO PERIPHERAL MODELLED (M3 P2)",
            span.name,
            span.base,
            span.end()
        );
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
            println!("DEADLINE cycle={cycle} ({micros} us emulated)");
        }
        Outcome::Breakpoint { pc, .. } => {
            let sym = machine.symbolize(*pc).unwrap_or_else(|| "?".into());
            println!("BREAKPOINT pc={pc:#010x} ({sym}) cycle={cycle}");
        }
        Outcome::WallTimeout { .. } => {
            println!("WALL TIMEOUT cycle={cycle} ({micros} us emulated)");
        }
        Outcome::Fault { pc, fault, .. } => {
            let sym = machine.symbolize(*pc).unwrap_or_else(|| "?".into());
            println!("FAULT pc={pc:#010x} ({sym}) cycle={cycle}: {fault:?}");
        }
        Outcome::StrictBus { violation } => {
            let sym = machine
                .symbolize(violation.pc)
                .unwrap_or_else(|| "?".into());
            let where_ = if violation.in_mmio_window {
                "inside the declared MMIO window — an UNMODELLED BLOCK".to_string()
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
            "--trace" => args.trace = Some(value()?),
            "--trace-block" => args.trace_blocks.push(value()?),
            "--seed" => {
                let v = value()?;
                args.seed = v.parse().map_err(|_| format!("--seed {v}: not a number"))?;
            }
            other => {
                return Err(format!(
                    "unrecognised flag `{other}`. This machine has no peripherals yet, so the \
                     doors a later phase adds (--uart0, --control, --flash, --cache-off-fetch) \
                     are absent rather than accepted-and-ignored. `--help` lists what exists."
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
