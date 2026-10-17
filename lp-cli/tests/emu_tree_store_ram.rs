//! The tree store's RAM on the emulated C6, `fs-tree` on vs off (plan
//! `lp2025/2026-10-08-2339-tree-store-firmware-and-emulator`, P8, D5). The
//! split image (the product's shape), each with `heap_peak_diag`, same
//! project, same emulator:
//!
//! (The unsplit image, direct load: the split one boots ROM-up and walked
//! far slower than the 10-minute command it had; the heap figures compare
//! the two filesystems on one image shape.)
//!
//! - **resident**: requested bytes live after a reboot with a c40-shaped
//!   project stored (and loaded at boot), at the second `[heap]` line;
//!   `fs-tree` minus littlefs is the store's share;
//! - **transient**: the window peak over one operation minus `live` just
//!   before it — a save (one 1.7 KB document written whole), a panel write
//!   (its auto-save), a 12 KiB file in three chunks (a write and two 4 KiB
//!   appends, each with its `file_size` check);
//! - **overlap**: the highest window peak and the lowest largest free block
//!   (sampled at the `[heap]` lines and the heartbeats) across a c40 push
//!   (into its own slot: littlefs has no room for two) whose load compiles
//!   the shader, and panel writes after it; and whether
//!   the board stayed up (one boot, no allocation failure).
//!
//! Requested bytes, not allocator overhead (`max_usage` is printed beside
//! them). The emulated C6 has no Wi-Fi joined, so its largest block is far
//! above silicon's 38–46 KB with Wi-Fi: compare the two images, not either
//! with a desk. Prints a table; asserts only that it ran.
//!
//! Emulated: `lp-emu:esp32c6:t1+net=lan`. Never hardware-validated.
//! `#[ignore]`d: it needs split builds (`LP_EMU_BUILD_FW=1`). Run:
//! `LP_EMU_BUILD_FW=1 cargo test -p lp-cli --release --test emu_tree_store_ram -- --include-ignored --nocapture`.

#[path = "support/editor_reads.rs"]
mod editor_reads;
#[path = "support/tree_store_board.rs"]
mod tree_store_board;
#[path = "support/tree_store_measure.rs"]
mod tree_store_measure;

use editor_reads::{block_on, heartbeats};
use lp_cli::commands::emu::link_host::{C6Board, EmuLinkHost};
use lp_emu_esp32c6::test_support::{FwImage, ReferenceImage};
use lpa_client::LpClient;
use lpc_model::LpValue;
use lpfs::LpPath;
use tree_store_board::*;
use tree_store_measure::*;

#[derive(Debug, Default)]
struct Ram {
    resident_live: u64,
    resident_max_usage: u64,
    resident_largest: u64,
    save: i64,
    panel: i64,
    chunks: i64,
    overlap_peak: u64,
    overlap_min_largest: u64,
    boots: usize,
}

#[test]
#[ignore = "needs split fw-esp32c6 builds; run by hand (see the module docs)"]
fn ram_resident_transient_and_compile_overlap() {
    let mut rows = Vec::new();
    // `LP_TREE_RAM_FS=fs-tree|littlefs`: one image per run (each walk is
    // minutes of emulated time).
    let only = std::env::var("LP_TREE_RAM_FS")
        .ok()
        .filter(|s| !s.is_empty());
    for (label, image) in [
        ("fs-tree", None),
        ("littlefs", Some(ReferenceImage::FIELDED_LITTLEFS_HEAP_PEAK)),
    ] {
        if only.as_deref().is_some_and(|o| o != label) {
            continue;
        }
        // The tree store is the shipped image since the cutover; littlefs is
        // the pinned fielded image, booted on its own table (`fielded_chip`).
        let found = match &image {
            None => elf("emu_tree_store_ram", &FwImage::SHIPPED_HEAP_PEAK),
            Some(fielded) => fielded_elf("emu_tree_store_ram", fielded),
        };
        let Some(elf) = found else {
            return;
        };
        rows.push((label, measure(label, &elf)));
    }
    println!(
        "\nRAM — configuration=lp-emu:esp32c6:t1+net=lan, unsplit image (direct load), heap_peak_diag (requested bytes)"
    );
    println!(
        "| | resident live | resident max_usage | largest at rest | save | panel write | 12 KiB in 3 chunks | overlap peak | overlap min largest | boots |"
    );
    println!("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|");
    for (label, r) in &rows {
        println!(
            "| {label} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            r.resident_live,
            r.resident_max_usage,
            r.resident_largest,
            r.save,
            r.panel,
            r.chunks,
            r.overlap_peak,
            r.overlap_min_largest,
            r.boots
        );
    }
    if let [(_, t), (_, l)] = &rows[..] {
        println!(
            "fs-tree − littlefs: resident {:+} B; transient save {:+}, panel {:+}, chunks {:+}; overlap peak {:+}, min largest {:+}",
            t.resident_live as i64 - l.resident_live as i64,
            t.save - l.save,
            t.panel - l.panel,
            t.chunks - l.chunks,
            t.overlap_peak as i64 - l.overlap_peak as i64,
            t.overlap_min_largest as i64 - l.overlap_min_largest as i64,
        );
    }
    for (label, r) in &rows {
        assert_eq!(r.boots, 1, "{label}: the board reset during the walk");
    }
}

fn measure(label: &str, elf: &std::path::Path) -> Ram {
    let wall = std::time::Instant::now();
    let note = |host: &EmuLinkHost<C6Board>, what: &str| {
        eprintln!(
            "[{label}] {what} at {:.1} s emulated, {:.0} s wall",
            host.board_seconds(),
            wall.elapsed().as_secs_f64()
        );
    };
    let mut host = board(elf, blank_for(label), 0x4A40_0001);
    boot(&mut host);
    let c40 = c40_files();
    {
        let mut client = LpClient::new(&mut host).with_request_ids_from(1_000);
        block_on(client.replace_and_load_project("demo", &c40)).expect("upload c40");
    }
    note(&host, "uploaded");
    let mut host = power_cycle(host, 0x4A40_0002);
    boot(&mut host);
    note(&host, "rebooted");
    let mut ram = Ram::default();

    // Resident: the second line after the reboot (the first heartbeat's
    // window holds the boot's own load).
    next_heap_line(&mut host);
    let (rest, _) = next_heap_line(&mut host);
    ram.resident_live = rest.live;
    note(&host, &format!("resident {rest:?}"));
    ram.resident_max_usage = rest.max_usage;
    ram.resident_largest = rest.largest;

    // Transient: a save.
    let (before, _) = next_heap_line(&mut host);
    {
        let mut doc = c40
            .iter()
            .find(|(p, _)| p == "docs/doc-000.json")
            .unwrap()
            .1
            .clone();
        doc.extend_from_slice(b" ");
        let mut client = LpClient::new(&mut host).with_request_ids_from(2_000);
        block_on(client.fs_write(LpPath::new("/projects/demo/docs/doc-000.json"), doc))
            .expect("save");
    }
    let (after, _) = next_heap_line(&mut host);
    ram.save = after.peak as i64 - before.live as i64;
    note(&host, &format!("save {}", ram.save));

    // Transient: a panel write and its auto-save (10 s spacing).
    let (before, from) = next_heap_line(&mut host);
    panel_time(&mut host, 3.5);
    let until = host.board.machine.micros() + 12_000_000;
    host.run_until(until, None).expect("the run");
    next_heap_line(&mut host);
    ram.panel = heap_lines(&host, from)
        .iter()
        .map(|l| l.peak)
        .max()
        .unwrap_or(0) as i64
        - before.live as i64;

    // Transient: a 12 KiB file in three chunks.
    let (before, _) = next_heap_line(&mut host);
    {
        let big: Vec<u8> = (0..12_288u32)
            .map(|i| b"lightplayer "[(i % 12) as usize])
            .collect();
        let mut client = LpClient::new(&mut host).with_request_ids_from(3_000);
        for request in lpa_client::project_deploy::project_write_requests(
            "demo",
            [lpa_client::ProjectDeployFile::new(
                "big.txt".to_string(),
                big,
            )],
        ) {
            block_on(client.send_request(request)).expect("a chunk");
        }
    }
    let (after, _) = next_heap_line(&mut host);
    ram.chunks = after.peak as i64 - before.live as i64;
    note(&host, &format!("chunks {}", ram.chunks));

    // Overlap: a c40 push whose load compiles, then panel writes.
    let beats_from = host.console().len();
    let (_, from) = next_heap_line(&mut host);
    {
        let mut edited = c40.clone();
        for (p, b) in &mut edited {
            if p == "shader.glsl" {
                b.extend_from_slice(b"\n// edited for the overlap\n");
            }
        }
        let mut client = LpClient::new(&mut host).with_request_ids_from(4_000);
        // Into the same slot: littlefs cannot hold two c40 copies (`no space
        // left on device` at the 27th document of the second: every file
        // over its 512 B inline limit takes whole 4 KiB blocks).
        block_on(client.replace_and_load_project("demo", &edited)).expect("the push");
    }
    panel_time(&mut host, 4.5);
    panel_time(&mut host, 5.5);
    let until = host.board.machine.micros() + 11_000_000;
    host.run_until(until, None).expect("the run");
    next_heap_line(&mut host);
    let lines = heap_lines(&host, from);
    ram.overlap_peak = lines.iter().map(|l| l.peak).max().unwrap_or(0);
    let beat_min = heartbeats(&host.console()[beats_from..])
        .iter()
        .filter_map(|b| b.largest_free_block)
        .min();
    ram.overlap_min_largest = lines
        .iter()
        .map(|l| l.largest)
        .chain(beat_min)
        .min()
        .unwrap_or(0);
    ram.boots = boots(&host);
    note(
        &host,
        &format!("overlap {} / {}", ram.overlap_peak, ram.overlap_min_largest),
    );
    ram
}

fn panel_time(host: &mut EmuLinkHost<C6Board>, value: f32) {
    let mut client = LpClient::new(host).with_request_ids_from(7_000 + (value * 10.0) as u64 * 100);
    let loaded = block_on(client.project_list_loaded())
        .expect("loaded")
        .value;
    let handle = loaded.first().expect("a project").handle;
    let read: lpc_wire::ProjectReadRequest = serde_json::from_value(serde_json::json!({
        "since": null,
        "probes": [{"binding_graph": {"structure": "always", "include_values": false}}]
    }))
    .unwrap();
    let events = block_on(client.project_read(handle, read))
        .expect("read")
        .value;
    let json = serde_json::to_value(&events).unwrap();
    let scope = find_scope(&json).expect("a time channel");
    block_on(client.project_panel_write(
        handle,
        lpc_wire::WirePanelWriteRequest {
            scope: serde_json::from_value(scope).unwrap(),
            channel: "time".to_string(),
            value: LpValue::F32(value),
            ttl_ms: None,
        },
    ))
    .expect("panel write");
}

fn find_scope(v: &serde_json::Value) -> Option<serde_json::Value> {
    match v {
        serde_json::Value::Object(m) => {
            if m.get("name").and_then(|n| n.as_str()) == Some("time")
                && let Some(s) = m.get("scope")
                && !s.is_null()
            {
                return Some(s.clone());
            }
            m.values().find_map(find_scope)
        }
        serde_json::Value::Array(a) => a.iter().find_map(find_scope),
        _ => None,
    }
}
