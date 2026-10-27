//! The pattern you pick survives a power cut, on the emulated C6
//! (playlists-on-the-bus plan `lp2025/2026-10-20-0001-playlists-on-the-bus`,
//! AC2; ADR `docs/adr/2026-10-20-the-picked-pattern-is-a-bus-setting.md`).
//!
//! The PLAYFUL choker tryout (`catalog/projects/playful-choker-tryout`, 25
//! entries, Soft Noise authored) goes on a ROM-up split-image C6 over its
//! USB link. A panel write picks Aurora (entry 2) on `playlist.entry`, the
//! board saves its panel state, and the power is cut. The first read after
//! the boot must say Aurora is the entry playing — the boot loaded the pick,
//! not the authored entry. Then the pick is cleared and the board power-
//! cycled again, and it is back on Soft Noise.
//!
//! **A report as well as a check.** Each boot prints the firmware's own
//! `[perf] first light …` line, so the two boots (a restored pick, and the
//! authored entry) show what the restored pick costs: the authored entry is
//! still derived and attached at load and unloaded in the first tick (plan
//! Q3). Emulated time, never a gate.
//!
//! Emulated: `lp-emu:esp32c6:t1` (`LP_PICK_GRADE` for another), ROM-up.
//! Never hardware-validated. `#[ignore]`d: it needs the split build
//! (`LP_EMU_BUILD_FW=1`, or `LP_EMU_C6_SPLIT_<SLUG>`).
//!
//! ```bash
//! LP_EMU_BUILD_FW=1 cargo test -p lp-cli --test emu_playlist_pick_reboot -- --ignored --nocapture
//! ```

#[path = "support/editor_reads.rs"]
mod editor_reads;
#[path = "support/tree_store_board.rs"]
mod tree_store_board;

use editor_reads::{EditorReads, block_on};
use lp_cli::commands::emu::link_host::{C6Board, EmuLinkHost};
use lp_emu_esp_common::Strap;
use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::loader::ResetCause;
use lp_emu_esp32c6::machine::{AppSource, BootMode, Esp32C6Builder, TimeGrade, UsbHost};
use lp_emu_esp32c6::test_support::{FwImage, split_image};
use lpa_client::LpClient;
use lpc_model::{LpValue, NodeId};
use lpc_wire::{
    ProjectReadEvent, WirePanelClearRequest, WirePanelWriteRequest, WireProjectHandle, WireScopeRef,
};
use serde_json::{Value, json};
use tree_store_board::project_files;

const TRYOUT: &str = "catalog/projects/playful-choker-tryout";
const SOFT_NOISE: u32 = 1;
const AURORA: u32 = 2;

/// Emulated µs a boot may take to a console line (a wedge detector).
const LINE_BUDGET_US: u64 = 30_000_000;

/// Past the panel state's 10 s write throttle, so the pick is on flash.
const PAST_THE_SAVE_US: u64 = 12_000_000;

#[test]
#[ignore = "needs the split fw-esp32c6 build; emulated, never hardware-validated"]
fn a_picked_pattern_survives_a_power_cut_on_the_emulated_c6() {
    let grade = std::env::var("LP_PICK_GRADE")
        .ok()
        .filter(|g| !g.is_empty())
        .map_or(TimeGrade::T1, |g| {
            TimeGrade::parse(&g).expect("LP_PICK_GRADE is t1, t2, t3 or t4")
        });
    let split = match split_image(&FwImage::SHIPPED) {
        Ok(split) => split,
        Err(reason) => {
            eprintln!("emu_playlist_pick_reboot: skipped — {reason}");
            return;
        }
    };
    let merged = std::fs::read(split.merged()).expect("the merged image");
    let machine = Esp32C6Builder::new()
        .boot_mode(BootMode::RomUp)
        .app(AppSource::Path(split.p2_elf()))
        .flash(FlashBacking::Bytes(merged))
        .reset_cause(ResetCause::UsbUartHpSys)
        .strap(Strap::App)
        .time_grade(grade)
        .usb_host(UsbHost::Attached { draining: true })
        .usb_sj_queue_source()
        .reboot_on_reset(true)
        .build()
        .expect("the split image builds a machine");
    let mut host = EmuLinkHost::new(
        C6Board::new(machine).expect("a hosted board"),
        0x91C0_0001,
        true,
    );
    line(&mut host, "\"hello\":{");

    // The tryout goes on, and becomes the startup project.
    let handle = {
        let mut client = LpClient::new(&mut host).with_request_ids_from(1_000);
        block_on(client.replace_and_load_project("tryout", &project_files(TRYOUT)))
            .unwrap_or_else(|e| panic!("upload: {e}"))
            .value
    };
    line(&mut host, "startup_project set to tryout");
    let (root, _) = playing(&mut host, handle);
    let scope = WireScopeRef::Module {
        owner: NodeId::new(root),
    };

    // Pick Aurora, and let the panel state reach flash.
    {
        let mut client = LpClient::new(&mut host).with_request_ids_from(2_000);
        block_on(client.project_panel_write(
            handle,
            WirePanelWriteRequest {
                scope,
                channel: lpc_model::PLAYLIST_ENTRY_CHANNEL.to_string(),
                value: LpValue::U32(AURORA).into(),
                ttl_ms: None,
            },
        ))
        .unwrap_or_else(|e| panic!("pick: {e}"));
    }
    idle(&mut host, PAST_THE_SAVE_US);
    let (_, before) = playing(&mut host, handle);
    assert_eq!(before, Some(AURORA), "the pick plays before the power cut");

    // Cut the power. The first read after the boot: Aurora.
    host = power_cycle(host, 0x91C0_0100);
    let picked_light = line(&mut host, "[perf] first light ");
    let handle = loaded_handle(&mut host);
    let (root, after) = playing(&mut host, handle);
    println!("restored pick: {picked_light}");
    assert_eq!(
        after,
        Some(AURORA),
        "the board comes back on the pattern picked before the power cut"
    );

    // Clear the pick, save, cut the power: back on the authored entry.
    {
        let mut client = LpClient::new(&mut host).with_request_ids_from(3_000);
        block_on(client.project_panel_clear(
            handle,
            WirePanelClearRequest::Channel {
                scope: WireScopeRef::Module {
                    owner: NodeId::new(root),
                },
                channel: lpc_model::PLAYLIST_ENTRY_CHANNEL.to_string(),
            },
        ))
        .unwrap_or_else(|e| panic!("clear: {e}"));
    }
    idle(&mut host, PAST_THE_SAVE_US);
    host = power_cycle(host, 0x91C0_0200);
    let authored_light = line(&mut host, "[perf] first light ");
    let handle = loaded_handle(&mut host);
    let (_, authored) = playing(&mut host, handle);
    println!("authored entry: {authored_light}");
    assert_eq!(
        authored,
        Some(SOFT_NOISE),
        "Clear gives back the authored entry"
    );

    println!(
        "configuration lp-emu:esp32c6:{} (emulated, never hardware-validated)",
        format!("{grade:?}").to_lowercase()
    );
}

/// The root module's runtime id and the playlist's playing entry
/// (`PlaylistState.active_entry`), from a fresh skeleton and slot read.
fn playing(host: &mut EmuLinkHost<C6Board>, handle: WireProjectHandle) -> (u32, Option<u32>) {
    let mut reads = EditorReads::new(host, handle);
    let skeleton = reads.read(
        "skeleton",
        json!({"since":null,"queries":[{"nodes":{"level":"detail","nodes":"all","include_slots":false}}]}),
    );
    let nodes = created_nodes(&skeleton);
    let root = nodes
        .iter()
        .find(|(_, _, root)| *root)
        .map(|(_, id, _)| *id)
        .unwrap_or_else(|| panic!("the root module among {nodes:?}"));
    let playlist = nodes
        .iter()
        .find(|(path, _, _)| path.contains(".playlist") || path.contains("\"ty\":\"playlist\""))
        .map(|(_, id, _)| *id)
        .unwrap_or_else(|| panic!("the playlist among {nodes:?}"));
    let slots = reads.read(
        "playlist slots",
        json!({"since":null,"queries":[{"nodes":{"level":"detail","nodes":{"by_ids":[playlist]},"include_slots":true}}]}),
    );
    let entry = active_entry(&slots);
    if entry.is_none()
        && let Ok(path) = std::env::var("LP_PICK_DUMP")
    {
        let dump = serde_json::to_string_pretty(&slots).unwrap_or_default();
        std::fs::write(path, dump).expect("dump the read");
    }
    (root, entry)
}

/// The playlist's `active_entry`: the fourth field of its runtime state
/// record (`PlaylistState`: output, entry_time, entry_progress,
/// active_entry), from the `node.<id>.state` slot root of a slot read.
fn active_entry(events: &[ProjectReadEvent]) -> Option<u32> {
    events.iter().find_map(|event| {
        let value = serde_json::to_value(event).ok()?;
        let root = value.pointer("/query/event/nodes/slot_root")?;
        if !root.get("name")?.as_str()?.ends_with(".state") {
            return None;
        }
        let field = root.pointer("/data/fields/3/value")?;
        field.as_u64().map(|entry| entry as u32)
    })
}

/// Every node the skeleton created, as `(path, id, is the root)`.
fn created_nodes(events: &[ProjectReadEvent]) -> Vec<(String, u32, bool)> {
    let mut nodes = Vec::new();
    for event in events {
        let value = serde_json::to_value(event).unwrap_or(Value::Null);
        let Some(Value::Array(deltas)) = value.pointer("/query/event/nodes/tree_deltas/deltas")
        else {
            continue;
        };
        for delta in deltas {
            let Some(created) = delta.get("created") else {
                continue;
            };
            let (Some(id), Some(path)) = (
                created.get("id").and_then(Value::as_u64),
                created.get("path"),
            ) else {
                continue;
            };
            let path = match path {
                Value::String(path) => path.clone(),
                other => other.to_string(),
            };
            let root = created.get("parent").is_none_or(Value::is_null);
            nodes.push((path, id as u32, root));
        }
    }
    nodes
}

/// The handle of the project the board loaded at boot.
fn loaded_handle(host: &mut EmuLinkHost<C6Board>) -> WireProjectHandle {
    let mut client = LpClient::new(&mut *host).with_request_ids_from(9_000);
    let loaded = block_on(client.project_list_loaded())
        .unwrap_or_else(|e| panic!("list loaded: {e}"))
        .value;
    loaded.first().expect("the startup project loaded").handle
}

fn idle(host: &mut EmuLinkHost<C6Board>, us: u64) {
    let until = host.board.machine.micros() + us;
    host.run_until(until, None).expect("the board runs");
}

/// Power-cycle the board (every domain to power-on, the flash as it is) and
/// put a fresh host on its link.
fn power_cycle(host: EmuLinkHost<C6Board>, nonce: u32) -> EmuLinkHost<C6Board> {
    let mut board = host.board;
    assert!(board.machine.power_cycle(Strap::App), "a power-on snapshot");
    EmuLinkHost::new(board, nonce, true)
}

/// Step `host` until a console line contains `needle`; that line.
fn line(host: &mut EmuLinkHost<C6Board>, needle: &str) -> String {
    let until = host.board.machine.micros() + LINE_BUDGET_US;
    let mut seen = 0;
    loop {
        if let Some(l) = host.console()[seen..].iter().find(|l| l.contains(needle)) {
            return l.clone();
        }
        seen = host.console().len();
        if host.board.machine.micros() >= until {
            let console = host.console();
            panic!(
                "no {needle:?}:\n{}",
                console[console.len().saturating_sub(60)..].join("\n")
            );
        }
        if let Err(e) = host.step() {
            panic!("the boot failed before {needle:?}: {e}");
        }
    }
}
