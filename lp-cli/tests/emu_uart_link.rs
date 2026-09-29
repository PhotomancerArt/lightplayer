//! The shipped classic (v3) image on its UART0 link, as lp-cli speaks to it:
//! lp-link both ends since wire proto 32 (plan
//! `lp2025/2026-09-28-2015-classic-uart-on-lp-link`, P3).
//!
//! The emulated classic runs in this process through lp-cli's own hosted
//! door (`lp_cli::commands::emu::link_host::V3Board` under an
//! `EmuLinkHost`, what `lp-cli emu run --chip esp32v3 --host-link` runs),
//! stepped in EMULATED time, with the product's host end on
//! `LinkConfig::uart()`. Its claims:
//!
//! - the board says hello first on the link session, answers a hello
//!   request, and takes a whole project upload with the loaded project then
//!   listed — no app errors, one session;
//! - a capture of what it wrote reads back through `lp-cli wire unpack`
//!   with nothing unreadable: the tools have nothing chip-specific in them;
//! - a Reboot request is a new link session under a **different** board
//!   nonce, and the host sees it as `PeerRestarted` (ruling DD28): the
//!   emulator restores the RNG with the rest of power-on, so the random word
//!   repeats and only the boot-count salt tells the two boots apart;
//! - a board nobody answers backs its SYNs off to one a second (ruling
//!   DD27), instead of ten; and one whose host went quiet mid-session gets
//!   there too, once its resend limit resets the link (printed: how long
//!   that takes is the Established state's, not the backoff's).
//!
//! It lives in lp-cli because nothing under `lp-emu/` may depend on lp-link
//! or a product crate (the MIT fence). `#[ignore]`d: it needs the shipped
//! `fw-esp32v3` ELF in `LP_EMU_ESP32V3_ELF` (`just test-emu-esp32v3-cli`
//! builds it and runs this). Numbers it prints are `lp-emu:esp32v3:t1`.

use std::future::Future;
use std::io::Write;
use std::pin::pin;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use lp_cli::commands::emu::link_host::{EmuLinkHost, EmuUsbBoard, V3Board};
use lp_emu_esp32v3::machine::{AppSource, BootMode, Esp32V3Builder};
use lp_emu_esp32v3::test_support;
use lpc_wire::lp_link::sniffer::{Direction, LinkSniffer, SniffEvent};
use lpc_wire::lp_link::{Link, LinkConfig, SelectiveRepeat};
use lpc_wire::{ClientMessage, ClientRequest};

/// Emulated microseconds the hello may take: the boot reaches the server
/// loop at ~0.12 s on a direct load.
const HELLO_BUDGET_US: u64 = 3_000_000;

/// The classic's boot-count salt (`fw_esp32_common::uart_link::uart_link_nonce`,
/// `SALT_MULTIPLIER`): two boots whose random words are equal differ by
/// exactly this.
const SALT_STEP: u32 = 0x9E37_79B1;

#[test]
#[ignore = "needs LP_EMU_ESP32V3_ELF; run through `just test-emu-esp32v3-cli`"]
fn the_classic_says_hello_and_takes_an_upload_over_lp_link() {
    let Some(board) = sniffed_board("the_classic_says_hello_and_takes_an_upload_over_lp_link")
    else {
        return;
    };
    let mut host = EmuLinkHost::new(board, 0x5E55_0332, true);
    host.answer_budget_s = 120.0;
    let mut app_errors = 0u32;
    {
        let mut client = lpa_client::LpClient::new(&mut host);
        match block_on(client.hello()) {
            Ok(hello) => {
                assert_eq!(hello.value.proto, lpc_wire::WIRE_PROTO_VERSION);
                assert_eq!(hello.value.build.package, "fw-esp32v3");
            }
            Err(error) => {
                eprintln!("emu_uart_link: hello failed: {error}");
                app_errors += 1;
            }
        }
        let files = project_files("projects/test/basic");
        if let Err(error) = block_on(client.replace_and_load_project("emu-uart-basic", &files)) {
            eprintln!("emu_uart_link: the upload failed: {error}");
            app_errors += 1;
        }
        match block_on(client.project_list_loaded()) {
            Ok(loaded) => assert!(
                loaded
                    .value
                    .iter()
                    .any(|project| project.path.as_str().contains("emu-uart-basic")),
                "the uploaded project is loaded: {:?}",
                loaded.value
            ),
            Err(error) => {
                eprintln!("emu_uart_link: listing loaded projects failed: {error}");
                app_errors += 1;
            }
        }
    }
    let c = host.counters();
    eprintln!(
        "lp-emu:esp32v3:t1, {:.1} s emulated: {app_errors} app errors, {} link errors; host \
         link {} frames out / {} in, {} resent, {} damaged, {} resets; notes {:?}",
        host.board_seconds(),
        host.link_errors,
        c.frames_tx,
        c.frames_rx,
        c.resends,
        c.damaged,
        c.resets.total,
        host.notes
    );
    assert_eq!(app_errors + host.link_errors, 0, "{}", tail(host.console()));
    assert_eq!(c.resets.total, 0, "one session, start to end");
    assert_eq!(c.payload_errors, 0);
    assert!(
        host.notes
            .iter()
            .any(|note| note.contains("replies packed")),
        "the classic packs its replies once asked: {:?}",
        host.notes
    );

    // The tools: the board's side of this very conversation, as a serial
    // capture would hold it, through `lp-cli wire unpack`.
    let capture = std::mem::take(&mut host.board.capture);
    // `LP_EMU_UART_CAPTURE=<file>` keeps it, for a reader or a later golden.
    if let Some(path) = std::env::var_os("LP_EMU_UART_CAPTURE") {
        std::fs::write(&path, &capture).expect("writing the capture");
    }
    let (stdout, stderr) = lp_cli_wire_unpack(&["--sizes"], &capture);
    let text = String::from_utf8_lossy(&stdout);
    let report = String::from_utf8_lossy(&stderr);
    eprintln!(
        "wire unpack --sizes: {}",
        report.lines().last().unwrap_or("")
    );
    assert!(text.contains("[INIT] fw-esp32v3 boot"), "boot text: {text}");
    assert!(
        text.lines()
            .any(|l| l.starts_with("M!{\"id\":0,") && l.contains("\"hello\"")),
        "the hello: {text}"
    );
    assert!(
        text.contains("emu-uart-basic"),
        "the upload's replies: {text}"
    );
    assert!(
        report.contains("unreadable 0"),
        "every message read: {report}"
    );
}

/// DD28: a Reboot request is a new link session under a different nonce.
#[test]
#[ignore = "needs LP_EMU_ESP32V3_ELF; run through `just test-emu-esp32v3-cli`"]
fn a_reboot_is_a_new_session_under_a_new_board_nonce() {
    let Some(board) = sniffed_board("a_reboot_is_a_new_session_under_a_new_board_nonce") else {
        return;
    };
    let mut host = EmuLinkHost::new(board, 0x5E55_0328, true);
    let hello = host.wait_for_line("\"hello\":{", HELLO_BUDGET_US).unwrap();
    assert!(hello.is_some(), "{}", tail(host.console()));
    host.send(&ClientMessage {
        id: 7,
        msg: ClientRequest::Reboot,
    })
    .unwrap();
    let deadline = host.board.micros() + 2 * HELLO_BUDGET_US;
    while host.board.micros() < deadline {
        host.step().unwrap();
        let hellos = host
            .console()
            .iter()
            .filter(|l| l.contains("\"hello\":{"))
            .count();
        if hellos >= 2 {
            break;
        }
    }
    let nonces = host.board.nonces.clone();
    eprintln!(
        "lp-emu:esp32v3:t1: board nonces by boot {:?} (software reboots: {})",
        nonces
            .iter()
            .map(|(at, n)| format!("{n:#010x} at {:.3} s", *at as f64 / 1e6))
            .collect::<Vec<_>>(),
        host.board.board.machine.reboots()
    );
    for line in host.console().iter().filter(|l| {
        l.contains("[link]") || l.contains("[RECOVERY] boot:") || l.contains("[REBOOT]")
    }) {
        eprintln!("  {line}");
    }
    assert_eq!(host.board.board.machine.reboots(), 1, "the board rebooted");
    assert_eq!(nonces.len(), 2, "one session per boot: {nonces:?}");
    let (first, second) = (nonces[0].1, nonces[1].1);
    assert_ne!(first, second, "the two boots' nonces differ");
    // The emulator puts the RNG back with the rest of power-on, so both
    // boots drew the same random word; the salt alone moved the nonce, by
    // one boot's step.
    assert_eq!(
        second.wrapping_sub(first),
        SALT_STEP,
        "the random word repeated and the boot count moved on by one"
    );
    assert!(
        host.console()
            .iter()
            .any(|l| l == "[link] reset (PeerRestarted)"),
        "the host saw the restart: {}",
        tail(host.console())
    );
    assert!(
        host.console()
            .iter()
            .any(|l| l.contains("[RECOVERY] boot: cause=software-reset")),
        "the second boot knew it was a software reset: {}",
        tail(host.console())
    );
}

/// DD27: a board nobody answers backs its SYNs off to one a second.
#[test]
#[ignore = "needs LP_EMU_ESP32V3_ELF; run through `just test-emu-esp32v3-cli`"]
fn a_classic_nobody_answers_backs_its_syns_off_to_one_a_second() {
    const WATCH_US: u64 = 12_000_000;
    let Some(mut board) =
        sniffed_board("a_classic_nobody_answers_backs_its_syns_off_to_one_a_second")
    else {
        return;
    };
    // A mute host: it reads what the board writes and never answers, so it
    // counts the board's SYNs (every one verifies with the handshake's key).
    let mut mute: Link<SelectiveRepeat> = Link::new(LinkConfig::uart(), 0x3171_0001);
    let mut syns = Vec::new();
    let mut text = LinkSniffer::usb();
    while board.micros() < WATCH_US {
        board.run_for_us(250).unwrap();
        let bytes = board.take_usb_output();
        let before = mute.counters().frames_rx;
        let now = board.micros();
        mute.on_bytes(now, &bytes);
        for _ in before..mute.counters().frames_rx {
            syns.push(now);
        }
        text.push(Direction::BoardToHost, now, &bytes, |event| {
            // The boot text, for a reader of the SYN times: io_task's settle
            // ends ~100 ms after its spawn line.
            if let SniffEvent::Text { text, .. } = event
                && now < 200_000
            {
                eprintln!(
                    "  {:.4} s: {}",
                    now as f64 / 1e6,
                    String::from_utf8_lossy(&text).trim_end()
                );
            }
        });
    }
    let first = syns[0];
    let gaps_ms: Vec<u64> = syns.windows(2).map(|w| (w[1] - w[0]) / 1_000).collect();
    let window: Vec<&u64> = syns.iter().filter(|&&t| t < first + 10_000_000).collect();
    eprintln!(
        "lp-emu:esp32v3:t1: {} SYNs in the 10 s after the first (at {:.3} s); gaps (ms) {:?}",
        window.len(),
        first as f64 / 1e6,
        gaps_ms
    );
    // The first SYN is queued on the link task's first pass (~22 ms) but held
    // in the TX pipe by io_task's 100 ms boot settle, so it reaches the wire
    // only ~10 ms before the second, which went out on schedule 100 ms after
    // it was queued. From there the gaps are the backoff's own.
    assert!(
        gaps_ms[0] <= 110,
        "the first two SYNs: 100 ms apart, less the settle: {gaps_ms:?}"
    );
    for (i, want) in [(1, 200), (2, 400), (3, 800)] {
        assert!(
            gaps_ms[i].abs_diff(want) <= 15,
            "gap {i} doubles to {want} ms: {gaps_ms:?}"
        );
    }
    assert!(
        gaps_ms[4..].iter().all(|&g| g.abs_diff(1_000) <= 15),
        "then one a second: {gaps_ms:?}"
    );
    assert!(
        window.len() <= 14,
        "at most 14 SYNs in ten seconds, not ~100: {}",
        window.len()
    );
}

/// DD27's other half, measured: a host that brought the link up and then
/// went away (a Studio tab closed) leaves the board Established, where it
/// keeps talking — keepalives, and its heartbeat resent — until its resend
/// limit resets the link; only then does the SYN backoff apply. This prints
/// the board's frames per second after the host goes quiet and holds the
/// tail to the backoff's one a second.
#[test]
#[ignore = "needs LP_EMU_ESP32V3_ELF; run through `just test-emu-esp32v3-cli`"]
fn after_its_host_goes_quiet_the_classic_falls_back_to_one_frame_a_second() {
    const WATCH_US: u64 = 60_000_000;
    let Some(board) =
        sniffed_board("after_its_host_goes_quiet_the_classic_falls_back_to_one_frame_a_second")
    else {
        return;
    };
    let mut host = EmuLinkHost::new(board, 0x5E55_0327, false);
    let hello = host.wait_for_line("\"hello\":{", HELLO_BUDGET_US).unwrap();
    assert!(hello.is_some(), "{}", tail(host.console()));
    // The host stops servicing its end: from here only the board runs, and
    // a mute reader counts every frame it writes.
    let board = &mut host.board;
    let quiet_from = board.micros();
    let mut mute: Link<SelectiveRepeat> = Link::new(LinkConfig::uart(), 0x3171_0002);
    let frames = |l: &Link<SelectiveRepeat>| {
        let c = l.counters();
        c.frames_rx + c.dropped_unsynced + c.bad_frames + c.stale_frames
    };
    let mut per_second = vec![0u32; (WATCH_US / 1_000_000) as usize];
    while board.micros() < quiet_from + WATCH_US {
        board.run_for_us(250).unwrap();
        let bytes = board.take_usb_output();
        let before = frames(&mute);
        mute.on_bytes(board.micros(), &bytes);
        let new = frames(&mute) - before;
        if new > 0 {
            let second = ((board.micros() - quiet_from) / 1_000_000) as usize;
            let last = per_second.len() - 1;
            per_second[second.min(last)] += new;
        }
    }
    eprintln!(
        "lp-emu:esp32v3:t1: board frames per second after the host went quiet at {:.3} s: {:?}",
        quiet_from as f64 / 1e6,
        per_second
    );
    let tail_rate = &per_second[per_second.len() - 10..];
    assert!(
        tail_rate.iter().all(|&n| n <= 2),
        "the last ten seconds are the backoff's one SYN a second: {per_second:?}"
    );
}

/// The classic's hosted board, with what it wrote kept (a serial capture)
/// and its link nonce read off every SYN that names a new one.
struct SniffedV3 {
    board: V3Board,
    sniffer: LinkSniffer,
    /// (emulated µs, the board's nonce) per new board session.
    nonces: Vec<(u64, u32)>,
    capture: Vec<u8>,
}

impl EmuUsbBoard for SniffedV3 {
    fn link_config(&self) -> LinkConfig {
        self.board.link_config()
    }

    fn link_name(&self) -> &'static str {
        self.board.link_name()
    }

    fn run_for_us(&mut self, us: u64) -> Result<(), String> {
        self.board.run_for_us(us)
    }

    fn micros(&self) -> u64 {
        self.board.micros()
    }

    fn take_usb_output(&mut self) -> Vec<u8> {
        let bytes = self.board.take_usb_output();
        let now = self.board.micros();
        let nonces = &mut self.nonces;
        self.sniffer
            .push(Direction::BoardToHost, now, &bytes, |event| {
                if let SniffEvent::Session {
                    dir: Direction::BoardToHost,
                    nonce,
                } = event
                {
                    nonces.push((now, nonce));
                }
            });
        self.capture.extend_from_slice(&bytes);
        bytes
    }

    fn push_usb_input(&mut self, bytes: &[u8]) {
        self.board.push_usb_input(bytes);
    }

    fn finish(&mut self) -> Vec<String> {
        self.board.finish()
    }
}

/// The shipped image, direct-loaded, under the hosted door's board; `None`
/// (with the skip notice) when no image was named.
fn sniffed_board(test: &str) -> Option<SniffedV3> {
    let elf = match test_support::fw_esp32v3_image() {
        Ok(elf) => elf,
        Err(reason) => {
            test_support::skip_notice(test, &reason);
            return None;
        }
    };
    let builder = Esp32V3Builder::new()
        .boot_mode(BootMode::Direct)
        .app(AppSource::Path(elf));
    Some(SniffedV3 {
        board: V3Board::build(builder).expect("the shipped image direct-loads"),
        sniffer: LinkSniffer::usb(),
        nonces: Vec::new(),
        capture: Vec::new(),
    })
}

/// The last lines of a console, for a failure message.
fn tail(console: &[String]) -> String {
    let from = console.len().saturating_sub(40);
    console[from..].join("\n")
}

/// A project directory as the upload's `(relative path, bytes)` list, in
/// path order (as `lp-cli upload` sends it).
fn project_files(relative: &str) -> Vec<(String, Vec<u8>)> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(relative);
    let mut files = Vec::new();
    let mut dirs = vec![root.clone()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).expect("the project directory") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                dirs.push(path);
            } else {
                let name = path
                    .strip_prefix(&root)
                    .expect("under the root")
                    .to_string_lossy()
                    .replace('\\', "/");
                files.push((name, std::fs::read(&path).expect("a file")));
            }
        }
    }
    files.sort();
    files
}

/// `lp-cli wire unpack <args>` over `input`: (stdout, stderr).
fn lp_cli_wire_unpack(args: &[&str], input: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_lp-cli"))
        .args(["wire", "unpack"])
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawning lp-cli wire unpack");
    let mut stdin = child.stdin.take().expect("piped");
    let input = input.to_vec();
    let writer = std::thread::spawn(move || stdin.write_all(&input));
    let output = child.wait_with_output().expect("lp-cli wire unpack ran");
    writer.join().unwrap().expect("writing its stdin");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    (output.stdout, output.stderr)
}

/// Drive a future whose every await completes synchronously (the host steps
/// the board inside `receive`): tests are edges, and a null waker is enough.
fn block_on<F: Future>(future: F) -> F::Output {
    struct Noop;
    impl Wake for Noop {
        fn wake(self: Arc<Self>) {}
    }
    let waker = Waker::from(Arc::new(Noop));
    let mut context = Context::from_waker(&waker);
    let mut future = pin!(future);
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
    }
}
