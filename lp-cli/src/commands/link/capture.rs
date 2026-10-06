//! `lp-cli link capture`: a board's port, opened as the host of its link, in
//! wall-clock time, with what the board says written as a console.
//!
//! Since wire proto 30 a board's USB link is lp-link: its hello, its
//! heartbeats and its log records leave the board only once a host has
//! brought the link up, so a raw reader (`espflash --monitor`,
//! `tty-capture.py`) holds boot text and frames and never the lines a
//! validation payload stops on. This is the reader that replaces them on a
//! board, and it writes the same lines the emulated host does
//! ([`console_lines`](crate::commands::emu::link_host::console_lines)), so
//! an emulated capture and a board's can be replayed against each other.
//!
//! `--request` ([`super::capture_requests`]) makes it the desk's way to ask a
//! board something — a `reboot` above all — with nothing else on the port.
//!
//! `--ota-offer` ([`crate::commands::ota_host`]) drives an over-the-air update
//! on the link's channel 3 with `lpa-update`'s driver. The board resets
//! three times in an update and its USB port goes away each time, so with
//! it a lost port is waited for and reopened rather than the end of the run.
//!
//! `blepipe:<port>` hosts the link over Bluetooth instead, through a browser
//! page that only moves frames ([`super::blepipe_capture`]). Everything above
//! the link is [`CaptureSession`], shared by both.

use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use lpc_wire::{PortRead, WireLinkPort};

use super::args::CaptureArgs;
use super::capture_session::CaptureSession;
use super::lab_port::{LabPort, TermiosMode};
use crate::commands::emu::link_host::{describe_link_counters, fresh_nonce};
use crate::commands::ota_host::OtaHost;

/// The target prefix of a Bluetooth frame pipe (`blepipe:<port>`).
pub const BLEPIPE_PREFIX: &str = "blepipe:";

/// Host the link on `args.target` and write the console until the marker or
/// the deadline.
pub fn capture(args: &CaptureArgs) -> Result<()> {
    if args.ota.ota_cut_after.is_some() {
        bail!("--ota-cut-after is `emu run`'s: on a board, cut the power with the hub");
    }
    if let Some(port) = args.target.strip_prefix(BLEPIPE_PREFIX) {
        let port: u16 = port
            .parse()
            .with_context(|| format!("`{}`: expected blepipe:<port>", args.target))?;
        return super::blepipe_capture::capture_blepipe(args, port);
    }
    let ota = OtaHost::from_args(&args.ota)?;
    let has_ota = ota.is_some();
    let mut session = CaptureSession::create(args, ota)?;
    let mut port = Some(LabPort::open(&args.target, TermiosMode::Raw)?);
    // A CH340-bridged classic takes the UART preset, a C6 or S3 the USB one:
    // the vendor id the product's own serial host reads (a socket is `usb()`).
    let config = lpa_client::transport_serial::link_config_for_port(&args.target);
    let mut link = WireLinkPort::new(config, fresh_nonce(), !args.json_replies);
    let deadline = Duration::from_secs(args.seconds);
    let clock = Instant::now();
    let mut buf = [0u8; 4096];

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
            Err(error) if has_ota => {
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
                if !has_ota {
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
                    session.ota_up(now_ms);
                    eprintln!(
                        "link capture: up (session {generation}) at {:.3} s, board nonce {}",
                        clock.elapsed().as_secs_f64(),
                        link.link()
                            .peer_nonce()
                            .map_or_else(|| "unknown".into(), |n| format!("{n:#010x}"))
                    );
                }
                PortRead::Reset { reason } => {
                    session.ota_down(now_ms);
                    eprintln!(
                        "link capture: reset ({reason:?}) at {:.3} s",
                        clock.elapsed().as_secs_f64()
                    );
                }
                _ => {}
            }
            session.on_read(&read)?;
            if session.matched() {
                break 'run;
            }
        }
        session.pump_ota(&mut link, now_ms)?;
        if session.matched() {
            break 'run;
        }
        session.pump_requests(&mut link)?;
    }
    // Log records ride a best-effort channel: one lost on the wire is never
    // resent, so a console missing a line says here whether the WIRE lost
    // it (a sequence gap this end saw) or the board never sent it (its own
    // `[LINK] n log records dropped` line, when its ring overflowed).
    let lost = format!(
        "{} log record(s) lost on the wire",
        link.counters().datagrams_lost
    );
    session.finish(
        clock.elapsed().as_secs_f64(),
        &describe_link_counters(&link.counters()),
        &[lost],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::link::ble_pipe_fake_board::{
        FakeBoard, FakeMode, board_running, offer_fixture,
    };
    use crate::commands::ota_host::OtaArgs;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};

    /// The serial path over a socket, on the session it shares with the
    /// Bluetooth pipe: the board's hello, an update host that asks `Q` on
    /// channel 3 at once (a USB link is trusted: no login), and a
    /// `--request` that waits for the hello and stops the run on its answer.
    #[test]
    fn a_socket_capture_answers_a_request_and_runs_the_update_host() {
        let fx = offer_fixture();
        let dir = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let args = CaptureArgs {
            target: format!("tcp://{addr}"),
            console: dir.path().join("console.txt"),
            exit_on: Some(format!(
                "M!{{\"id\":{},",
                super::super::capture_requests::REQUEST_ID_BASE
            )),
            seconds: 60,
            json_replies: true,
            request: vec!["hello".into()],
            ota: OtaArgs {
                ota_offer: Some(fx.ota_dir()),
                ..OtaArgs::default()
            },
        };
        let manifest = board_running(&fx);
        let board = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            board_double(stream, manifest)
        });
        capture(&args).expect("the request was answered, and the run stopped on it");
        let board = board.join().unwrap();
        assert_eq!(board.queries, 1, "the update asked before the request went");
        let console = std::fs::read_to_string(&args.console).unwrap();
        assert!(console.contains("[link] up (session"), "{console}");
        assert!(console.contains("[host-ota] offering"), "{console}");
    }

    #[test]
    fn a_blepipe_target_needs_a_port_number() {
        let dir = tempfile::tempdir().unwrap();
        let args = CaptureArgs {
            target: "blepipe:nope".into(),
            console: dir.path().join("console.txt"),
            exit_on: None,
            seconds: 1,
            json_replies: false,
            request: Vec::new(),
            ota: OtaArgs::default(),
        };
        let err = capture(&args).unwrap_err().to_string();
        assert!(err.contains("blepipe:<port>"), "{err}");
    }

    /// A USB board on a socket, until the host goes away.
    fn board_double(mut stream: TcpStream, manifest: lpc_update::BoardManifest) -> FakeBoard {
        stream
            .set_read_timeout(Some(Duration::from_millis(2)))
            .unwrap();
        let mut board = FakeBoard::over_usb(FakeMode::Engine { password: None }, manifest);
        let clock = Instant::now();
        let mut buf = [0u8; 4096];
        while clock.elapsed() < Duration::from_secs(60) {
            let now = clock.elapsed().as_micros() as u64;
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => board.on_bytes(now, &buf[..n]),
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(_) => break,
            }
            for bytes in board.poll(now) {
                if stream.write_all(&bytes).is_err() {
                    return board;
                }
            }
        }
        board
    }
}
