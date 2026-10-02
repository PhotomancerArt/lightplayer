//! A Bluetooth-on C6 keeps serving Studio's reads while shader edits
//! fragment its heap (`docs/defects/2026-09-27-fragmented-heap-refuses-every-read.md`).
//!
//! Each shader edit leaves a few 2–4 KB objects (the new compiled code, a
//! copy of the shader's settings list, lookup tables) in the middle of the
//! heap's big free tail. With Bluetooth on, the controller holds the second
//! heap region, so after a handful of edits the largest free block is
//! ~19.5 KB while ~90 KB is still free. The read gate used to demand a
//! 32 KiB block, and refused every read from there on — on the emulator 52
//! of 96, on the prod choker all of them. No read asks for more than 8 KB
//! at once, and reads keep nothing.
//!
//! The run: the shipped image with no device store (so Bluetooth is on, as
//! on a fresh board), `catalog/projects/playful-choker` uploaded over the
//! link, Studio's staged first sync, then rounds of the editor lens across
//! its four focuses plus the device card's feed, at Studio's cadence, with
//! [`SHADER_EDITS`] shader rewrites (each a recompile, each a line longer
//! than the last) from a fifth of the way in, and the shader restored at the
//! end (plan `lp2025/2026-09-27-1218-fragmentation-tolerant-reads`, P0; the
//! sequence is the research's `frag-drive`). The heap ends at a ~19.5 KB
//! largest block with ~86 KB free.
//!
//! Asserts: every read answered with `End`, none refused, and the board
//! never reset. On a failure it prints the heartbeat memory series, which is
//! the diagnosis; `LP_EMU_FRAG_CONSOLE=<file>` writes the board's whole
//! console there. Numbers printed are `lp-emu:esp32c6:t1`.
//!
//! It catches the bug: with the C6 put back on the old gate (a 32 KiB block
//! only), 15 of its 131 reads are refused (2026-09-28).
//!
//! `#[ignore]`d: it needs a built `fw-esp32c6` ELF (`LP_EMU_BUILD_FW=1`), and
//! `just test-emu-c6-cli` runs it.

#[path = "support/editor_reads.rs"]
mod editor_reads;

use std::path::{Path, PathBuf};

use editor_reads::{EditorReads, LENS_GAP_US, block_on, deploy, heartbeats};
use lp_cli::commands::emu::link_host::{C6Board, EmuLinkHost};
use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::machine::{AppSource, Esp32C6Builder, TimeGrade, UsbHost};
use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image};
use lpa_client::LpClient;

/// One fixed host nonce, so two runs are the same run.
const NONCE: u32 = 0xF4A6_C6E5;

/// Lens rounds: each is the four focuses and a card-feed read.
const ROUNDS: u32 = 25;

/// Shader rewrites, one per round from round `ROUNDS - SHADER_EDITS` on.
///
/// Twenty, not the research's ten: on the image the research measured
/// (`4caa5b658`) ten comment edits took the largest block to ~25 KB, but
/// on `main` after #858 and #860 (a request's bytes and a finished
/// reassembly no longer leave holes) ten edits leave the catalog choker at
/// 36–40 KB, where the old gate still passes. Twenty growing edits end at
/// ~19.5 KB — the prod choker's own refusals were at 19,478–19,480 B.
const SHADER_EDITS: u32 = 20;

/// The boot banner: a second one is a reset.
const BOOT_BANNER: &str = "[INIT] Initializing board...";

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6-cli` runs it"]
fn a_bluetooth_on_c6_serves_every_read_through_twenty_shader_edits() {
    let Some(elf) = image(&FwImage::SHIPPED) else {
        return;
    };
    let mut host = hosted(&elf);
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
    let project = loaded
        .value
        .first()
        .cloned()
        .expect("the deploy loaded the choker");
    let shader = format!("{}/shader.glsl", project.path.as_str());

    let mut editor = EditorReads::new(&mut host, project.handle);
    editor.initial_sync();
    let original = editor.fs_read(&shader);
    let first_edit = ROUNDS - SHADER_EDITS;
    for round in 0..ROUNDS {
        for focus in ["none", "render", "control", "timebase"] {
            editor.lens(focus);
            editor.idle(LENS_GAP_US);
        }
        editor.card_feed();
        if round >= first_edit {
            editor.fs_write(&shader, edited_shader(&original, round - first_edit + 1));
        }
    }
    editor.fs_write(&shader, original);
    for focus in ["none", "render"] {
        editor.idle(500_000);
        editor.lens(focus);
    }
    let focused = (
        editor.render_node,
        editor.control_node,
        editor.timebase_node,
    );
    let reads = std::mem::take(&mut editor.reads);
    drop(editor);
    // Past the next heartbeat, so the series ends after the last edit.
    let until = host.board.machine.micros() + 10_500_000;
    host.run_until(until, None).expect("the tail");

    let console = host.console().to_vec();
    let beats = heartbeats(&console);
    let series = beats
        .iter()
        .map(|beat| {
            format!(
                "  uptime {:>7} ms  free {:>6} B  largest block {}",
                beat.uptime_ms,
                beat.free_bytes,
                beat.largest_free_block
                    .map_or_else(|| "?".to_string(), |bytes| format!("{bytes} B"))
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let refused: Vec<_> = reads.iter().filter(|read| read.error.is_some()).collect();
    let unended: Vec<_> = reads.iter().filter(|read| !read.ended).collect();
    let boots = console
        .iter()
        .filter(|line| line.contains(BOOT_BANNER))
        .count();
    let counters = host.counters();
    let report = format!(
        "lp-emu:esp32c6:t1 — {} reads, {} refused or failed, {} without End; {boots} boot(s); \
         host link {} resets, {} payload errors, {} app errors; focused nodes (render, \
         control, timebase) = {focused:?}\nheartbeats:\n{series}\nrefused:\n{}",
        reads.len(),
        refused.len(),
        unended.len(),
        counters.resets.total,
        counters.payload_errors,
        host.link_errors,
        refused
            .iter()
            .map(|read| format!(
                "  #{} {} at {:.3} s: {}",
                read.index,
                read.label,
                read.at_s,
                read.error.as_deref().unwrap_or("")
            ))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    println!("{report}");

    assert!(
        focused.0.is_some() && focused.1.is_some() && focused.2.is_some(),
        "the skeleton did not name the shader, fixture and clock nodes: {report}"
    );
    assert_eq!(boots, 1, "the board reset: {report}");
    assert_eq!(counters.resets.total, 0, "the link reset: {report}");
    assert_eq!(host.link_errors, 0, "{report}");
    assert!(refused.is_empty(), "reads were refused: {report}");
    assert!(unended.is_empty(), "reads did not end: {report}");
    let expected = 4 + ROUNDS * 5 + 2;
    assert_eq!(reads.len() as u32, expected, "{report}");
}

/// The shader after `edits` edits: each adds one more line of work to the
/// brightness, so every recompile's code is a little bigger than the last —
/// as it is when someone is writing a shader, and unlike a comment, which
/// compiles to the same code and lands in the hole the last one left.
fn edited_shader(original: &[u8], edits: u32) -> Vec<u8> {
    let source = std::str::from_utf8(original).expect("the shader is UTF-8");
    let at = source.find(EDIT_ANCHOR).unwrap_or_else(|| {
        panic!("the choker's shader has no {EDIT_ANCHOR:?} line to edit before")
    });
    let mut edited = String::from(&source[..at]);
    for edit in 1..=edits {
        edited.push_str(&format!(
            "    lum *= 1.0 + 0.01 * sin(p.x * {edit}.0 + time);\n"
        ));
    }
    edited.push_str(&source[at..]);
    edited.into_bytes()
}

/// The line in the choker's shader the edits go before.
const EDIT_ANCHOR: &str = "    vec3 color = texture(palette";

/// The shipped image, attached and draining from power-on, with a blank
/// flash: no device store, so Bluetooth is on (the controller and host hold
/// ~23 KB of the second heap region, which is what caps its largest block).
fn hosted(elf: &Path) -> EmuLinkHost<C6Board> {
    let machine = Esp32C6Builder::new()
        .app(AppSource::Path(elf.to_path_buf()))
        .flash(FlashBacking::Blank)
        // Not strict: this test is about the heap, not the bus, and a strict
        // bus runs it about five times slower (~64 s against ~5 min for 15
        // rounds, the same heap figures to the byte). The link gates run the
        // same image strict.
        .strict(false)
        .time_grade(TimeGrade::T1)
        .usb_host(UsbHost::Attached { draining: true })
        .usb_sj_queue_source()
        .build()
        .expect("the shipped image builds a machine");
    let host = EmuLinkHost::new(C6Board::new(machine).expect("a hosted board"), NONCE, true);
    // `LP_EMU_FRAG_CONSOLE=<file>`: the board's whole console, for a
    // diagnosis the heartbeat series does not settle.
    match std::env::var_os("LP_EMU_FRAG_CONSOLE") {
        Some(path) => host.with_console_sink(Box::new(
            std::fs::File::create(path).expect("the console file"),
        )),
        None => host,
    }
}

fn image(image: &FwImage) -> Option<PathBuf> {
    match fw_esp32c6_image(image) {
        Ok(path) => Some(path),
        Err(reason) => {
            eprintln!("emu_frag_reads: skipped — {reason}");
            None
        }
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lp-cli sits under the repo root")
        .to_path_buf()
}
