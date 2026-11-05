//! The choker tryout's first sync still reaches Studio after its playlist
//! has stepped once (`docs/defects/2026-10-31-a-big-playlist-loses-the-panel-after-its-first-step.md`).
//!
//! The tryout's playlist (25 entries) is a ~16 KB slot root. When the
//! playlist steps to its next pattern the board re-reads its def, and every
//! revision stamp in it goes from `0` to the current revision — a few digits
//! each, a few hundred stamps — so the root outgrew the 16 KiB read frame,
//! and every read carrying it ended in "project-read event exceeded frame
//! budget". Studio's first sync lost every slot root of that read, and the
//! card's panel drew without its controls (the master brightness among them)
//! until the board was reset. On `da1b5c5b6` a first sync at 60 s passed (the
//! stamps still three digits) and one at 120 s failed — on a heap with a
//! 65 KB largest block, so fragmentation had no part in it.
//!
//! The run: the shipped image with no device store (Bluetooth on, as on a
//! fresh board), `catalog/projects/playful-choker-tryout` uploaded over the
//! link, the playlist left to play two minutes, then Studio's staged first
//! sync, and again after one more step. Asserts every read ended with `End` and the board never reset.
//! Numbers printed are `lp-emu:esp32c6:t1`.
//!
//! `#[ignore]`d: it needs a built `fw-esp32c6` ELF (`LP_EMU_BUILD_FW=1`), and
//! `just test-emu-c6-cli-boards` runs it.

#[path = "support/editor_reads.rs"]
mod editor_reads;

use std::path::{Path, PathBuf};

use editor_reads::{EditorReads, block_on, deploy, heartbeats};
use lp_cli::commands::emu::link_host::{C6Board, EmuLinkHost};
use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::machine::{AppSource, Esp32C6Builder, TimeGrade, UsbHost};
use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image};
use lpa_client::LpClient;

/// One fixed host nonce, so two runs are the same run.
const NONCE: u32 = 0xF4A6_C6E7;

/// Two minutes of play: four steps, the last with four-digit stamps.
const PLAYING_US: u64 = 125_000_000;

/// One more step.
const ONE_STEP_US: u64 = 35_000_000;

/// The boot banner: a second one is a reset.
const BOOT_BANNER: &str = "[INIT] Initializing board...";

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6-cli-boards` runs it"]
fn the_tryouts_first_sync_survives_its_playlists_steps() {
    let elf = match fw_esp32c6_image(&FwImage::SHIPPED) {
        Ok(path) => path,
        Err(reason) => {
            eprintln!("emu_panel_reads: skipped — {reason}");
            return;
        }
    };
    let mut host = hosted(&elf);
    let hello = host
        .wait_for_line("\"hello\":{", 3_000_000)
        .expect("the boot");
    assert!(hello.is_some(), "no hello:\n{}", host.console().join("\n"));
    deploy(
        &mut host,
        &repo_root().join("catalog/projects/playful-choker-tryout"),
    );
    let loaded = {
        let mut client = LpClient::new(&mut host).with_request_ids_from(1_000);
        block_on(client.project_list_loaded()).expect("the loaded projects")
    };
    let project = loaded
        .value
        .first()
        .cloned()
        .expect("the deploy loaded the tryout");

    let mut editor = EditorReads::new(&mut host, project.handle);
    editor.idle(PLAYING_US);
    editor.initial_sync();
    editor.idle(ONE_STEP_US);
    editor.initial_sync();
    let reads = std::mem::take(&mut editor.reads);
    drop(editor);

    let console = host.console().to_vec();
    let boots = console
        .iter()
        .filter(|line| line.contains(BOOT_BANNER))
        .count();
    let memory = heartbeats(&console)
        .last()
        .map(|beat| {
            format!(
                "free {} B, largest block {:?}",
                beat.free_bytes, beat.largest_free_block
            )
        })
        .unwrap_or_default();
    let report = reads
        .iter()
        .map(|read| {
            format!(
                "  #{} {} at {:.1} s: {} events, ended {}, error {:?}",
                read.index, read.label, read.at_s, read.events, read.ended, read.error
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    println!("lp-emu:esp32c6:t1 — {boots} boot(s); {memory}\n{report}");

    assert_eq!(boots, 1, "the board reset:\n{report}");
    assert_eq!(reads.len(), 8, "{report}");
    assert!(
        reads.iter().all(|read| read.ended && read.error.is_none()),
        "a read failed:\n{report}"
    );
}

/// The shipped image, attached and draining from power-on, with a blank
/// flash: no device store, so Bluetooth is on.
fn hosted(elf: &Path) -> EmuLinkHost<C6Board> {
    let machine = Esp32C6Builder::new()
        .app(AppSource::Path(elf.to_path_buf()))
        .flash(FlashBacking::Blank)
        // Not strict: this test is about what a read carries, not the bus.
        .strict(false)
        .time_grade(TimeGrade::T1)
        .usb_host(UsbHost::Attached { draining: true })
        .usb_sj_queue_source()
        .build()
        .expect("the shipped image builds a machine");
    EmuLinkHost::new(C6Board::new(machine).expect("a hosted board"), NONCE, true)
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lp-cli sits under the repo root")
        .to_path_buf()
}
