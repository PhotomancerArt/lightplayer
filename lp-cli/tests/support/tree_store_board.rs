//! An emulated C6 running the `fs-tree` firmware, for the tree-store walks
//! (plan `lp2025/2026-10-08-2339-tree-store-firmware-and-emulator`, P6–P8):
//! build a board on a given flash, boot it to its hello, read its boot
//! words, power-cycle it, read its flash out, and mount that flash on the
//! host with `lp-tree-store` — the second, independent reading of what the
//! board stored. `lp-tree-store` is AGPL and stays on this side of the
//! fence: `lp-emu/` never sees it.
//!
//! Every wait is on the board's own words (`[FS] …`, the hello), with an
//! emulated-time budget; a host wall clock is only the link host's net.

#![allow(dead_code, reason = "each test file uses part of this")]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use lp_cli::commands::emu::link_host::{C6Board, EmuLinkHost};
use lp_emu_esp_common::Strap;
use lp_emu_esp32c6::flash::{DEFAULT_FLASH_LEN, FlashBacking, LPFS_LEN, LPFS_OFFSET};
use lp_emu_esp32c6::machine::{AppSource, Esp32C6Builder, TimeGrade, UsbHost};
use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image};
use lp_nor_sim::{NorFlashSim, NorGeometry};
use lp_tree_store::{MountSummary, SoftSha256, StoreConfig, StoreError, TreeStore};

/// Emulated microseconds a boot may take to its hello (a wedge detector).
pub const BOOT_BUDGET_US: u64 = 20_000_000;

/// The boot banner: one per power-on.
pub const BOOT_BANNER: &str = "[INIT] Initializing board...";

/// A file set: absolute path → bytes.
pub type Files = BTreeMap<String, Vec<u8>>;

/// The `fs-tree` image (or `None` with a skip notice).
pub fn fs_tree_elf(test: &str) -> Option<PathBuf> {
    elf(test, &FwImage::FS_TREE)
}

/// Any image (or `None` with a skip notice).
pub fn elf(test: &str, image: &FwImage) -> Option<PathBuf> {
    match fw_esp32c6_image(image) {
        Ok(path) => Some(path),
        Err(reason) => {
            eprintln!("{test}: skipped — {reason}");
            None
        }
    }
}

/// A board on `flash` (a whole chip, or empty for a blank one), not yet
/// stepped. Direct load; a chip without a partition table gets the C6's.
pub fn board(elf: &Path, flash: Vec<u8>, nonce: u32) -> EmuLinkHost<C6Board> {
    let backing = if flash.is_empty() {
        FlashBacking::Blank
    } else {
        FlashBacking::Bytes(flash)
    };
    let machine = Esp32C6Builder::new()
        .app(AppSource::Path(elf.to_path_buf()))
        .flash(backing)
        .strict(false)
        .time_grade(TimeGrade::T1)
        .usb_host(UsbHost::Attached { draining: true })
        .usb_sj_queue_source()
        // The power-on snapshot a power cycle (and a cut) restores.
        .reboot_on_reset(true)
        .build()
        .expect("the image builds a machine");
    EmuLinkHost::new(C6Board::new(machine).expect("a hosted board"), nonce, true)
}

/// Step `host` to its hello; the hello's line. Panics with the console if it
/// never comes.
pub fn boot(host: &mut EmuLinkHost<C6Board>) -> String {
    match host.wait_for_line("\"hello\":{", BOOT_BUDGET_US) {
        Ok(Some(line)) => line,
        Ok(None) => panic!("no hello:\n{}", host.console().join("\n")),
        Err(e) => panic!("the boot failed ({e}):\n{}", host.console().join("\n")),
    }
}

/// Power-cycle the board (both domains to power-on, the flash as it is) and
/// put a fresh host on its link. Not stepped.
pub fn power_cycle(host: EmuLinkHost<C6Board>, nonce: u32) -> EmuLinkHost<C6Board> {
    let mut board = host.board;
    assert!(board.machine.power_cycle(Strap::App), "a power-on snapshot");
    EmuLinkHost::new(board, nonce, true)
}

/// The hello's `fs` value (`mounted`, `formatted`, `refused`, …).
pub fn hello_fs(hello: &str) -> String {
    let at = hello.find("\"fs\":\"").expect("the hello carries fs") + 6;
    let end = hello[at..].find('"').expect("a closed string");
    hello[at..at + end].to_string()
}

/// The first console line containing `needle`.
pub fn line_with<'a>(host: &'a EmuLinkHost<C6Board>, needle: &str) -> Option<&'a String> {
    host.console().iter().find(|l| l.contains(needle))
}

/// How many boots the console shows.
pub fn boots(host: &EmuLinkHost<C6Board>) -> usize {
    host.console()
        .iter()
        .filter(|line| line.contains(BOOT_BANNER))
        .count()
}

/// The whole chip as it is now.
pub fn chip(host: &EmuLinkHost<C6Board>) -> Vec<u8> {
    host.board.machine.flash().lock().unwrap().bytes().to_vec()
}

/// The `lpfs` partition of a chip.
pub fn lpfs(chip: &[u8]) -> &[u8] {
    &chip[LPFS_OFFSET as usize..(LPFS_OFFSET + LPFS_LEN) as usize]
}

/// A chip of the C6's size: blank, with `region` written at `lpfs`.
pub fn chip_with_lpfs(region: &[u8]) -> Vec<u8> {
    let mut chip = vec![0xFFu8; DEFAULT_FLASH_LEN as usize];
    let at = LPFS_OFFSET as usize;
    chip[at..at + region.len()].copy_from_slice(region);
    chip
}

/// `region` (a whole `lpfs`) as a NOR model the host store can mount.
pub fn nor_of(region: &[u8]) -> NorFlashSim {
    let sectors = (region.len() / 4096) as u32;
    let mut f = NorFlashSim::new(NorGeometry::c6(sectors));
    f.set_panic_on_violation(false);
    for (s, chunk) in region.chunks(4096).enumerate() {
        if chunk.iter().any(|b| *b != 0xFF) {
            f.program(s as u32 * 4096, chunk).expect("program the copy");
        }
    }
    f
}

/// The region's bytes back out of a NOR model.
pub fn region_of(f: &NorFlashSim) -> Vec<u8> {
    let mut out = vec![0u8; (f.geometry().sector_count * 4096) as usize];
    f.peek(0, &mut out);
    out
}

/// Mount `region` on the host with the firmware's configuration: its files
/// and summary, or the mount's error.
pub fn host_mount(
    region: &[u8],
) -> Result<(Files, MountSummary), StoreError<lp_nor_sim::NorError>> {
    let mut st = TreeStore::mount(nor_of(region), SoftSha256, StoreConfig::default())
        .map_err(|(e, ..)| e)?;
    let summary = st.summary();
    let mut files = Files::new();
    for path in st.list("/")? {
        let bytes = st.get(&path)?.expect("a listed file");
        files.insert(path, bytes);
    }
    Ok((files, summary))
}

/// A store region built on the host, with `files` committed (one commit
/// each), at the C6's 176 sectors.
pub fn host_store(files: &[(&str, &[u8])]) -> Vec<u8> {
    let sectors = LPFS_LEN / 4096;
    let mut st = match TreeStore::format(
        NorFlashSim::new(NorGeometry::c6(sectors)),
        SoftSha256,
        StoreConfig::default(),
    ) {
        Ok(st) => st,
        Err((e, ..)) => panic!("format: {e:?}"),
    };
    for (path, bytes) in files {
        st.put(path, bytes).expect("put");
    }
    region_of(st.flash())
}

/// A project directory as the upload's `(relative path, bytes)` list, in
/// path order (as `lp-cli upload` sends it).
pub fn project_files(relative: &str) -> Vec<(String, Vec<u8>)> {
    let root = repo_root().join(relative);
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

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lp-cli sits under the repo root")
        .to_path_buf()
}
