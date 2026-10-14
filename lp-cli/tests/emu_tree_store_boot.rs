//! The `fs-tree` firmware (the tree store as `lpfs`) booting on the
//! emulated C6 under every mount verdict (plan
//! `lp2025/2026-10-08-2339-tree-store-firmware-and-emulator`, P6). No cuts
//! here; `emu_tree_store_cuts.rs` cuts.
//!
//! Each case waits for the board's own words — the `[FS] tree store …` boot
//! word and the hello's `fs` — and reads the flash back on the host:
//!
//! 1. **Blank flash** → `formatted`, the server answers; the host mounts the
//!    partition as an empty store.
//! 2. **Upload `projects/test/basic`, power-cycle** → `mounted` with its
//!    summary; every file reads back byte for byte over the wire and from the
//!    host's own mount of the flash; the package hash is the same.
//! 3. **A littlefs partition** (today's shipped image after an upload) → the
//!    store finds no store of its own: `formatted`, the old files gone (the
//!    D8 hazard in miniature; the update guard is for the other direction).
//! 4. **A legacy-layout chip** (a pre-repartition littlefs at `0x310000`) →
//!    `legacy_held`, the old region byte-identical.
//! 5. **A newer store header** (`Unsupported`) → `refused`, the boot word
//!    says "a newer or damaged store header — files kept", the device store
//!    is locked (Bluetooth off for that reason; network writes refused), and
//!    the partition is byte-identical.
//! 6. **A damaged store** (file records, no root) → the same as 5.
//! 7. **A project switch and a panel write persist** across a power cycle.
//!
//! Plus one **split** boot (the split image's core holds the store): blank →
//! `formatted`, upload, power-cycle → `mounted`, the files on the host mount.
//!
//! On access (5, 6): over USB — the emulator's only link here — the board
//! answers at edit, as every board does on its trusted link; what "locked"
//! changes is the boot's device store (`device_store_at_boot`), which the
//! `[ble] off (file store refused …)` line and the refused network writes
//! show.
//!
//! Emulated: `lp-emu:esp32c6:t1+net=lan` (direct load; the split case
//! ROM-up). Never hardware-validated. `#[ignore]`d: they need a built
//! `fw-esp32c6` ELF (`LP_EMU_BUILD_FW=1`); `just test-emu-c6-cli-boards`
//! runs them.

#[path = "support/editor_reads.rs"]
mod editor_reads;
#[path = "support/tree_store_board.rs"]
mod tree_store_board;

use editor_reads::block_on;
use lp_cli::commands::emu::link_host::{C6Board, EmuLinkHost};
use lp_emu_esp_common::Strap;
use lp_emu_esp32c6::flash::{FlashBacking, LPFS_LEN, LPFS_OFFSET};
use lp_emu_esp32c6::loader::ResetCause;
use lp_emu_esp32c6::machine::{AppSource, BootMode, Esp32C6Builder, UsbHost};
use lp_emu_esp32c6::test_support::{FwImage, split_image};
use lpa_client::LpClient;
use lpa_link::layout_migration::{LpfsGeometry, LpfsTree, build_image, legacy_c6_v1_table};
use lpc_model::LpValue;
use lpfs::LpPath;
use tree_store_board::*;

/// The upload's project id (it lands at `/projects/<id>/`).
const PROJECT: &str = "tree-basic";
/// A second project, for the switch.
const OTHER: &str = "tree-other";

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6-cli-boards` runs it"]
fn blank_flash_formats_and_serves() {
    let Some(elf) = fs_tree_elf("blank_flash_formats_and_serves") else {
        return;
    };
    let mut host = board(&elf, Vec::new(), 0x7EE0_0001);
    let hello = boot(&mut host);
    assert_eq!(hello_fs(&hello), "formatted", "{hello}");
    let word = boot_word(&host, "[FS] tree store formatted (");
    assert!(word.contains("(176 sectors, "), "{word}");
    let loaded = {
        let mut client = LpClient::new(&mut host).with_request_ids_from(100);
        block_on(client.project_list_loaded())
    };
    assert!(loaded.is_ok(), "the server answers: {loaded:?}");
    let (files, summary) = host_mount(lpfs(&chip(&host))).expect("the host mounts the store");
    println!(
        "1 blank: {word}; host mount {summary:?}, {} file(s)",
        files.len()
    );
    assert_eq!(summary.sectors, 176);
}

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6-cli-boards` runs it"]
fn an_upload_survives_a_power_cycle_byte_for_byte() {
    let Some(elf) = fs_tree_elf("an_upload_survives_a_power_cycle_byte_for_byte") else {
        return;
    };
    let mut host = board(&elf, Vec::new(), 0x7EE0_0002);
    boot(&mut host);
    let files = project_files("projects/test/basic");
    let hash_before = upload(&mut host, PROJECT, &files);

    let mut host = power_cycle(host, 0x7EE0_0012);
    let hello = boot(&mut host);
    assert_eq!(hello_fs(&hello), "mounted", "{hello}");
    let word = boot_word(&host, "[FS] tree store mounted (");
    let hash_after = {
        let mut client = LpClient::new(&mut host).with_request_ids_from(5_000);
        for (rel, bytes) in &files {
            let path = format!("/projects/{PROJECT}/{rel}");
            let got = block_on(client.fs_read(LpPath::new(&path)))
                .unwrap_or_else(|e| panic!("read {path}: {e}"))
                .value;
            assert_eq!(&got, bytes, "{path} over the wire");
        }
        block_on(client.hash_package(PROJECT)).expect("hash").value
    };
    assert_eq!(hash_after, hash_before, "the package hash after the reboot");
    let (stored, summary) = host_mount(lpfs(&chip(&host))).expect("the host mounts the store");
    for (rel, bytes) in &files {
        let path = format!("/projects/{PROJECT}/{rel}");
        assert_eq!(stored.get(&path), Some(bytes), "{path} on the host's mount");
    }
    println!(
        "2 upload: {word}; {} file(s) of the project, {} on the store; host {summary:?}; \
         hash {hash_after}",
        files.len(),
        stored.len()
    );
}

#[test]
#[ignore = "needs built fw-esp32c6 ELFs; `just test-emu-c6-cli-boards` runs it"]
fn a_littlefs_partition_is_no_store_and_is_formatted() {
    let test = "a_littlefs_partition_is_no_store_and_is_formatted";
    let (Some(littlefs), Some(elf)) = (elf(test, &FwImage::SHIPPED), fs_tree_elf(test)) else {
        return;
    };
    let mut lfs = board(&littlefs, Vec::new(), 0x7EE0_0003);
    boot(&mut lfs);
    upload(&mut lfs, PROJECT, &project_files("projects/test/basic"));
    let littlefs_chip = chip(&lfs);
    assert!(
        host_mount(lpfs(&littlefs_chip)).is_err(),
        "the premise: a littlefs partition is no tree store"
    );

    let mut host = board(&elf, littlefs_chip, 0x7EE0_0013);
    let hello = boot(&mut host);
    assert_eq!(hello_fs(&hello), "formatted", "{hello}");
    let word = boot_word(&host, "[FS] tree store formatted (");
    let listed = {
        let mut client = LpClient::new(&mut host).with_request_ids_from(100);
        block_on(client.fs_list_dir(LpPath::new("/projects"), true))
    };
    let (stored, _) = host_mount(lpfs(&chip(&host))).expect("the host mounts the store");
    assert!(
        !stored.keys().any(|p| p.starts_with("/projects/")),
        "the littlefs files are gone: {stored:?}"
    );
    println!("3 littlefs: {word}; /projects lists {listed:?}");
}

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6-cli-boards` runs it"]
fn a_legacy_layout_is_held_and_never_written() {
    let test = "a_legacy_layout_is_held_and_never_written";
    let (Some(littlefs), Some(elf)) = (elf(test, &FwImage::SHIPPED), fs_tree_elf(test)) else {
        return;
    };
    let geometry = LpfsGeometry::from_table(&legacy_c6_v1_table()).expect("the legacy lpfs");
    let tree = LpfsTree::from_files([
        (
            "/hardware.json".to_string(),
            b"{\"board\":\"held\"}".to_vec(),
        ),
        (
            format!("/projects/{PROJECT}/project.json"),
            b"{\"name\":\"held\"}".to_vec(),
        ),
    ]);
    let legacy = build_image(&tree, geometry).expect("a legacy littlefs image");
    let mut chip0 = vec![0xFFu8; lp_emu_esp32c6::flash::DEFAULT_FLASH_LEN as usize];
    let at = geometry.offset as usize;
    chip0[at..at + legacy.len()].copy_from_slice(&legacy);
    let region = at..(LPFS_OFFSET + LPFS_LEN) as usize;

    // The premise, on today's littlefs image: does this chip read as held?
    // A direct load stages the app in `factory` (0x10000..0x350000), and
    // today's `.text` runs past 0x310000 — over the old filesystem's
    // superblock pair — so the probe may find nothing to hold.
    let mut shipped = board(&littlefs, chip0.clone(), 0x7EE0_0014);
    let shipped_fs = hello_fs(&boot(&mut shipped));
    let mut host = board(&elf, chip0.clone(), 0x7EE0_0004);
    let hello = boot(&mut host);
    if shipped_fs != "legacy_held" {
        // Not reachable on this image: say so, and hold the tree store to
        // what today's image does with the same chip.
        let staged = &chip(&shipped)[at..at + 8192] != &legacy[..8192];
        println!(
            "4 legacy: NOT REACHABLE on this image — today's littlefs image reports \
             `{shipped_fs}` on a legacy-layout chip (the staged app overwrote the old \
             superblock pair at 0x310000: {staged}); fs-tree reports `{}`. The probe-and-hold \
             path is covered on the host (`tree_fs::tests::hold_writes_nothing`).",
            hello_fs(&hello)
        );
        assert_eq!(hello_fs(&hello), shipped_fs, "{hello}");
        return;
    }
    let before = chip(&shipped)[region.clone()].to_vec();
    assert_eq!(hello_fs(&hello), "legacy_held", "{hello}");
    boot_word(&host, "[FS] legacy-layout filesystem found at 0x310000");
    assert!(
        line_with(&host, "[FS] tree store formatted").is_none(),
        "{}",
        host.console().join("\n")
    );
    assert!(
        chip(&host)[region] == before[..],
        "the held chip was written between 0x310000 and the end of lpfs"
    );
    println!("4 legacy: legacy_held, 0x310000..0x400000 byte-identical");
}

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6-cli-boards` runs it"]
fn a_newer_store_header_is_refused_locked_and_untouched() {
    let Some(elf) = fs_tree_elf("a_newer_store_header_is_refused_locked_and_untouched") else {
        return;
    };
    let mut region = files_region();
    // The first trusted sector's version, made 4: a magic in front of a
    // newer version refuses before any CRC (FORMAT.md "Sector").
    let s = (0..region.len() / 4096)
        .find(|s| region[s * 4096..s * 4096 + 4] == 0x3153_544Cu32.to_le_bytes())
        .expect("a store sector");
    region[s * 4096 + 4..s * 4096 + 6].copy_from_slice(&4u16.to_le_bytes());
    refused_case("5 newer header", &elf, region, "newer format");
}

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6-cli-boards` runs it"]
fn a_damaged_store_is_refused_locked_and_untouched() {
    let Some(elf) = fs_tree_elf("a_damaged_store_is_refused_locked_and_untouched") else {
        return;
    };
    let mut region = files_region();
    // Every hot sector (the roots, and the panel file) erased: the files'
    // records stay in the cold sectors, and no root names them.
    for s in 0..region.len() / 4096 {
        let h = &region[s * 4096..s * 4096 + 24];
        if h[..4] == 0x3153_544Cu32.to_le_bytes() && h[6] == 1 {
            region[s * 4096..(s + 1) * 4096].fill(0xFF);
        }
    }
    refused_case("6 damaged", &elf, region, "no complete root");
}

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6-cli-boards` runs it"]
fn a_project_switch_and_a_panel_write_persist() {
    let Some(elf) = fs_tree_elf("a_project_switch_and_a_panel_write_persist") else {
        return;
    };
    let mut host = board(&elf, Vec::new(), 0x7EE0_0007);
    boot(&mut host);
    let files = project_files("projects/test/basic");
    upload(&mut host, PROJECT, &files);
    upload(&mut host, OTHER, &files);
    // The switch back: `LoadProject` of the first (it writes the server
    // config that names the project to boot).
    let handle = {
        let mut client = LpClient::new(&mut host).with_request_ids_from(3_000);
        block_on(client.stop_all_projects()).expect("stop");
        block_on(client.project_load(&lpa_client::project_deploy::project_load_path(PROJECT)))
            .expect("the switch")
            .value
    };
    panel_write_time(&mut host, handle, 42.5);
    // The panel state is written at most every 10 s (`PANEL_STATE_WRITE_
    // INTERVAL_MS`): let the board's clock pass it.
    let until = host.board.machine.micros() + 12_000_000;
    host.run_until(until, None).expect("the run");
    let panel = format!("/projects/{PROJECT}/.lp/panel.json");
    let (stored, _) = host_mount(lpfs(&chip(&host))).expect("mount");
    let saved = stored
        .get(&panel)
        .unwrap_or_else(|| panic!("no {panel} on the store: {:?}", stored.keys()))
        .clone();

    let mut host = power_cycle(host, 0x7EE0_0017);
    let hello = boot(&mut host);
    assert_eq!(hello_fs(&hello), "mounted", "{hello}");
    let (stored, _) = host_mount(lpfs(&chip(&host))).expect("mount");
    assert_eq!(stored.get(&panel), Some(&saved), "the panel state");
    let loaded = {
        let mut client = LpClient::new(&mut host).with_request_ids_from(6_000);
        block_on(client.project_list_loaded())
            .expect("loaded")
            .value
    };
    let names: Vec<String> = loaded.iter().map(|p| format!("{p:?}")).collect();
    assert!(
        names.iter().any(|p| p.contains(PROJECT)),
        "the switched-to project boots: {names:?}"
    );
    assert!(
        String::from_utf8_lossy(&saved).contains("42.5"),
        "the written value: {}",
        String::from_utf8_lossy(&saved)
    );
    println!(
        "7 switch + panel: boots {names:?}; {panel} = {}",
        String::from_utf8_lossy(&saved)
    );
}

#[test]
#[ignore = "needs a split fw-esp32c6 build; `just test-emu-c6-cli-boards` runs it"]
fn the_split_image_formats_and_mounts_its_store() {
    let split = match split_image(&FwImage::FS_TREE) {
        Ok(split) => split,
        Err(reason) => {
            eprintln!("emu_tree_store_boot split: skipped — {reason}");
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
        .usb_host(UsbHost::Attached { draining: true })
        .usb_sj_queue_source()
        .reboot_on_reset(true)
        .build()
        .expect("the split image builds a machine");
    let mut host = EmuLinkHost::new(
        C6Board::new(machine).expect("a hosted board"),
        0x7EE0_0008,
        true,
    );
    let hello = boot(&mut host);
    assert_eq!(hello_fs(&hello), "formatted", "{hello}");
    let formatted = boot_word(&host, "[FS] tree store formatted (");
    let files = project_files("projects/test/basic");
    let hash = upload(&mut host, PROJECT, &files);

    let mut host = power_cycle(host, 0x7EE0_0018);
    let hello = boot(&mut host);
    assert_eq!(hello_fs(&hello), "mounted", "{hello}");
    let mounted = boot_word(&host, "[FS] tree store mounted (");
    let (stored, _) = host_mount(lpfs(&chip(&host))).expect("mount");
    for (rel, bytes) in &files {
        let path = format!("/projects/{PROJECT}/{rel}");
        assert_eq!(stored.get(&path), Some(bytes), "{path}");
    }
    let again = {
        let mut client = LpClient::new(&mut host).with_request_ids_from(9_000);
        block_on(client.hash_package(PROJECT)).expect("hash").value
    };
    assert_eq!(again, hash);
    println!("split: {formatted} → {mounted}");
}

// ---- helpers ---------------------------------------------------------------

/// The boot word starting with `prefix`, or a panic with the console.
fn boot_word(host: &EmuLinkHost<C6Board>, prefix: &str) -> String {
    line_with(host, prefix)
        .unwrap_or_else(|| panic!("no {prefix:?}:\n{}", host.console().join("\n")))
        .clone()
}

/// Upload `files` as project `id` (Studio's replace-and-load) and return its
/// package hash.
fn upload(host: &mut EmuLinkHost<C6Board>, id: &str, files: &[(String, Vec<u8>)]) -> String {
    let mut client = LpClient::new(host).with_request_ids_from(1_000);
    block_on(client.replace_and_load_project(id, files))
        .unwrap_or_else(|e| panic!("upload {id}: {e}"));
    block_on(client.hash_package(id)).expect("hash").value
}

/// A store region holding a few files (and a hot panel file), built on the
/// host as the firmware would have left it.
fn files_region() -> Vec<u8> {
    host_store(&[
        ("/hardware.json", b"{\"board\":\"kept\"}"),
        ("/projects/kept/project.json", b"{\"name\":\"kept\"}"),
        ("/projects/kept/.lp/panel.json", b"{\"version\":1}"),
    ])
}

/// Boot the `fs-tree` image over `region`; it must refuse, say so, lock
/// the device store, refuse network writes, and leave `lpfs` untouched.
fn refused_case(label: &str, elf: &std::path::Path, region: Vec<u8>, why: &str) {
    let before = region.clone();
    let mut host = board(elf, chip_with_lpfs(&region), 0x7EE0_0005);
    let hello = boot(&mut host);
    assert_eq!(hello_fs(&hello), "refused", "{hello}");
    let word = boot_word(
        &host,
        "[FS] tree store refused: a newer or damaged store header — files kept (",
    );
    assert!(word.contains(why), "{word}");
    host.run_until(host.board.machine.micros() + 2_000_000, None)
        .expect("the run");
    let ble = host
        .console()
        .iter()
        .find(|l| l.contains("[ble] "))
        .cloned()
        .unwrap_or_default();
    assert!(
        ble.contains("[ble] off (file store refused"),
        "the boot's device store is locked (Bluetooth off for it): {ble:?}\n{}",
        host.console().join("\n")
    );
    let network = {
        let mut client = LpClient::new(&mut host).with_request_ids_from(100);
        block_on(client.network_add(
            "lab".to_string(),
            lpc_wire::WifiPassword::new("not-a-real-one"),
            None,
        ))
    };
    let refusal = format!("{network:?}");
    assert!(
        refusal.contains("refused its file store"),
        "a network write is refused on a refused board: {refusal}"
    );
    assert!(
        lpfs(&chip(&host)) == &before[..],
        "{label}: the refused partition was written"
    );
    println!("{label}: {word}; {ble}; network write: refused; lpfs byte-identical");
}

/// Write `value` to the clock's `time` channel from the panel (the
/// control's home scope, read off the binding graph).
fn panel_write_time(
    host: &mut EmuLinkHost<C6Board>,
    handle: lpc_wire::WireProjectHandle,
    value: f32,
) {
    let read: lpc_wire::ProjectReadRequest = serde_json::from_value(serde_json::json!({
        "since": null,
        "probes": [{"binding_graph": {"structure": "always", "include_values": false}}]
    }))
    .expect("a binding-graph read");
    let mut client = LpClient::new(host).with_request_ids_from(4_000);
    let events = block_on(client.project_read(handle, read))
        .expect("the read")
        .value;
    let json = serde_json::to_value(&events).expect("events as JSON");
    let scope = find_time_scope(&json).unwrap_or_else(|| panic!("no `time` channel in {json}"));
    let request = lpc_wire::WirePanelWriteRequest {
        scope: serde_json::from_value(scope).expect("a scope"),
        channel: "time".to_string(),
        value: LpValue::F32(value),
        ttl_ms: None,
    };
    block_on(client.project_panel_write(handle, request)).expect("the panel write");
}

/// The `scope` of the channel named `time`, anywhere in a read's JSON.
fn find_time_scope(v: &serde_json::Value) -> Option<serde_json::Value> {
    match v {
        serde_json::Value::Object(map) => {
            if map.get("name").and_then(|n| n.as_str()) == Some("time")
                && let Some(scope) = map.get("scope")
                && !scope.is_null()
            {
                return Some(scope.clone());
            }
            map.values().find_map(find_time_scope)
        }
        serde_json::Value::Array(items) => items.iter().find_map(find_time_scope),
        _ => None,
    }
}
