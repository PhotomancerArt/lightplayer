use std::path::Path;

use anyhow::{Context, Result, bail};
use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::machine::{
    AppSource, BootMode, Esp32C6Builder, Esp32C6Machine, FrameSink, Outcome, PinLogSink,
    StopCondition, TimeGrade, Uart0Sink, UsbHost, UsbSjDrain, UsbSjSink,
};
use lp_emu_esp32c6::memmap;

use super::args::{EmuChip, EmuCli, EmuCommand, Grade, LinkKind, RunArgs};

pub fn handle_emu(cli: EmuCli) -> Result<()> {
    match cli.command {
        EmuCommand::Run(args) => run(args),
    }
}

fn run(args: RunArgs) -> Result<()> {
    let EmuChip::Esp32C6 = args.chip;

    let micros = parse_duration_us(&args.timeout)?;
    let grade = match args.time_grade {
        Grade::T1 => TimeGrade::T1,
        Grade::T2 => TimeGrade::T2,
    };

    let mut builder = Esp32C6Builder::new()
        .time_grade(grade)
        .strict(args.strict_bus)
        .usb_host(if args.host_absent {
            UsbHost::Absent
        } else {
            UsbHost::Attached { draining: true }
        })
        // `--monitor` takes the socket out of the port's open/close story:
        // the host is declared attached and draining from power-on and stays
        // that way, so a client that uploads and leaves does not take the
        // console with it.
        .usb_sj_drain(if args.monitor {
            UsbSjDrain::Manual
        } else {
            UsbSjDrain::Auto
        });

    // The image, and with it the boot path. `--merged` is the whole chip:
    // the hart starts at the reset vector and the real ROM finds the
    // bootloader, so a `--flash` beside it would be a second chip and is
    // refused rather than silently ignored.
    match (&args.elf, &args.merged) {
        (Some(elf), None) => {
            check_file(elf, "--elf")?;
            builder = builder.app(AppSource::Path(elf.clone()));
            builder = match &args.flash {
                Some(path) => builder.flash(FlashBacking::File(path.clone())),
                None => builder.flash(FlashBacking::Blank),
            };
        }
        (None, Some(merged)) => {
            check_file(merged, "--merged")?;
            if args.flash.is_some() {
                bail!(
                    "--merged is the whole chip's bytes; --flash would be a second one. \
                     Drop one: --merged for a boot from the reset vector through the real \
                     ROM, --elf --flash for a direct load with a flash part that persists."
                );
            }
            let len = std::fs::metadata(merged)
                .with_context(|| format!("reading {}", merged.display()))?
                .len();
            let len = u32::try_from(len)
                .ok()
                .filter(|n| n.is_power_of_two())
                .with_context(|| {
                    format!(
                        "{} is {len} bytes, which is not a whole flash part. A merged image is the \
                     WHOLE chip padded to its size — `espflash save-image --merge`, or \
                     scripts/emu/build-merged-image.sh.",
                        merged.display()
                    )
                })?;
            builder = builder
                .boot_mode(BootMode::RomUp)
                .flash(FlashBacking::Copy(merged.clone()))
                .flash_len(len);
        }
        (Some(_), Some(_)) => unreachable!("clap's `image` group allows only one"),
        (None, None) => bail!(
            "nothing to run: pass --elf <fw-esp32c6> for a direct load, or --merged \
             <chip.bin> to boot from the reset vector through the real ROM"
        ),
    }

    // The link. Both kinds listen; the difference is which of the chip's two
    // serials the socket is, and a shipped image only speaks on the USB one.
    // With no `--link` neither is a socket: the console is still collected in
    // memory, and `--console` still writes it.
    if let Some(addr) = &args.link {
        builder = match args.link_kind {
            LinkKind::Usb => builder.usb_sj(UsbSjSink::Tcp(addr.clone())),
            LinkKind::Uart0 => builder.uart0(Uart0Sink::Tcp(addr.clone())),
        };
    }
    if let Some(path) = &args.dump_frames {
        builder = builder.dump_frames(FrameSink::File(path.clone()));
    }
    if let Some(path) = &args.pin_log {
        builder = builder.pin_log(PinLogSink::File(path.clone()));
    }

    let mut machine = builder
        .build()
        .map_err(|e| anyhow::anyhow!("building the machine: {e}"))?;

    let link = match args.link_kind {
        LinkKind::Usb => "usb-serial-jtag",
        LinkKind::Uart0 => "uart0",
    };
    match &args.link {
        Some(addr) => eprintln!(
            "emu: esp32c6 {} boot, grade {}, {link} on {addr} — connect with `lp-cli upload \
             <project> serial:tcp://{addr}`",
            machine.boot_mode().as_str(),
            grade.configuration(),
        ),
        None => eprintln!(
            "emu: esp32c6 {} boot, grade {}, no socket (console in memory; pass --link \
             <addr> to serve the {link} link)",
            machine.boot_mode().as_str(),
            grade.configuration(),
        ),
    }
    eprintln!(
        "emu: running for {micros} us of EMULATED time (wall-clock net: {} s)",
        args.wall_timeout_secs
    );

    let stop = StopCondition {
        stop_cycle: Some(micros * memmap::CYCLES_PER_US),
        exit_on: args.exit_on.clone(),
        wall_timeout: Some(std::time::Duration::from_secs(args.wall_timeout_secs)),
        probes: Vec::new(),
    };
    let outcome = machine.run_until(&stop);
    // A frame still open on a pad is reported as incomplete rather than
    // silently dropped.
    machine.flush_frames();

    let console = console_bytes(&machine, args.link_kind);
    if let Some(path) = &args.console {
        std::fs::write(path, &console)
            .with_context(|| format!("writing the console transcript to {}", path.display()))?;
        eprintln!(
            "emu: console → {} ({} bytes)",
            path.display(),
            console.len()
        );
    }

    eprintln!(
        "emu: {} — {} us emulated, {} instructions, {} bytes on the console",
        describe(&outcome),
        machine.micros(),
        machine.instructions(),
        console.len(),
    );
    // Unmapped accesses, always, even on a clean run. An address no
    // peripheral claims reads as zero and the guest believes it — a run that
    // ends well with a hundred of them has told you less than it appears to,
    // and `--strict-bus` is the flag that turns each one into a fault with a
    // pc. Reported rather than gated because a walk is not a bring-up.
    let (reads, writes) = (
        machine.bus.unmapped_reads(),
        machine.bus.unmapped_writes(),
    );
    if reads + writes > 0 {
        eprintln!(
            "emu: {reads} unmapped read(s), {writes} unmapped write(s) at {} distinct site(s) — \
             each read zero and was believed. Re-run with --strict-bus to fault on them.",
            machine.bus.unmapped_sites(),
        );
    } else {
        eprintln!("emu: no unmapped accesses");
    }

    // The machine's own exit code, in this door's vocabulary. `Deadline` and
    // `ExitMatched` are the two ways a run ends well; everything else is a
    // finding, and saying which one it was is the whole value of the line.
    match outcome {
        Outcome::Deadline { .. } | Outcome::ExitMatched { .. } => Ok(()),
        other => bail!("the run ended at {}", describe(&other)),
    }
}

/// What the host saw, on whichever serial the link is.
fn console_bytes(machine: &Esp32C6Machine, kind: LinkKind) -> Vec<u8> {
    match kind {
        LinkKind::Usb => machine.usb_sj().bytes().to_vec(),
        LinkKind::Uart0 => machine.uart0().bytes().to_vec(),
    }
}

fn describe(outcome: &Outcome) -> String {
    match outcome {
        Outcome::Deadline { .. } => "reached its deadline".to_string(),
        Outcome::ExitMatched { .. } => "stopped on --exit-on".to_string(),
        Outcome::Fault { pc, fault, .. } => format!("faulted at {pc:#010x}: {fault:?}"),
        Outcome::StrictBus { violation } => format!("strict-bus refused an access: {violation:?}"),
        Outcome::Reset { source, strap, .. } => {
            format!("the chip asked to reset ({source}, into {strap:?})")
        }
        Outcome::WallTimeout { .. } => {
            "hit the wall-clock net — raise --wall-timeout, or lower --timeout".to_string()
        }
        Outcome::Breakpoint { pc, .. } => format!("stopped at a breakpoint, pc {pc:#010x}"),
    }
}

fn check_file(path: &Path, flag: &str) -> Result<()> {
    if !path.is_file() {
        bail!("{flag} {} is not a file", path.display());
    }
    Ok(())
}

/// `5s`, `1500ms`, `900us` → microseconds. The unit is required: a bare
/// number would read as seconds to one caller and microseconds to the next,
/// and this clock is emulated, which is confusing enough already.
fn parse_duration_us(text: &str) -> Result<u64> {
    let (digits, scale) = if let Some(d) = text.strip_suffix("ms") {
        (d, 1_000u64)
    } else if let Some(d) = text.strip_suffix("us") {
        (d, 1)
    } else if let Some(d) = text.strip_suffix('s') {
        (d, 1_000_000)
    } else {
        bail!("--timeout `{text}` has no unit — write 5s, 1500ms or 900us (emulated time)");
    };
    let n: u64 = digits
        .parse()
        .with_context(|| format!("--timeout `{text}`"))?;
    Ok(n * scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_duration_needs_a_unit() {
        assert_eq!(parse_duration_us("5s").unwrap(), 5_000_000);
        assert_eq!(parse_duration_us("1500ms").unwrap(), 1_500_000);
        assert_eq!(parse_duration_us("900us").unwrap(), 900);
        assert!(parse_duration_us("100").is_err());
        assert!(parse_duration_us("s").is_err());
    }
}
