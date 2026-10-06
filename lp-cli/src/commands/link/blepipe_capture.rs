//! `lp-cli link capture blepipe:<port>`: host a board's Bluetooth link
//! through the pipe page, a browser tab that only moves frames.
//!
//! This end listens on `ws://127.0.0.1:<port>` for the page
//! (`spikes/ble-lab/pipe.html?ws=ws://127.0.0.1:<port>`) and runs
//! [`BlePipeHost`] on what it sends: binary messages are lp-link frames (one
//! notification each), text messages `up`, `down` and `log …`. Every frame
//! the host writes goes back as one binary message, which the page writes as
//! one GATT write.
//!
//! The page owns the connection and reconnects by itself after a drop (the
//! board resets three times in an update); this end only follows. If the
//! page itself goes away — a reload, a crashed tab — its connection is gone
//! with it, and the next page to attach is taken as the new one. A second
//! page attaching while one is open replaces it.

use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use lpa_update::ServeConfig;
use tungstenite::{Message, WebSocket};

use super::args::CaptureArgs;
use super::ble_pipe_host::{BlePipeHost, PageMessage};
use super::capture_session::CaptureSession;
use crate::commands::ota_host::OtaHost;

/// How long a read waits for the page before the link's timers run again.
const READ_WAIT: Duration = Duration::from_millis(2);

/// Host `args`' capture over the pipe page on `port`.
pub fn capture_blepipe(args: &CaptureArgs, port: u16) -> Result<()> {
    let addr = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&addr).with_context(|| format!("listening on {addr}"))?;
    listener.set_nonblocking(true)?;
    let ota = OtaHost::from_args_over(&args.ota, ServeConfig::BLE)?;
    let session = CaptureSession::create(args, ota)?;
    let mut host = BlePipeHost::new(
        session,
        args.ota.ota_password.as_deref(),
        !args.json_replies,
    );
    let deadline = Duration::from_secs(args.seconds);
    let clock = Instant::now();
    let mut page: Option<WebSocket<TcpStream>> = None;
    let mut out = Vec::new();

    eprintln!(
        "link capture: waiting for the pipe page on ws://{addr} (up to {} s) → {}",
        args.seconds,
        args.console.display()
    );
    while clock.elapsed() < deadline && !host.matched() {
        let now = clock.elapsed().as_micros() as u64;
        match listener.accept() {
            Ok((stream, _)) => {
                let ws = accept_page(stream)?;
                if page.is_some() {
                    host.page_gone(now)?;
                }
                eprintln!(
                    "link capture: pipe page attached at {:.3} s",
                    clock.elapsed().as_secs_f64()
                );
                page = Some(ws);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e).context("accepting the pipe page"),
        }
        let Some(ws) = page.as_mut() else {
            std::thread::sleep(Duration::from_millis(20));
            continue;
        };
        let mut gone = false;
        match ws.read() {
            Ok(Message::Binary(frame)) => host.on_page(now, PageMessage::Frame(frame))?,
            Ok(Message::Text(text)) => host.on_page(now, PageMessage::from_text(&text))?,
            Ok(Message::Close(_)) => gone = true,
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => gone = true,
        }
        if !gone {
            out.clear();
            host.poll(now, &mut out)?;
            for frame in out.drain(..) {
                if ws.send(Message::Binary(frame)).is_err() {
                    gone = true;
                    break;
                }
            }
        }
        if gone {
            eprintln!(
                "link capture: pipe page gone at {:.3} s",
                clock.elapsed().as_secs_f64()
            );
            host.page_gone(now)?;
            page = None;
        }
    }
    if let Some(mut ws) = page {
        ws.close(None).ok();
        ws.flush().ok();
    }
    host.finish(clock.elapsed().as_secs_f64())
}

/// The page's WebSocket handshake, on a blocking stream whose reads give
/// up after [`READ_WAIT`] so the link's timers keep running.
fn accept_page(stream: TcpStream) -> Result<WebSocket<TcpStream>> {
    // An accepted socket inherits the listener's non-blocking mode on BSD.
    stream.set_nonblocking(false)?;
    stream.set_nodelay(true)?;
    let ws = tungstenite::accept(stream).context("the pipe page's WebSocket handshake")?;
    ws.get_ref().set_read_timeout(Some(READ_WAIT))?;
    Ok(ws)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::link::ble_pipe_fake_board::{
        FakeBoard, FakeMode, board_running, offer_fixture,
    };
    use crate::commands::ota_host::OtaArgs;

    /// The whole command over a real socket: a WebSocket client stands in
    /// for the page (and the radio), relaying frames to a fake engine; the
    /// capture logs in, asks, finds the board up to date and stops on
    /// `--exit-on`. Then the page drops and reconnects to a board that came
    /// back core-only, and the console says both connections.
    #[test]
    fn a_capture_over_the_pipe_logs_in_asks_and_stops_on_its_marker() {
        let fx = offer_fixture();
        let dir = tempfile::tempdir().unwrap();
        let port = free_port();
        let args = CaptureArgs {
            target: format!("blepipe:{port}"),
            console: dir.path().join("console.txt"),
            exit_on: Some("[host-ota] done: UpToDate".into()),
            seconds: 60,
            json_replies: false,
            request: Vec::new(),
            ota: OtaArgs {
                ota_offer: Some(fx.ota_dir()),
                ota_password: Some("desk-lab".into()),
                ..OtaArgs::default()
            },
        };
        let manifest = board_running(&fx);
        let page = std::thread::spawn(move || page_double(port, manifest));
        super::super::capture::capture(&args).expect("the capture stopped on its marker");
        let (first, second) = page.join().unwrap();
        assert!(
            first.granted && first.queries == 1,
            "connection 1: login, then Q"
        );
        assert_eq!(second.queries, 1, "connection 2: a new link, asked again");

        let console = std::fs::read_to_string(&args.console).unwrap();
        assert!(
            console.contains("[pipe] 0.001 ble up (LP-test)"),
            "{console}"
        );
        assert!(
            console.contains("[host-ble] Bluetooth connection 1 up"),
            "{console}"
        );
        assert!(
            console.contains("[host-ble] the engine's login granted Edit"),
            "{console}"
        );
        assert!(
            console.contains("[host-ble] Bluetooth connection 1 down"),
            "{console}"
        );
        assert!(
            console.contains("[host-ble] Bluetooth connection 2 up"),
            "{console}"
        );
        assert!(console.contains("runs its core alone"), "{console}");
        assert!(console.contains("[host-ota] done: UpToDate"), "{console}");
    }

    /// The page: connects to the capture, says `up`, relays frames to an
    /// engine that never answers `Q` (it "resets"), says `down`, then `up`
    /// again to a core-only board that answers, until the capture closes.
    fn page_double(port: u16, manifest: lpc_update::BoardManifest) -> (FakeBoard, FakeBoard) {
        let stream = connect_retrying(port);
        let (mut ws, _) =
            tungstenite::client(format!("ws://127.0.0.1:{port}/"), stream).expect("handshake");
        // After the handshake, which a read timeout would interrupt.
        ws.get_ref()
            .set_read_timeout(Some(Duration::from_millis(2)))
            .unwrap();
        let mut first = FakeBoard::new(
            FakeMode::Engine {
                password: Some("desk-lab"),
            },
            manifest.clone(),
        )
        .silent_on_update();
        let mut second = FakeBoard::new(FakeMode::CoreOnly, manifest);
        ws.send(Message::text("log 0.001 ble up (LP-test)"))
            .unwrap();
        ws.send(Message::text("up")).unwrap();
        let clock = Instant::now();
        let mut on_second = false;
        loop {
            let now = clock.elapsed().as_micros() as u64;
            let board = if on_second { &mut second } else { &mut first };
            match ws.read() {
                Ok(Message::Binary(frame)) => board.on_write(now, &frame),
                Ok(Message::Close(_)) => break,
                Ok(_) => {}
                Err(tungstenite::Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(_) => break,
            }
            for frame in board.poll(now) {
                if ws.send(Message::Binary(frame)).is_err() {
                    return (first, second);
                }
            }
            if !on_second && first.queries == 1 {
                // The board resets: the connection drops, the page
                // reconnects to the same device.
                ws.send(Message::text("down")).unwrap();
                ws.send(Message::text("up")).unwrap();
                on_second = true;
            }
            assert!(
                clock.elapsed() < Duration::from_secs(60),
                "the page double ran on"
            );
        }
        (first, second)
    }

    fn connect_retrying(port: u16) -> TcpStream {
        for _ in 0..500 {
            if let Ok(stream) = TcpStream::connect(("127.0.0.1", port)) {
                return stream;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("the capture never listened on {port}");
    }

    fn free_port() -> u16 {
        TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }
}
