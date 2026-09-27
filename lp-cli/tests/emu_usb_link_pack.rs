//! JSON Pack on the emulated C6's USB link, since the link is an lp-link
//! (plan `lp2025/2026-09-27-0215-lp-link-usb-cutover`, D4). It replaces
//! `emu_usb_json_pack.rs`, whose `M!` lines, `0x00 'L'` frames, re-asks and
//! desync path the cut-over deleted.
//!
//! What must hold now, on the shipped image:
//!
//! 1. **in process**: every session starts JSON. The board's hello is JSON;
//!    the port's own opt-in (this build's format) turns the replies after it
//!    into learned packed payloads that decode to exactly the JSON the typed
//!    message serializes to; an opt-in naming a format this build does not
//!    speak is answered and puts the board back on JSON; and a link restart
//!    (a new session) starts JSON again, with a new hello and a new opt-in.
//! 2. **through the `emu serve` door**, as a host on a socket does: packed
//!    replies arrive, and `lp-cli wire unpack` turns both the client's raw
//!    capture and the door's wire tap back into `M!{json}` lines.
//! 3. **a torn frame** (the door's `LP_EMU_WIRE_TEAR`, the loss the real C6
//!    showed): the link counts it and resends, and every reply still arrives
//!    whole — no desync, no reset, nothing the app sees.
//!
//! In lp-cli because the link host is a product crate (the MIT fence).
//! `#[ignore]`d: it needs a built `fw-esp32c6` ELF (`LP_EMU_BUILD_FW=1`);
//! `just test-emu-c6-cli` runs it. Figures printed are `lp-emu:esp32c6:t1`.

mod support;

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use lp_cli::commands::emu::link_host::{C6Board, EmuLinkHost};
use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::machine::{AppSource, Esp32C6Builder, TimeGrade, UsbHost};
use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image};
use lpc_wire::{ClientMessage, ClientRequest, PACK_FORMAT_VERSION, WireEncoding};
use support::Serve;
use support::door_link::{DoorLink, DoorRead};

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6-cli` runs it"]
fn every_session_starts_json_and_packs_only_after_this_builds_opt_in() {
    let Some(elf) = image() else { return };
    let machine = Esp32C6Builder::new()
        .app(AppSource::Path(elf))
        .flash(FlashBacking::Blank)
        .strict(true)
        .time_grade(TimeGrade::T1)
        .usb_host(UsbHost::Attached { draining: true })
        .usb_sj_queue_source()
        .build()
        .expect("the shipped image builds a machine");
    let mut host = EmuLinkHost::new(C6Board::new(machine).unwrap(), 0x0A0C_C601, true);

    // 1. The session's first message is the board's hello, as JSON, and the
    //    port's opt-in follows it.
    host.wait_for_line("replies packed", 3_000_000).unwrap();
    let first = host.messages[0];
    assert_eq!(first.id, 0, "the hello comes first: {:?}", host.messages);
    assert!(!first.packed, "the hello is JSON");
    assert!(
        host.notes.iter().any(|n| n.contains("replies packed")),
        "{:?}",
        host.notes
    );

    // 2. Replies after the port's opt-in are packed, and decode to exactly
    //    the JSON the typed message serializes to.
    for (id, msg) in [
        (6, ClientRequest::Hello),
        (7, ClientRequest::ListLoadedProjects),
    ] {
        host.send(&ClientMessage { id, msg }).unwrap();
        let line = host
            .wait_for_line(&format!("M!{{\"id\":{id},"), 2_000_000)
            .unwrap()
            .unwrap_or_else(|| panic!("no reply {id}"));
        let reply = host.messages.iter().rev().find(|m| m.id == id).unwrap();
        assert!(reply.packed, "reply {id} came as JSON: {line}");
        let json = line.strip_prefix("M!").unwrap();
        let message: lpc_wire::WireServerMessage = lpc_wire::json::from_str(json).unwrap();
        assert_eq!(lpc_wire::json::to_string(&message).unwrap(), json);
        println!(
            "reply {id}: {} B packed on the link for {} B of JSON",
            reply.wire_len, reply.json_len
        );
    }

    // 3. An opt-in for a format this build does not speak is answered, and
    //    the board goes back to JSON rather than guess: the next reply is
    //    JSON (the port decodes either, by the payload's first byte).
    const WRONG: u64 = 5;
    host.send(&ClientMessage {
        id: WRONG,
        msg: ClientRequest::SetEncoding {
            encoding: WireEncoding::Packed,
            format: PACK_FORMAT_VERSION + 1,
        },
    })
    .unwrap();
    let answer = host
        .wait_for_line(&format!("M!{{\"id\":{WRONG},"), 2_000_000)
        .unwrap()
        .expect("the wrong-format opt-in is answered");
    assert!(answer.contains("json"), "{answer}");
    host.send(&ClientMessage {
        id: 8,
        msg: ClientRequest::Hello,
    })
    .unwrap();
    host.wait_for_line("M!{\"id\":8,", 2_000_000)
        .unwrap()
        .expect("a reply after the wrong-format opt-in");
    assert!(
        !host
            .messages
            .iter()
            .rev()
            .find(|m| m.id == 8)
            .unwrap()
            .packed,
        "a reply after a wrong-format opt-in came packed"
    );

    // 4. A new session starts JSON again: a new hello, unpacked, then the
    //    port's opt-in again, then packed.
    let before = host.messages.len();
    let notes_before = host.notes.len();
    let now = host.now_us();
    host.port.restart(now);
    host.wait_for_line("[link] up (session 1)", 3_000_000)
        .unwrap()
        .expect("the link came back up");
    let deadline = host.board.machine.micros() + 3_000_000;
    while host.notes[notes_before..]
        .iter()
        .all(|n| !n.contains("replies packed"))
    {
        assert!(host.board.machine.micros() < deadline, "no second opt-in");
        host.step().unwrap();
    }
    let hello = host.messages[before];
    assert_eq!(hello.id, 0, "the new session's first message is the hello");
    assert!(!hello.packed, "a new session starts JSON");
    host.send(&ClientMessage {
        id: 9,
        msg: ClientRequest::ListLoadedProjects,
    })
    .unwrap();
    host.wait_for_line("M!{\"id\":9,", 2_000_000)
        .unwrap()
        .expect("a reply in the new session");
    assert!(
        host.messages
            .iter()
            .rev()
            .find(|m| m.id == 9)
            .unwrap()
            .packed,
        "packed again after the new session's opt-in"
    );
    let counters = host.counters();
    assert_eq!(counters.payload_errors, 0);
    assert_eq!(
        counters.resets.requested, 1,
        "the one restart this test asked for"
    );
}

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6-cli` runs it"]
fn a_client_through_the_door_gets_packed_replies_and_unpack_restores_json() {
    let Some(elf) = image() else { return };
    let tap_dir = tempfile::tempdir().expect("a temp dir");
    let serve = Serve::start_specs_with_env(
        &[format!("c6-a={}", elf.display())],
        &[],
        support::scratch(),
        &[("LP_EMU_WIRE_TAP", tap_dir.path())],
    );
    let mut link = DoorLink::new(serve.bytes("c6-a"), true);
    link.pump_until("the port's opt-in", |reads| {
        reads
            .iter()
            .any(|r| matches!(r, DoorRead::Link(n) if n.contains("replies packed")))
    });
    link.send(&ClientMessage {
        id: 3,
        msg: ClientRequest::Hello,
    });
    link.send(&ClientMessage {
        id: 4,
        msg: ClientRequest::ListLoadedProjects,
    });
    link.pump_until("replies 3 and 4", |reads| {
        [3, 4].iter().all(|id| {
            reads
                .iter()
                .any(|r| matches!(r, DoorRead::Message { message, .. } if message.id == *id))
        })
    });
    let raw = std::mem::take(&mut link.raw);
    let received: Vec<(String, bool)> = link
        .reads
        .iter()
        .filter_map(|r| match r {
            DoorRead::Message { json, packed, .. } => Some((json.clone(), *packed)),
            _ => None,
        })
        .collect();
    drop(link);
    serve.shutdown();

    for id in [3, 4] {
        let (json, packed) = received
            .iter()
            .find(|(json, _)| json.starts_with(&format!("{{\"id\":{id},")))
            .unwrap_or_else(|| panic!("no reply {id}: {received:?}"));
        assert!(*packed, "reply {id} came as JSON");
        let message: lpc_wire::WireServerMessage = lpc_wire::json::from_str(json).unwrap();
        assert_eq!(&lpc_wire::json::to_string(&message).unwrap(), json);
    }
    let packed_seen = received.iter().filter(|(_, p)| *p).count();

    // `wire unpack --sizes` over the client's capture: every message the
    // client read, as its `M!` line, and the link's own damage at zero.
    let (stdout, stderr) = lp_cli_wire_unpack(&["--sizes"], &raw);
    let text = String::from_utf8_lossy(&stdout);
    for (json, _) in &received {
        assert!(text.contains(&format!("M!{json}\n")), "missing M!{json}");
    }
    let stderr = String::from_utf8(stderr).unwrap();
    let total = stderr
        .lines()
        .find(|l| l.starts_with("total messages "))
        .unwrap_or_else(|| panic!("no total line:\n{stderr}"));
    assert!(total.contains(" damaged 0 gaps 0 "), "{total}");
    let packed_total: usize = total
        .split(" packed ")
        .nth(1)
        .and_then(|s| s.split(' ').next())
        .and_then(|s| s.parse().ok())
        .expect("a packed count");
    assert!(
        packed_total >= packed_seen,
        "{packed_total} packed in the capture, {packed_seen} read: {total}"
    );

    // The door's tap annotated each board message (`P`); after `wire unpack
    // --tap` it reads like a tap of a link with no framing.
    let tap = std::fs::read(tap_dir.path().join("c6-a.tap")).expect("the tap was written");
    let annotations = tap_headers(&tap)
        .iter()
        .filter(|h| h.split(' ').nth(1) == Some("P"))
        .count();
    assert!(
        annotations >= received.len(),
        "{annotations} P records for {} messages read",
        received.len()
    );
    let (unpacked_tap, _stderr) = lp_cli_wire_unpack(&["--tap"], &tap);
    assert!(
        tap_headers(&unpacked_tap)
            .iter()
            .all(|h| matches!(h.split(' ').nth(1), Some("<" | ">"))),
        "the annotations are dropped"
    );
    let unpacked_tap = String::from_utf8_lossy(&unpacked_tap);
    let (list_json, _) = received
        .iter()
        .find(|(json, _)| json.starts_with("{\"id\":4,"))
        .unwrap();
    assert!(unpacked_tap.contains(&format!("M!{list_json}\n")));
    println!(
        "door (lp-emu:esp32c6:t1): {} B read, {} messages, {packed_seen} packed; {total}",
        raw.len(),
        received.len()
    );
}

/// A frame torn in flight — the door's `LP_EMU_WIRE_TEAR`, the loss the
/// real C6 link showed — is a damaged frame the link resends. Before proto
/// 30 it desynced the packed reader until the board reset its table; now
/// the reader never sees it.
#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6-cli` runs it"]
fn a_torn_frame_is_resent_and_no_reply_is_lost() {
    let Some(elf) = image() else { return };
    // Tear board→host frames 12 through 20 by five bytes each: well past the
    // handshake, inside the packed replies.
    let serve = Serve::start_specs_with_env(
        &[format!("c6-a={}", elf.display())],
        &[],
        support::scratch(),
        &[(
            "LP_EMU_WIRE_TEAR",
            std::path::Path::new("12,13,14,15,16,17,18,19,20"),
        )],
    );
    let mut link = DoorLink::new(serve.bytes("c6-a"), true);
    link.pump_until("the port's opt-in", |reads| {
        reads
            .iter()
            .any(|r| matches!(r, DoorRead::Link(n) if n.contains("replies packed")))
    });
    const REQUESTS: u64 = 12;
    for id in 10..10 + REQUESTS {
        link.send(&ClientMessage {
            id,
            msg: ClientRequest::ListLoadedProjects,
        });
        link.pump_until(&format!("reply {id}"), |reads| {
            reads
                .iter()
                .any(|r| matches!(r, DoorRead::Message { message, .. } if message.id == id))
        });
    }
    let counters = link.port.counters();
    let resets: Vec<String> = link
        .reads
        .iter()
        .filter_map(|r| match r {
            DoorRead::Link(n) if n.starts_with("reset") => Some(n.clone()),
            _ => None,
        })
        .collect();
    drop(link);
    serve.shutdown();
    println!(
        "tear (lp-emu:esp32c6:t1): {} damaged, {} resent, {} resets, {} payload errors",
        counters.damaged, counters.resends, counters.resets.total, counters.payload_errors
    );
    assert!(counters.damaged > 0, "the tear was noticed: {counters:?}");
    assert_eq!(counters.payload_errors, 0);
    assert!(
        resets.is_empty(),
        "a tear is resent, not a reset: {resets:?}"
    );
}

fn image() -> Option<PathBuf> {
    match fw_esp32c6_image(&FwImage::SHIPPED) {
        Ok(path) => Some(path),
        Err(reason) => {
            eprintln!("emu_usb_link_pack: skipped — {reason}");
            None
        }
    }
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

/// Every record header of a wire tap, in order.
fn tap_headers(tap: &[u8]) -> Vec<String> {
    let mut headers = Vec::new();
    let mut at = 0;
    while at < tap.len() {
        let nl = at
            + tap[at..]
                .iter()
                .position(|&b| b == b'\n')
                .expect("a header");
        let header = String::from_utf8(tap[at..nl].to_vec()).expect("a text header");
        let len: usize = header.split(' ').nth(2).expect("a length").parse().unwrap();
        headers.push(header);
        at = nl + 1 + len + 1;
    }
    headers
}
