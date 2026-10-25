//! A slow-draining host cannot hold one C6 server pass past the watchdog
//! (`docs/defects/2026-10-09-the-editor-never-opens-on-an-emu-serve-usb-board.md`,
//! ticket `2026-10-20-send-room-wait-starves-watchdog`).
//!
//! A `ProjectRead` is one server pass that streams several reply frames, and
//! each frame waits for the link to finish reading the one before it out of
//! the one frame buffer. With the host reading at a few KB/s, those waits
//! used to add up: each frame waited up to `SEND_ROOM_WAIT` (3 s), the
//! heartbeats waited again, and nothing bounded the pass as a whole. The RTC
//! watchdog (8 s, fed once per pass) reset the board after 2 of the choker's
//! full read's 4 frames. Now every send wait in a pass ends at the pass's
//! send deadline (`TICK_WAIT_LIMIT_MS`, the radio links' rule).
//!
//! The run: the shipped image, JSON replies (as a host that never asks for
//! packed gets), the choker deployed, Studio's first sync, then the editor's
//! full read with the host reading the board's output at [`READ_RATE`] B/s.
//! Either a finished read or a drop notice is acceptable; a board reset (a
//! second boot banner, or the chip's own "asked to reset") is not. Numbers are `lp-emu:esp32c6:t1`.
//!
//! `#[ignore]`d: it needs a built `fw-esp32c6` ELF (`LP_EMU_BUILD_FW=1`), and
//! `just test-emu-c6-cli` runs it.

#[path = "support/editor_reads.rs"]
mod editor_reads;

use std::path::{Path, PathBuf};

use editor_reads::{EditorReads, block_on, deploy};
use lp_cli::commands::emu::link_host::{C6Board, EmuLinkHost};
use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::machine::{AppSource, Esp32C6Builder, TimeGrade, UsbHost};
use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image};
use lpa_client::LpClient;
use lpc_wire::{ClientMessage, ClientRequest, ProjectReadRequest, WireProjectHandle};

/// One fixed host nonce, so two runs are the same run.
const NONCE: u32 = 0xF4A6_C6E5;

/// How fast the host application reads the board's output, bytes per
/// emulated second. Slow enough that the full read's frames cannot all be
/// out inside one watchdog period (the choker's read is ~29 KB of JSON).
const READ_RATE: u64 = 5_500;

/// A slower host than [`READ_RATE`]'s: slow enough that the full read's
/// frame is dropped as `host link busy` while a second request waits.
const DROP_READ_RATE: u64 = 2_250;

/// The request ids of [`a_dropped_reply_does_not_leave_the_next_request_unanswered`].
const READ_ID: u64 = 900_001;
const SECOND_ID: u64 = 900_002;

/// The boot banner: a second one is a reset.
const BOOT_BANNER: &str = "[INIT] Initializing board...";

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6-cli` runs it"]
fn a_slow_host_does_not_reset_the_board_during_a_full_read() {
    let Some((mut host, handle)) = booted_choker() else {
        return;
    };

    let mut editor = EditorReads::new(&mut host, handle);
    editor.initial_sync();
    editor.idle(2_000_000);
    editor.host.set_read_rate(Some(READ_RATE));
    let started = editor.host.board_seconds();
    editor.read(
        "full",
        serde_json::json!({
            "since": null,
            "queries": [
                {"shapes": {"level": "detail"}},
                {"nodes": {"level": "detail", "nodes": "all", "include_slots": true}},
                {"runtime": null}
            ],
            "probes": [
                {"output_frame": {"geometry": "always", "samples": "srgb8"}},
                {"binding_graph": {"structure": "always", "include_values": true}}
            ]
        }),
    );
    let took = editor.host.board_seconds() - started;
    let read = editor.reads.last().cloned().expect("the read");
    editor.host.set_read_rate(None);
    drop(editor);
    // Past a heartbeat and the watchdog's period, so a pass that is still
    // wedged would have shown by now.
    let until = host.board.machine.micros() + 10_000_000;
    host.run_until(until, None).expect("the tail");

    let console = host.console().to_vec();
    let boots = console
        .iter()
        .filter(|line| line.contains(BOOT_BANNER))
        .count();
    let resets: Vec<_> = console
        .iter()
        .filter(|line| line.contains("LP_WDT") || line.contains("[RECOVERY] boot:"))
        .cloned()
        .collect();
    let report = format!(
        "lp-emu:esp32c6:t1 — the full read at {READ_RATE} B/s took {took:.3} s emulated, \
         ended={} error={:?}; {boots} boot(s); recovery lines {resets:?}",
        read.ended, read.error
    );
    println!("{report}");

    // The emulated chip stops at a reset and says who asked, which the
    // read's error carries; a console with a second banner is the other way
    // a reset shows.
    let reset_error = read
        .error
        .as_deref()
        .is_some_and(|error| error.contains("chip asked to reset"));
    assert!(!reset_error, "the watchdog reset the board: {report}");
    assert_eq!(boots, 1, "the board reset: {report}");
    assert!(
        read.ended || read.error.is_some(),
        "the read neither finished nor was told it was dropped: {report}"
    );
}

/// A busy drop of one reply ends only that reply: a second request in the same
/// pass still gets a response (its answer, or a drop notice if the stalled
/// frame is still in the way), and the board neither stops answering nor resets
/// (ticket `2026-10-20-busy-frame-aborts-tick`).
#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6-cli` runs it"]
fn a_dropped_reply_does_not_leave_the_next_request_unanswered() {
    let Some((mut host, handle)) = booted_choker() else {
        return;
    };
    {
        let mut editor = EditorReads::new(&mut host, handle);
        editor.initial_sync();
        editor.idle(2_000_000);
    }
    let full: ProjectReadRequest = serde_json::from_value(serde_json::json!({
        "since": null,
        "queries": [
            {"shapes": {"level": "detail"}},
            {"nodes": {"level": "detail", "nodes": "all", "include_slots": true}},
            {"runtime": null}
        ],
        "probes": [
            {"output_frame": {"geometry": "always", "samples": "srgb8"}},
            {"binding_graph": {"structure": "always", "include_values": true}}
        ]
    }))
    .expect("the read request");
    let seen = host.console().len();
    host.set_read_rate(Some(DROP_READ_RATE));
    // Both requests arrive in one pass: the read, then a small one.
    host.send(&ClientMessage {
        id: READ_ID,
        msg: ClientRequest::ProjectRead {
            handle,
            request: full,
        },
    })
    .expect("send the read");
    host.send(&ClientMessage {
        id: SECOND_ID,
        msg: ClientRequest::ListLoadedProjects,
    })
    .expect("send the second request");
    let until = host.board.machine.micros() + 25_000_000;
    host.run_until(until, None).expect("the run");
    host.set_read_rate(None);
    let until = host.board.machine.micros() + 3_000_000;
    host.run_until(until, None).expect("the tail");

    let console = &host.console()[seen..];
    let answered = console
        .iter()
        .any(|line| line.starts_with("M!") && line.contains(&format!("\"id\":{SECOND_ID}")));
    let boots = console
        .iter()
        .filter(|line| line.contains(BOOT_BANNER))
        .count();
    println!(
        "lp-emu:esp32c6:t1 — second request answered={answered}, {boots} new boot(s) at \
         {DROP_READ_RATE} B/s"
    );
    assert!(answered, "the second request got no response at all");
    assert_eq!(boots, 0, "the board reset");
}

/// The shipped image booted on a blank flash with the choker deployed and
/// running, behind a host that asks for JSON replies. `None` when the image
/// is not built.
fn booted_choker() -> Option<(EmuLinkHost<C6Board>, WireProjectHandle)> {
    let elf = match fw_esp32c6_image(&FwImage::SHIPPED) {
        Ok(path) => path,
        Err(reason) => {
            eprintln!("emu_slow_host_read: skipped — {reason}");
            return None;
        }
    };
    let machine = Esp32C6Builder::new()
        .app(AppSource::Path(elf))
        .flash(FlashBacking::Blank)
        .strict(false)
        .time_grade(TimeGrade::T1)
        .usb_host(UsbHost::Attached { draining: true })
        .usb_sj_queue_source()
        .build()
        .expect("the machine");
    // JSON replies: the host never asks the board to pack them.
    let mut host = EmuLinkHost::new(C6Board::new(machine).expect("the board"), NONCE, false);
    let hello = host
        .wait_for_line("\"hello\":{", 3_000_000)
        .expect("the boot");
    assert!(hello.is_some(), "no hello:\n{}", host.console().join("\n"));

    deploy(
        &mut host,
        &repo_root().join("catalog/projects/playful-choker"),
    );
    let loaded = {
        let mut client = LpClient::new(&mut host).with_request_ids_from(1_000);
        block_on(client.project_list_loaded()).expect("the loaded projects")
    };
    let handle = loaded
        .value
        .first()
        .expect("the deploy loaded the choker")
        .handle;
    Some((host, handle))
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lp-cli has a parent")
        .to_path_buf()
}
