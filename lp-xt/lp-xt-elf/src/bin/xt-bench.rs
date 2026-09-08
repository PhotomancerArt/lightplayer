//! `xt-bench` — the workload driver behind `just bench-emu-xt`.
//!
//! Loads one linked Xtensa fixture ELF (`lp-xt/fixtures/elf/*.elf`, built by
//! `lp-xt/fixtures/build.sh`) into a fresh [`Emulator`] and runs it to
//! completion `--repeat` times, reporting the retired-instruction and cycle
//! totals on a `stopped after` line shaped like the C6 machine's.
//!
//! **Why repeats.** There is no long-running Xtensa image in this repo — the
//! fixture corpus is a set of short programs, the longest of which retires a
//! few hundred thousand instructions. The probe needs a CPU-bound stretch of
//! ≥100 M instructions to say anything about throughput, so it re-runs the
//! same program from a clean emulator. Each repeat is a full
//! parse-free / load / stage / run cycle, so the reported instructions per
//! second include the loader and the region allocation, exactly as a host
//! engine calling into guest code repeatedly would pay them.
//!
//! **This is an ORACLE, not a gate.** Nothing in CI runs it and no number it
//! prints gates anything. See `scripts/emu/bench-xt.sh` for the A/B protocol.

use std::io::Write as _;
use std::process::ExitCode;

use lp_xt_elf::{GuestHost, XtensaElf};
use lp_xt_emu::trace::{TraceEvent, Tracer};
use lp_xt_emu::{Emulator, NoopTracer, RunOutcome, TextTracer};

const USAGE: &str = "\
usage: xt-bench --elf <path> [options]

  --elf <path>          linked Xtensa ELF to run (required)
  --repeat <n>          run it n times from a fresh emulator (default 1)
  --arg <n>             argument passed to the guest entry (default 0)
  --out <path>          write the guest's collected output here
  --trace <path>        write a text trace of the FIRST repeat here
  --trace-lines <n>     cap the trace at n lines (default 200000)
  --step-budget <n>     per-repeat step budget (default 200000000)
";

/// A [`TextTracer`] that stops recording after `cap` lines.
///
/// The trace is the identity oracle for this probe, and an uncapped one over a
/// multi-million-instruction run is tens of gigabytes. Capping keeps it a
/// deterministic prefix: same run, same first `cap` lines.
struct CapTracer {
    inner: TextTracer,
    cap: usize,
}

impl Tracer for CapTracer {
    fn event(&mut self, event: TraceEvent<'_>) {
        if self.inner.lines.len() < self.cap {
            self.inner.event(event);
        }
    }
}

struct Args {
    elf: String,
    repeat: u32,
    arg: u32,
    out: Option<String>,
    trace: Option<String>,
    trace_lines: usize,
    step_budget: u64,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        elf: String::new(),
        repeat: 1,
        arg: 0,
        out: None,
        trace: None,
        trace_lines: 200_000,
        step_budget: 200_000_000,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--elf" => a.elf = value()?,
            "--repeat" => a.repeat = value()?.parse().map_err(|e| format!("--repeat: {e}"))?,
            "--arg" => a.arg = value()?.parse().map_err(|e| format!("--arg: {e}"))?,
            "--out" => a.out = Some(value()?),
            "--trace" => a.trace = Some(value()?),
            "--trace-lines" => {
                a.trace_lines = value()?
                    .parse()
                    .map_err(|e| format!("--trace-lines: {e}"))?;
            }
            "--step-budget" => {
                a.step_budget = value()?
                    .parse()
                    .map_err(|e| format!("--step-budget: {e}"))?;
            }
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            other => return Err(format!("unknown option {other}")),
        }
    }
    if a.elf.is_empty() {
        return Err("--elf is required".to_string());
    }
    if a.repeat == 0 {
        return Err("--repeat must be at least 1".to_string());
    }
    Ok(a)
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("xt-bench: {e}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    let bytes = match std::fs::read(&args.elf) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("xt-bench: {}: {e}", args.elf);
            return ExitCode::from(1);
        }
    };
    let parsed = match XtensaElf::parse(&bytes) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("xt-bench: {}: {e:?}", args.elf);
            return ExitCode::from(1);
        }
    };

    let mut instructions: u64 = 0;
    let mut cycles: u64 = 0;
    let mut output: Vec<u8> = Vec::new();
    let mut trace: Option<Vec<String>> = None;

    for repeat in 0..args.repeat {
        let mut emu = Emulator::new();
        emu.step_budget = args.step_budget;
        if let Err(e) = parsed.load_into(&mut emu) {
            eprintln!("xt-bench: loading {}: {e:?}", args.elf);
            return ExitCode::from(1);
        }
        let mut host = GuestHost::default();
        let outcome = if repeat == 0 && args.trace.is_some() {
            let mut t = CapTracer {
                inner: TextTracer::new(),
                cap: args.trace_lines,
            };
            let outcome = emu.run_loaded(parsed.entry(), args.arg, &mut t, &mut host);
            trace = Some(t.inner.lines);
            outcome
        } else {
            emu.run_loaded(parsed.entry(), args.arg, &mut NoopTracer, &mut host)
        };

        match outcome {
            RunOutcome::Ok(0) => {}
            RunOutcome::Ok(code) => {
                eprintln!("xt-bench: guest exited {code} on repeat {repeat}");
                if let Some(msg) = &host.panic {
                    eprintln!("xt-bench: guest panic: {msg}");
                }
                return ExitCode::from(1);
            }
            RunOutcome::Trap(t) => {
                eprintln!("xt-bench: trap on repeat {repeat}: {t:?}");
                return ExitCode::from(1);
            }
        }
        instructions += emu.get_instruction_count();
        cycles += emu.get_cycle_count();
        // Only the first repeat's output is kept: every repeat runs the same
        // program from the same clean state, so a differing later repeat is a
        // determinism bug the probe should surface rather than concatenate.
        if repeat == 0 {
            output = host.output;
        } else if host.output != output {
            eprintln!("xt-bench: repeat {repeat} produced different output — not deterministic");
            return ExitCode::from(1);
        }
    }

    if let Some(path) = &args.out {
        if let Err(e) = std::fs::write(path, &output) {
            eprintln!("xt-bench: writing {path}: {e}");
            return ExitCode::from(1);
        }
    }
    if let (Some(path), Some(lines)) = (&args.trace, &trace) {
        let file = match std::fs::File::create(path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("xt-bench: writing {path}: {e}");
                return ExitCode::from(1);
            }
        };
        let mut w = std::io::BufWriter::new(file);
        for line in lines {
            if writeln!(w, "{line}").is_err() {
                eprintln!("xt-bench: writing {path} failed");
                return ExitCode::from(1);
            }
        }
    }

    println!(
        "stopped after {cycles} cycles ({instructions} instructions, {} repeats)",
        args.repeat
    );
    ExitCode::SUCCESS
}
