//! `lp-cli link capture`: a board's port, opened as the host of its link, in
//! wall-clock time, with what the board says written as a console.
//!
//! Since wire proto 30 a board's USB link is lp-link: its hello, its
//! heartbeats and its log records leave the board only once a host has
//! brought the link up, so a raw reader (`espflash --monitor`,
//! `tty-capture.py`) holds boot text and frames and never the lines a
//! validation payload stops on. This is the reader that replaces them on a
//! board, and it writes the same lines the emulated host does
//! ([`console_lines`]), so an emulated capture and a board's can be replayed
//! against each other.

use std::io::Write;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use lpc_wire::WireLinkPort;

use super::args::CaptureArgs;
use super::lab_port::{LabPort, TermiosMode};
use crate::commands::emu::link_host::{console_lines, describe_link_counters, fresh_nonce};

/// Host the link on `args.target` and write the console until the marker or
/// the deadline.
pub fn capture(args: &CaptureArgs) -> Result<()> {
    let mut port = LabPort::open(&args.target, TermiosMode::Raw)?;
    let mut link = WireLinkPort::new(fresh_nonce(), !args.json_replies);
    let file = std::fs::File::create(&args.console)
        .with_context(|| format!("creating the console {}", args.console.display()))?;
    let mut console = std::io::LineWriter::new(file);
    let deadline = Duration::from_secs(args.seconds);
    let clock = Instant::now();
    let mut buf = [0u8; 4096];
    let mut lines = 0usize;
    let mut matched = false;

    eprintln!(
        "link capture: {} hosted for up to {} s → {}",
        args.target,
        args.seconds,
        args.console.display()
    );
    'run: while clock.elapsed() < deadline {
        let now = clock.elapsed().as_micros() as u64;
        let n = port
            .read(&mut buf)
            .with_context(|| format!("reading {}", args.target))?;
        if n > 0 {
            link.on_bytes(now, &buf[..n]);
        }
        while let Some(frame) = link.poll_transmit(now) {
            let frame = frame.to_vec();
            port.write_all(&frame)
                .with_context(|| format!("writing {}", args.target))?;
        }
        while let Some(read) = link.poll_read() {
            for line in console_lines(&read) {
                writeln!(console, "{line}")?;
                lines += 1;
                if args
                    .exit_on
                    .as_deref()
                    .is_some_and(|needle| line.contains(needle))
                {
                    matched = true;
                    break 'run;
                }
            }
        }
    }
    console.flush()?;
    eprintln!(
        "link capture: {} after {:.3} s, {lines} console lines; host link — {}",
        if matched {
            "stopped on --exit-on"
        } else {
            "reached its deadline"
        },
        clock.elapsed().as_secs_f64(),
        describe_link_counters(&link.counters()),
    );
    if let Some(needle) = &args.exit_on
        && !matched
    {
        bail!(
            "no console line contained `{needle}` within {} s",
            args.seconds
        );
    }
    Ok(())
}
