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
//! The second test is `--request`: a `reboot` the board answers and restarts
//! on, and a `hello` the capture holds back until the rebooted board has said
//! its own.
//!
//! `#[ignore]`d: it needs a built `fw-esp32c6` ELF (`LP_EMU_BUILD_FW=1`, or
//! `LP_CI_IMAGES`), and `just test-emu-c6-cli` runs it.

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use lp_cli::commands::link::args::CaptureArgs;
use lp_cli::commands::link::capture::capture;
use lp_cli::commands::link::capture_requests::REQUEST_ID_BASE;
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
        request: Vec::new(),
        ota_offer: None,
        password: None,
        ota_ahead: None,
        ble_window: 32,
        ota_ticket_file: None,
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

/// `--request reboot --request hello`: the board answers the reboot, resets,
/// comes back under a new session nonce (the host sees `PeerRestarted`), and
/// the hello goes to the REBOOTED board and is answered there. The desk's
/// way to reboot a board with nothing else on its port.
///
/// ⚠️ On this machine the restart is the LP watchdog's, not the software
/// reset's: the emulated C6 stores the ROM's `LP_AON.sys_cfg.hpsys_sw_reset`
/// write without acting on it, `esp_hal::system::software_reset` returns into
/// the next function, and the RWDT reboots the chip seconds later
/// (`docs/defects/2026-09-29-the-emulated-c6-does-not-perform-a-software-
/// reset.md`). So no `--strict-bus` here — the fall-through writes to
/// address 0 — and what this test proves is the capture's side: the request
/// order, the restart seen as `PeerRestarted`, and the next request asked of
/// the new session. How fast a board restarts is not in it.
#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6-cli` runs it"]
fn a_reboot_request_restarts_the_board_and_the_next_request_goes_to_the_new_session() {
    let Some(elf) = image() else { return };
    let addr = free_addr();
    let _board = Board(
        Command::new(env!("CARGO_BIN_EXE_lp-cli"))
            .args(["emu", "run", "--elf"])
            .arg(&elf)
            .args(["--link", &addr, "--reboot-on-reset", "--timeout", "60s"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawning lp-cli emu run"),
    );
    wait_listening(&addr);

    let dir = tempfile::tempdir().unwrap();
    let console = dir.path().join("reboot.cap");
    // The answer to the SECOND request, id REQUEST_ID_BASE + 1.
    let second_answer = format!("M!{{\"id\":{},", REQUEST_ID_BASE + 1);
    capture(&CaptureArgs {
        target: format!("tcp://{addr}"),
        console: console.clone(),
        exit_on: Some(second_answer.clone()),
        seconds: 300,
        json_replies: true,
        request: vec!["reboot".into(), "hello".into()],
        ota_offer: None,
        password: None,
        ota_ahead: None,
        ble_window: 32,
        ota_ticket_file: None,
    })
    .expect("the capture sent both requests and the rebooted board answered the second");

    let text = std::fs::read_to_string(&console).unwrap();
    eprintln!(
        "link_capture: the reboot capture —\n{}",
        reboot_excerpt(&text)
    );
    let at = |needle: &str| {
        text.find(needle)
            .unwrap_or_else(|| panic!("no `{needle}` in:\n{text}"))
    };
    let first_up = at("[link] up (session");
    let restarted = at("[link] reset (PeerRestarted)");
    let answer = at(&second_answer);
    assert!(
        first_up < restarted && restarted < answer,
        "out of order:\n{text}"
    );
    let boot_hello = "M!{\"id\":0,\"msg\":{\"hello\"";
    let after = &text[restarted..];
    assert!(
        after.contains("[link] up (session") && after.contains(boot_hello),
        "no new session and hello after the reset:\n{text}"
    );
    assert_eq!(
        text.matches(boot_hello).count(),
        2,
        "one boot hello per boot:\n{text}"
    );
}

/// The capture's link lines and the lines that name the two requests' ids,
/// for the test's own output.
fn reboot_excerpt(text: &str) -> String {
    let first = format!("\"id\":{},", REQUEST_ID_BASE);
    let second = format!("\"id\":{},", REQUEST_ID_BASE + 1);
    text.lines()
        .filter(|l| {
            l.starts_with("[link]")
                || l.starts_with("M!{\"id\":0,\"msg\":{\"hello\"")
                || l.contains(&first)
                || l.contains(&second)
                || l.contains("[INIT]")
        })
        .map(|l| {
            let mut l = l.to_string();
            if l.len() > 160 {
                l.truncate(160);
                l.push('…');
            }
            format!("  {l}")
        })
        .collect::<Vec<_>>()
        .join("\n")
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
