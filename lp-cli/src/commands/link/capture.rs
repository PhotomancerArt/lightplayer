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
//!
//! `--request` ([`super::capture_requests`]) makes it the desk's way to ask a
//! board something — a `reboot` above all — with nothing else on the port.
//!
//! `--ota-offer` ([`crate::commands::ota_host`]) drives an over-the-air update
//! on the link's channel 3 with `lpa-update`'s driver. The board resets
//! three times in an update and its USB port goes away each time, so with
//! it a lost port is waited for and reopened rather than the end of the run.

use std::io::Write;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use lpc_wire::{PortRead, WireLinkPort};

use super::args::CaptureArgs;
use super::capture_requests::CaptureRequests;
use super::lab_port::{LabPort, TermiosMode};
use crate::commands::emu::link_host::{console_lines, describe_link_counters, fresh_nonce};
use crate::commands::ota_host::OtaHost;

/// Host the link on `args.target` and write the console until the marker or
/// the deadline.
pub fn capture(args: &CaptureArgs) -> Result<()> {
    let mut requests = CaptureRequests::parse(&args.request)?;
    if args.ota.ota_cut_after.is_some() {
        bail!("--ota-cut-after is `emu run`'s: on a board, cut the power with the hub");
    }
    let mut ota = OtaHost::from_args(&args.ota)?;
    let mut port = Some(LabPort::open(&args.target, TermiosMode::Raw)?);
    // A CH340-bridged classic takes the UART preset, a C6 or S3 the USB one:
    // the vendor id the product's own serial host reads (a socket is `usb()`).
    let config = lpa_client::transport_serial::link_config_for_port(&args.target);
    let mut link = WireLinkPort::new(config, fresh_nonce(), !args.json_replies);
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
        // A board that resets drops its USB port: with an update running,
        // wait for it to come back instead of ending the capture.
        let Some(p) = port.as_mut() else {
            std::thread::sleep(Duration::from_millis(100));
            if let Ok(p) = LabPort::open(&args.target, TermiosMode::Raw) {
                eprintln!(
                    "link capture: port back at {:.3} s",
                    clock.elapsed().as_secs_f64()
                );
                port = Some(p);
            }
            continue;
        };
        let n = match p.read(&mut buf) {
            Ok(n) => n,
            Err(error) if ota.is_some() => {
                eprintln!(
                    "link capture: port lost ({error}) at {:.3} s",
                    clock.elapsed().as_secs_f64()
                );
                port = None;
                continue;
            }
            Err(error) => return Err(error).with_context(|| format!("reading {}", args.target)),
        };
        if n > 0 {
            link.on_bytes(now, &buf[..n]);
        }
        let mut lost = false;
        while let Some(frame) = link.poll_transmit(now) {
            let frame = frame.to_vec();
            if let Err(error) = p.write_all(&frame) {
                if ota.is_none() {
                    return Err(error).with_context(|| format!("writing {}", args.target));
                }
                lost = true;
                break;
            }
        }
        if lost {
            port = None;
            continue;
        }
        let now_ms = now / 1_000;
        while let Some(read) = link.poll_read() {
            match &read {
                PortRead::Up { generation } => {
                    requests.on_session_change();
                    if let Some(ota) = ota.as_mut() {
                        ota.link_up(now_ms);
                    }
                    eprintln!(
                        "link capture: up (session {generation}) at {:.3} s, board nonce {}",
                        clock.elapsed().as_secs_f64(),
                        link.link()
                            .peer_nonce()
                            .map_or_else(|| "unknown".into(), |n| format!("{n:#010x}"))
                    );
                }
                PortRead::Reset { reason } => {
                    requests.on_session_change();
                    if let Some(ota) = ota.as_mut() {
                        ota.link_down(now_ms);
                    }
                    eprintln!(
                        "link capture: reset ({reason:?}) at {:.3} s",
                        clock.elapsed().as_secs_f64()
                    );
                }
                _ => {}
            }
            for line in console_lines(&read) {
                requests.on_line(&line);
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
        if let Some(ota) = ota.as_mut() {
            while let Some(message) = link.poll_update() {
                ota.on_board(now_ms, &message);
            }
            ota.tick(now_ms);
            while let Some(message) = ota.next_outgoing() {
                match link.send_update(message) {
                    Ok(()) => ota.sent(),
                    // The send ring is full: it drains as frames go out.
                    Err(lp_link::SendError::Full) => break,
                    Err(error) => bail!("the link refused an update message: {error:?}"),
                }
            }
            for line in ota.take_lines() {
                eprintln!("link capture: {line}");
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
        if let Some(message) = requests.due() {
            match link.send_client(message) {
                Ok(()) => requests.sent(),
                // The send ring is full: it drains as frames go out.
                Err(lp_link::SendError::Full) => {}
                Err(error) => bail!("the link refused a --request: {error:?}"),
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
    // Log records ride a best-effort channel: one lost on the wire is never
    // resent, so a console missing a line says here whether the WIRE lost
    // it (a sequence gap this end saw) or the board never sent it (its own
    // `[LINK] n log records dropped` line, when its ring overflowed).
    eprintln!(
        "link capture: {} log record(s) lost on the wire",
        link.counters().datagrams_lost
    );
    if let Some(summary) = requests.describe() {
        eprintln!("link capture: {summary}");
    }
    if let Some(ota) = &ota {
        eprintln!("link capture: ota — {}", ota.summary());
    }
    if let Some(why) = requests.unfinished() {
        bail!("{why}");
    }
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
