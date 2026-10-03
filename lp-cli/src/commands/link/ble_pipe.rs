//! `lp-cli link capture blepipe:<port>` (OTA spike): host a board's BLE link
//! through a browser page that only moves frames.
//!
//! Web Bluetooth lives in a browser, and lp-link lives here. The page
//! (`spikes/ble-lab/pipe.html`) connects to the board, subscribes, and then
//! carries one lp-link frame per WebSocket binary message in each direction:
//! a notification from the board becomes one message to us, and one message
//! from us becomes one write without response. It sends the text `up` when a
//! connection is subscribed and `down` when it drops, and it reconnects to the
//! same device by itself (no chooser), which is what an update's resets need.
//!
//! This end listens on `127.0.0.1:<port>`, starts a fresh link on every `up`
//! (each BLE connection is a new session on the board), logs in with
//! `--password` once the board says hello, and with `--ota-offer` offers the
//! build: after the login while an engine runs, or at once to a core-only
//! board's `Q`. Every offer carries one ticket, chosen per run, which is how
//! the core-only board, with no login of its own, knows this host is the one
//! a logged-in link authorized.

use std::io::Write;
use std::net::TcpListener;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use lp_link::LinkConfig;
use lpc_wire::{ClientMessage, ClientRequest, PortRead, WireLinkPort, WireServerMsgBody};
use tungstenite::Message;

use super::args::CaptureArgs;
use crate::commands::emu::link_host::{OtaServe, console_lines, describe_link_counters, fresh_nonce};

const LOGIN_ID_BASE: u64 = 2_000_000;

/// The update's ticket: read from `file` when it holds one (so a later run
/// can finish an update an earlier one authorized), else fresh, and saved
/// there.
fn ticket_for(file: Option<&std::path::Path>) -> Result<[u8; 16]> {
    if let Some(f) = file
        && let Ok(text) = std::fs::read_to_string(f)
        && let Some(t) = parse_hex16(text.trim())
    {
        return Ok(t);
    }
    let mut ticket = [0u8; 16];
    for chunk in ticket.chunks_mut(4) {
        chunk.copy_from_slice(&fresh_nonce().to_le_bytes());
    }
    if let Some(f) = file {
        let hex: String = ticket.iter().map(|b| format!("{b:02x}")).collect();
        std::fs::write(f, hex).with_context(|| format!("saving the ticket to {}", f.display()))?;
    }
    Ok(ticket)
}

fn parse_hex16(s: &str) -> Option<[u8; 16]> {
    if s.len() != 32 {
        return None;
    }
    let mut t = [0u8; 16];
    for (i, b) in t.iter_mut().enumerate() {
        *b = u8::from_str_radix(s.get(2 * i..2 * i + 2)?, 16).ok()?;
    }
    Some(t)
}

pub fn capture(args: &CaptureArgs, port: &str) -> Result<()> {
    let addr = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&addr).with_context(|| format!("listening on {addr}"))?;
    let mut ota = match &args.ota_offer {
        Some(dir) => {
            let mut serve = OtaServe::from_dir(dir)?;
            serve.ticket = Some(ticket_for(args.ota_ticket_file.as_deref())?);
            serve.ahead = args.ota_ahead.unwrap_or(4);
            Some(serve)
        }
        None => None,
    };
    let file = std::fs::File::create(&args.console)
        .with_context(|| format!("creating the console {}", args.console.display()))?;
    let mut console = std::io::LineWriter::new(file);
    let deadline = Duration::from_secs(args.seconds);
    let clock = Instant::now();
    let stamp = |clock: &Instant| clock.elapsed().as_secs_f64();
    let mut matched = false;
    let mut lines = 0usize;
    let mut connections = 0u32;

    eprintln!(
        "link capture: waiting for the pipe page on ws://{addr} (up to {} s) → {}",
        args.seconds,
        args.console.display()
    );
    'pipe: while clock.elapsed() < deadline {
        listener.set_nonblocking(true)?;
        let stream = match listener.accept() {
            Ok((stream, _)) => stream,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
            Err(e) => return Err(e.into()),
        };
        stream.set_nonblocking(false)?;
        stream.set_read_timeout(Some(Duration::from_millis(2)))?;
        stream.set_nodelay(true)?;
        let mut ws = tungstenite::accept(stream).context("WebSocket handshake")?;
        eprintln!("link capture: pipe page attached at {:.3} s", stamp(&clock));
        let mut link: Option<WireLinkPort> = None;
        let mut logged_in = false;
        let mut login_id = LOGIN_ID_BASE;
        let mut out: std::collections::VecDeque<Vec<u8>> = std::collections::VecDeque::new();

        while clock.elapsed() < deadline {
            let now = clock.elapsed().as_micros() as u64;
            match ws.read() {
                Ok(Message::Text(text)) if text.as_str() == "up" => {
                    connections += 1;
                    eprintln!(
                        "link capture: BLE connection {connections} up at {:.3} s",
                        stamp(&clock)
                    );
                    let mut cfg = LinkConfig::ble();
                    cfg.tx_window = args.ble_window;
                    link = Some(WireLinkPort::new(cfg, fresh_nonce(), false));
                    logged_in = false;
                    out.clear();
                }
                Ok(Message::Text(text)) if text.as_str() == "down" => {
                    eprintln!(
                        "link capture: BLE connection down at {:.3} s; host link — {}",
                        stamp(&clock),
                        link.as_ref().map_or_else(String::new, |l| describe_link_counters(&l.counters()))
                    );
                    link = None;
                }
                Ok(Message::Text(text)) => {
                    writeln!(console, "[pipe] {text}")?;
                }
                Ok(Message::Binary(frame)) => {
                    if let Some(link) = link.as_mut() {
                        link.on_datagram(now, &frame);
                    }
                }
                Ok(_) => {}
                Err(tungstenite::Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(e) => {
                    eprintln!("link capture: pipe page gone ({e}) at {:.3} s", stamp(&clock));
                    continue 'pipe;
                }
            }
            let Some(l) = link.as_mut() else {
                continue;
            };
            while let Some(frame) = l.poll_transmit(now) {
                ws.send(Message::Binary(frame.to_vec().into()))?;
            }
            while let Some(read) = l.poll_read() {
                if let PortRead::Up { generation } = &read {
                    eprintln!(
                        "link capture: link up (session {generation}) at {:.3} s",
                        stamp(&clock)
                    );
                }
                if let PortRead::Message(payload) = &read
                    && let Ok(message) = &payload.message
                {
                    match &message.msg {
                        WireServerMsgBody::Hello(_) => {
                            if args.password.is_some() && !logged_in {
                                login_id += 1;
                                l.send_client(&ClientMessage {
                                    id: login_id,
                                    msg: ClientRequest::LoginBegin,
                                })
                                .ok();
                            } else if args.password.is_none()
                                && let Some(ota) = ota.as_mut()
                            {
                                // No login: offer anyway (the refusal test).
                                out.push_back(ota.offer());
                            }
                        }
                        WireServerMsgBody::LoginChallenge { nonce, offers } => {
                            let password = args.password.as_deref().unwrap_or("");
                            let macs = offers
                                .iter()
                                .map(|o| {
                                    let k = lpc_access::derive_login_key(
                                        password.as_bytes(),
                                        &o.salt,
                                        o.iterations,
                                    );
                                    lpc_access::LoginMac::compute(&k, nonce)
                                })
                                .collect();
                            login_id += 1;
                            l.send_client(&ClientMessage {
                                id: login_id,
                                msg: ClientRequest::LoginAnswer { macs },
                            })
                            .ok();
                        }
                        WireServerMsgBody::LoginResult(outcome) => {
                            eprintln!(
                                "link capture: login {outcome:?} at {:.3} s",
                                stamp(&clock)
                            );
                            if matches!(outcome, lpc_access::LoginOutcome::Granted { .. }) {
                                logged_in = true;
                                if let Some(ota) = ota.as_mut() {
                                    out.push_back(ota.offer());
                                }
                            }
                        }
                        _ => {}
                    }
                }
                for line in console_lines(&read) {
                    writeln!(console, "{line}")?;
                    lines += 1;
                    if args
                        .exit_on
                        .as_deref()
                        .is_some_and(|needle| line.contains(needle))
                    {
                        matched = true;
                        break 'pipe;
                    }
                }
            }
            if let Some(ota) = ota.as_mut() {
                while let Some(msg) = l.poll_update() {
                    if msg.first() == Some(&b'A') {
                        eprintln!(
                            "link capture: the board refused the offer (A: log in / ticket) at {:.3} s",
                            stamp(&clock)
                        );
                    }
                    out.extend(ota.answer_all(&msg));
                }
                while let Some(next) = out.front() {
                    match l.send_update(next) {
                        Ok(()) => {
                            out.pop_front();
                        }
                        Err(_) => break,
                    }
                }
                while let Some(frame) = l.poll_transmit(now) {
                    ws.send(Message::Binary(frame.to_vec().into()))?;
                }
            }
        }
    }
    console.flush()?;
    eprintln!(
        "link capture: {} after {:.3} s, {lines} console lines, {connections} BLE connection(s)",
        if matched {
            "stopped on --exit-on"
        } else {
            "reached its deadline"
        },
        stamp(&clock),
    );
    if let Some(ota) = &ota {
        eprintln!(
            "link capture: ota — {} offer(s), {} request(s), {} B served, {} refusal(s), {} auth refusal(s)",
            ota.offers, ota.requests, ota.served_bytes, ota.refusals, ota.auth_refusals
        );
        eprintln!("link capture: ota rates — {}", ota.describe_rates());
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
