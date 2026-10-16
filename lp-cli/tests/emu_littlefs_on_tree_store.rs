//! Today's littlefs firmware meeting a tree store on the emulated C6 (plan
//! `lp2025/2026-10-16-0848-tree-store-adoption`, A5; ADR
//! `2026-10-10-fs-tree-refused-state-and-update-guard`, amended 2026-10-16).
//!
//! A littlefs core reaches a tree store by a loader rollback from a tree
//! core whose trial failed, by a USB flash of an older image, or by Studio's
//! "Other version…" install. Before this it took the store for an
//! unformatted partition and formatted it. Each case boots the SHIPPED
//! image over a store built on the host by `lp-tree-store`, waits for the
//! board's own words, and reads the flash back:
//!
//! 1. **A store whose first sectors are in use** → the hello says
//!    `fs: refused` with no `fs.tree`; the boot word says the files are in
//!    the tree store and to update the firmware; Bluetooth is off for the
//!    refusal; a network write is refused in those words; the link still
//!    answers; `lpfs` is byte-identical, and the host still mounts every
//!    file. A power cycle changes nothing.
//! 2. **A store whose first sectors are free** (its wear levelling moved its
//!    headers on) → the same.
//! 3. **The split image** (the core that ships), ROM-up over a store → the
//!    same words, `lpfs` byte-identical.
//! 4. **The host half**: the USB flash path's probe reads the same chip as a
//!    tree store, refuses the shipped image (its manifest has no `fs.tree`)
//!    and takes the `fs-tree` image — the real images' own manifests.
//!
//! The rollback itself (a tree core's trial dies, the loader boots the
//! littlefs core) is `emu_split_scenarios.rs`'s S11.
//!
//! Emulated: `lp-emu:esp32c6:t1+net=lan` (direct load; the split case
//! ROM-up). Never hardware-validated. `#[ignore]`d: they need built
//! `fw-esp32c6` ELFs (`LP_EMU_BUILD_FW=1`); `just test-emu-c6-cli-boards`
//! runs them.

#[path = "support/editor_reads.rs"]
mod editor_reads;
#[path = "support/tree_store_board.rs"]
mod tree_store_board;

use editor_reads::block_on;
use lp_cli::commands::emu::link_host::{C6Board, EmuLinkHost};
use lp_emu_esp_common::Strap;
use lp_emu_esp32c6::flash::{
    FlashBacking, LPFS_OFFSET, PARTITION_TABLE_OFFSET, c6_partition_table_bytes,
};
use lp_emu_esp32c6::loader::ResetCause;
use lp_emu_esp32c6::machine::{AppSource, BootMode, Esp32C6Builder, UsbHost};
use lp_emu_esp32c6::test_support::{FwImage, split_image};
use lpa_client::LpClient;
use lpa_link::PartitionTable;
use lpa_link::layout_migration::{
    LayoutDecision, LayoutState, Refusal, decide, embedded_manifest, firmware_reads_tree_store,
    inspect_flash,
};
use tree_store_board::*;

/// The boot word a littlefs core prints when it holds a tree store.
const HOLD_WORD: &str = "[FS] this board's files are in the tree store, a newer filesystem than \
     this firmware reads — not formatting; files kept";

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6-cli-boards` runs it"]
fn a_littlefs_core_holds_a_tree_store_and_writes_nothing() {
    let test = "a_littlefs_core_holds_a_tree_store_and_writes_nothing";
    let Some(shipped) = elf(test, &FwImage::SHIPPED) else {
        return;
    };
    let region = files_region();
    assert!(
        has_header_in_first_two_sectors(&region),
        "the premise: a fresh store's heads are its first sectors"
    );
    held_case("1 store at the front", &shipped, region);
}

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6-cli-boards` runs it"]
fn a_store_with_its_first_sectors_free_is_held_too() {
    let test = "a_store_with_its_first_sectors_free_is_held_too";
    let Some(shipped) = elf(test, &FwImage::SHIPPED) else {
        return;
    };
    let mut region = files_region();
    region.rotate_right(100 * 4096);
    assert!(
        !has_header_in_first_two_sectors(&region),
        "the premise: no header in the first two sectors"
    );
    assert!(
        host_mount(&region).is_ok(),
        "the premise: the moved store is still a whole store"
    );
    held_case("2 store moved on", &shipped, region);
}

#[test]
#[ignore = "needs a split fw-esp32c6 build; `just test-emu-c6-cli-boards` runs it"]
fn the_shipped_split_core_holds_a_tree_store() {
    let split = match split_image(&FwImage::SHIPPED) {
        Ok(split) => split,
        Err(reason) => {
            eprintln!("emu_littlefs_on_tree_store split: skipped — {reason}");
            return;
        }
    };
    let region = files_region();
    let mut merged = std::fs::read(split.merged()).expect("the merged image");
    let at = LPFS_OFFSET as usize;
    if merged.len() < at + region.len() {
        merged.resize(lp_emu_esp32c6::flash::DEFAULT_FLASH_LEN as usize, 0xFF);
    }
    merged[at..at + region.len()].copy_from_slice(&region);
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
        0x7EE1_0003,
        true,
    );
    let hello = boot(&mut host);
    assert_eq!(hello_fs(&hello), "refused", "{hello}");
    assert!(!hello.contains("fs.tree"), "a littlefs build: {hello}");
    let word = boot_word(&host, HOLD_WORD);
    assert!(
        lpfs(&chip(&host)) == &region[..],
        "the split core wrote to the tree store"
    );
    println!("3 split: {word}; lpfs byte-identical");
}

#[test]
#[ignore = "needs built fw-esp32c6 ELFs; `just test-emu-c6-cli-boards` runs it"]
fn the_usb_flash_path_refuses_the_shipped_image_and_takes_a_tree_one() {
    let test = "the_usb_flash_path_refuses_the_shipped_image_and_takes_a_tree_one";
    let (Some(shipped), Some(tree)) = (elf(test, &FwImage::SHIPPED), fs_tree_elf(test)) else {
        return;
    };
    let mut flash = chip_with_lpfs(&{
        let mut region = files_region();
        region.rotate_right(100 * 4096);
        region
    });
    let table = c6_partition_table_bytes();
    let at = PARTITION_TABLE_OFFSET as usize;
    flash[at..at + table.len()].copy_from_slice(&table);
    let target = PartitionTable::parse(&table).expect("the C6's table");
    let inspection = inspect_flash(&flash, target.clone(), true);
    assert_eq!(inspection.state, LayoutState::TreeStore);

    let manifest_of = |elf: &std::path::Path| {
        let bytes = std::fs::read(elf).expect("the ELF");
        // One blob, and no stray marker: a scan finds the FIRST begin marker,
        // and the update guard's own needle once sat in `.rodata` in front of
        // the blob (a package then failed: "not a ManifestCore").
        let begin = lpc_model::manifest::MANIFEST_BLOB_BEGIN.as_bytes();
        let markers = bytes.windows(begin.len()).filter(|w| *w == begin).count();
        assert_eq!(markers, 1, "{}: begin markers", elf.display());
        embedded_manifest([bytes.as_slice()])
    };
    let littlefs = manifest_of(&shipped);
    let reads = manifest_of(&tree);
    assert!(
        littlefs.is_some() && reads.is_some(),
        "both embed a manifest"
    );
    let shipped_reads = firmware_reads_tree_store(littlefs.as_deref());
    let tree_reads = firmware_reads_tree_store(reads.as_deref());
    assert!(!shipped_reads && tree_reads);

    let refused = decide(&inspection, &target, 0x2D_0000, None, false, shipped_reads);
    assert_eq!(refused, Err(Refusal::TreeStoreNotRead));
    let taken = decide(&inspection, &target, 0x2D_0000, None, false, tree_reads);
    assert!(matches!(taken, Ok(LayoutDecision::Plain(_))), "{taken:?}");
    println!(
        "4 host: TreeStore; shipped image → {}; fs-tree image → plain flash",
        Refusal::TreeStoreNotRead
    );
}

// ---- helpers ---------------------------------------------------------------

/// Boot the littlefs image over `region`; it must hold, say so, lock the
/// device store, refuse a network write in its words, keep answering, and
/// leave `lpfs` untouched — across a power cycle too.
fn held_case(label: &str, elf: &std::path::Path, region: Vec<u8>) {
    let (before, _) = host_mount(&region).expect("the premise: the host mounts the store");
    let mut host = board(elf, chip_with_lpfs(&region), 0x7EE1_0001);
    let hello = boot(&mut host);
    assert_eq!(hello_fs(&hello), "refused", "{hello}");
    assert!(!hello.contains("fs.tree"), "a littlefs build: {hello}");
    let word = boot_word(&host, HOLD_WORD);
    assert!(
        line_with(&host, "Formatted and mounted fresh filesystem").is_none(),
        "{}",
        host.console().join("\n")
    );
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
    let (network, loaded) = {
        let mut client = LpClient::new(&mut host).with_request_ids_from(100);
        let network = block_on(client.network_add(
            "lab".to_string(),
            lpc_wire::WifiPassword::new("not-a-real-one"),
            None,
        ));
        (network, block_on(client.project_list_loaded()))
    };
    let refusal = format!("{network:?}");
    assert!(
        refusal.contains("update the firmware"),
        "a network write is refused in the held board's words: {refusal}"
    );
    assert!(loaded.is_ok(), "the link still answers: {loaded:?}");
    assert!(
        lpfs(&chip(&host)) == &region[..],
        "{label}: the held partition was written"
    );

    let mut host = power_cycle(host, 0x7EE1_0011);
    let hello = boot(&mut host);
    assert_eq!(hello_fs(&hello), "refused", "after a power cycle: {hello}");
    assert!(
        lpfs(&chip(&host)) == &region[..],
        "{label}: written after a power cycle"
    );
    let (after, _) = host_mount(lpfs(&chip(&host))).expect("the host still mounts the store");
    assert_eq!(after, before, "every file is still on the store");
    println!(
        "{label}: {word}; {ble}; network write refused; link answers; lpfs byte-identical \
         across a power cycle; {} file(s) mount on the host",
        after.len()
    );
}

/// A store region holding a few files (and a hot panel file), built on the
/// host as an `fs-tree` core would have left it.
fn files_region() -> Vec<u8> {
    host_store(&[
        ("/hardware.json", b"{\"board\":\"kept\"}"),
        ("/projects/kept/project.json", b"{\"name\":\"kept\"}"),
        ("/projects/kept/.lp/panel.json", b"{\"version\":1}"),
    ])
}

fn has_header_in_first_two_sectors(region: &[u8]) -> bool {
    [0usize, 4096]
        .iter()
        .any(|&at| region[at..at + 4] == *b"LTS1")
}

/// The boot word containing `needle`, or a panic with the console.
fn boot_word(host: &EmuLinkHost<C6Board>, needle: &str) -> String {
    line_with(host, needle)
        .unwrap_or_else(|| panic!("no {needle:?}:\n{}", host.console().join("\n")))
        .clone()
}
