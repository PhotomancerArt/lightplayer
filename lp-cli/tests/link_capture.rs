//! `lp-cli link capture` — the silicon half of `lp-cli validate`'s link host —
//! run against an emulated board's socket, which is the closest this suite
//! gets to a board.
//!
//! The board is `lp-cli emu run --link <addr>`: the shipped image with its
//! USB-Serial-JTAG port on a TCP socket, a client's connect being the port's
//! open (`attached-idle`). `link capture tcp://<addr>` then does what it does
//! on a real port: opens it without a reset, brings lp-link up, and writes
//! the decoded console until the sentinel. The claim is that the capture
//! holds the lines `boot-idle` / `boot-idle-flash` stop on, in the same form
//! the emulated host writes them. It has never run against silicon; the desk
//! sitting in `lp2025/2026-09-27-validate-driver-link-host/desk-recapture.md`
//! is its first.
//!
//! `#[ignore]`d: it needs a built `fw-esp32c6` ELF (`LP_EMU_BUILD_FW=1`, or
//! `LP_CI_IMAGES`), and `just test-emu-c6-cli` runs it.

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use lp_cli::commands::link::args::CaptureArgs;
use lp_cli::commands::link::capture::capture;
use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image};

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6-cli` runs it"]
fn a_capture_over_the_boards_socket_reaches_the_boot_idle_sentinel() {
    let Some(elf) = image() else { return };
    let addr = free_addr();
    let _board = Board(
        Command::new(env!("CARGO_BIN_EXE_lp-cli"))
            .args(["emu", "run", "--elf"])
            .arg(&elf)
            .args(["--link", &addr, "--timeout", "30s", "--strict-bus"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawning lp-cli emu run"),
    );
    wait_listening(&addr);

    let dir = tempfile::tempdir().unwrap();
    let console = dir.path().join("boot-idle.cap");
    capture(&CaptureArgs {
        target: format!("tcp://{addr}"),
        console: console.clone(),
        exit_on: Some("[stack] heartbeat: high-water".into()),
        seconds: 300,
        json_replies: true,
    })
    .expect("the capture reached the sentinel");

    let text = std::fs::read_to_string(&console).unwrap();
    for want in [
        "[link] up (session",
        "M!{\"id\":0,\"msg\":{\"hello\":{\"proto\":",
        "M!{\"id\":0,\"msg\":{\"heartbeat\":{",
        "[stack] heartbeat: high-water",
    ] {
        assert!(text.contains(want), "no `{want}` in:\n{text}");
    }
    assert!(
        !text.contains("replies are now packed"),
        "--json-replies did not ask for packing:\n{text}"
    );
}

/// The emulated board, killed when the test is done with it.
struct Board(Child);

impl Drop for Board {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn image() -> Option<PathBuf> {
    match fw_esp32c6_image(&FwImage::SHIPPED) {
        Ok(path) => Some(path),
        Err(reason) => {
            eprintln!("link_capture: skipped — {reason}");
            None
        }
    }
}

/// A loopback address nothing is listening on right now.
fn free_addr() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().to_string()
}

/// Wait for the board's socket without connecting to it: a connect IS the
/// port's open, so a probe would be a first client the capture is not.
fn wait_listening(addr: &str) {
    let deadline = Instant::now() + Duration::from_secs(120);
    while Instant::now() < deadline {
        if TcpListener::bind(addr).is_err() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("the emulated board never listened on {addr}");
}
