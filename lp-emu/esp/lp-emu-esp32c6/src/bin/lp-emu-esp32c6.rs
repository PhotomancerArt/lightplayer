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

use lp_emu_esp_common::ScriptedSource;
use lp_emu_esp32c6::loader::EfuseIdentity;
use lp_emu_esp32c6::machine::{
    AppSource, Esp32C6Builder, Esp32C6Machine, Outcome, RomSource, StopCondition, TimeGrade,
    Uart0Sink, UsbSjSink,
};
use lp_emu_esp32c6::memmap;

const USAGE: &str = "\
lp-emu-esp32c6 — the ESP32-C6 machine

USAGE:
    lp-emu-esp32c6 --elf <app.elf> [options]
    lp-emu-esp32c6 --hooks

OPTIONS:
    --elf <path>            the application image to direct-load
    --rom <path>            a mask ROM ELF (default: the vendored C6 rev0 image)
    --time-grade t1|t2      t1 = instruction count, t2 = the per-class model [t1]
    --timeout <5s|1500ms|900us>
                            EMULATED time to run for [100ms]
    --wall-timeout <s>      host-clock safety net; exits 4
    --exit-on <substr>      stop when this appears on UART0
    --uart0 stdout|file:<path>|tcp:<host:port>
                            where UART0's bytes go; tcp: LISTENS, one client at a
                            time, and the client's bytes are UART0's RX (not
                            deterministic: wall clock decides their cycle)
    --uart0-script <file>   scripted host input for UART0, deterministic:
                            one chunk per line: <ms> then a double-quoted
                            string (\\n \\r \\t \\xNN escapes) or hex bytes
                            (`1500 \"M!{...}\\n\"`, `2000 4d 21 0a`)
    --usb-sj stderr|file:<path>
                            where USB-Serial-JTAG's IN-endpoint bytes are
                            OBSERVED — what the guest tried to print with no
                            host attached [kept in memory, summarised at exit]
    --efuse-mac <a0:f2:..>  the MAC the eFuse block reports [the desk board]
    --efuse-rev <0.2>       wafer major.minor [0.2]
    --seed <u64>            the machine PRNG's seed [0]
    --trace [BLOCK,BLOCK]   log every MMIO access; an optional block filter
    --trace-file <path>     write the trace here instead of stderr
    --strict-bus            an access nothing claims is fatal; exits 3
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
    rom: Option<PathBuf>,
    time_grade: TimeGrade,
    timeout: Option<u64>,
    wall_timeout: Option<Duration>,
    exit_on: Option<String>,
    uart0: Uart0Sink,
    uart0_script: Option<PathBuf>,
    usb_sj: UsbSjSink,
    efuse: EfuseIdentity,
    seed: u64,
    trace: bool,
    trace_blocks: Vec<String>,
    trace_file: Option<PathBuf>,
    strict: bool,
    probes: Vec<(u64, String)>,
    break_at: Vec<String>,
    hooks: bool,
    map: bool,
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
        .efuse(args.efuse)
        .seed(args.seed)
        .uart0(args.uart0.clone())
        .usb_sj(args.usb_sj.clone());

    if let Some(rom) = args.rom.clone() {
        builder = builder.rom(RomSource::Path(rom));
    }
    if let Some(elf) = args.elf.clone() {
        builder = builder.app(AppSource::Path(elf));
    } else if !args.hooks {
        return Err("--elf is required (or --hooks / --map)".to_string());
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
        builder = builder.uart0_source(Box::new(source));
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

fn parse_usb_sj(text: &str) -> Result<UsbSjSink, String> {
    match text {
        "stderr" => Ok(UsbSjSink::Stderr),
        "memory" => Ok(UsbSjSink::Memory),
        other => match other.split_once(':') {
            Some(("file", path)) => Ok(UsbSjSink::File(path.into())),
            _ => Err(format!(
                "`{other}` is not a USB-SJ destination (stderr, memory, file:<path>)"
            )),
        },
    }
}

/// `--uart0-script`: one chunk per line, `<ms> <bytes>`, where `<bytes>` is
/// either a double-quoted string with `\n \r \t \\ \" \xNN` escapes or
/// whitespace-separated hex bytes. `#` starts a comment; blank lines are
/// skipped. The millisecond is emulated time from cycle zero; chunks keep
/// file order on the wire whatever their times say (a serial line has an
/// order).
fn parse_uart0_script(text: &str) -> Result<ScriptedSource, String> {
    let mut source = ScriptedSource::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (ms, rest) = line
            .split_once(char::is_whitespace)
            .ok_or_else(|| format!("line {}: expected `<ms> <bytes>`", n + 1))?;
        let ms: u64 = ms
            .trim_end_matches("ms")
            .parse()
            .map_err(|e| format!("line {}: `{ms}` is not a millisecond count: {e}", n + 1))?;
        let rest = rest.trim();
        let bytes = if let Some(quoted) = rest.strip_prefix('"') {
            let body = quoted
                .strip_suffix('"')
                .ok_or_else(|| format!("line {}: unterminated string", n + 1))?;
            unescape(body).map_err(|e| format!("line {}: {e}", n + 1))?
        } else {
            rest.split_whitespace()
                .map(|h| {
                    u8::from_str_radix(h.trim_start_matches("0x"), 16)
                        .map_err(|e| format!("line {}: `{h}` is not a hex byte: {e}", n + 1))
                })
                .collect::<Result<Vec<u8>, String>>()?
        };
        source.push(ms * 1_000 * memmap::CYCLES_PER_US, bytes);
    }
    Ok(source)
}

fn unescape(body: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(body.len());
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            let mut buf = [0u8; 4];
            out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            continue;
        }
        match chars.next() {
            Some('n') => out.push(b'\n'),
            Some('r') => out.push(b'\r'),
            Some('t') => out.push(b'\t'),
            Some('0') => out.push(0),
            Some('\\') => out.push(b'\\'),
            Some('"') => out.push(b'"'),
            Some('x') => {
                let hex: String = chars.by_ref().take(2).collect();
                out.push(
                    u8::from_str_radix(&hex, 16)
                        .map_err(|e| format!("`\\x{hex}` is not a hex byte: {e}"))?,
                );
            }
            Some(other) => return Err(format!("unknown escape `\\{other}`")),
            None => return Err("trailing backslash".to_string()),
        }
    }
    Ok(out)
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
    let usb = machine.usb_sj();
    if !usb.is_empty() {
        eprintln!(
            "usb-sj: host absent; the guest handed {} bytes to the IN endpoint that no host \
             read:\n{}",
            usb.len(),
            usb.text()
                .lines()
                .map(|l| format!("  | {l}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    match outcome {
        Outcome::ExitMatched { .. } => eprintln!("--exit-on matched"),
        Outcome::Deadline { .. } => eprintln!("emulated timeout reached, no fault"),
        Outcome::Reset { cycle, source } => eprintln!(
            "RESET requested by {source} at cycle {cycle} ({} us) — the chip would reboot; \
             the emulator reports it (M7 owns the boot chain)",
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
            eprintln!(
                "STRICT-BUS {:?}{} of {} bytes at {:#010x} from pc={:#010x}{symbol} at cycle {}",
                violation.access,
                if violation.in_mmio_window {
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
        assert_eq!(unescape("a\\nb\\x00c\\\\").unwrap(), b"a\nb\0c\\");
    }

    #[test]
    fn an_unknown_flag_is_an_error_and_not_a_positional() {
        assert!(parse(vec!["--nope".into()]).is_err());
        assert!(parse(vec!["--elf".into()]).is_err(), "--elf needs a value");
    }
}
