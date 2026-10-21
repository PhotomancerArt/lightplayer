//! The ESP32-S3's split image (`lp-fw-split build --chip esp32s3`: loader,
//! boot records, core, engine; layout 2), booted from the reset vector on
//! `lp-emu:esp32s3:t1` through the real mask ROM and espflash's bundled IDF
//! bootloader — the IDF door, the S3's only one — with lp-cli as the host on
//! its USB link. The C6's twin is `emu_split_boot.rs`.
//!
//! - the packaged image (core low, as flashed) boots ROM → IDF → loader →
//!   core → engine, and the engine says hello on the link;
//! - the same pieces with the core at the region's high end and the engine
//!   at its start (where every second update leaves them) boot the same way;
//! - a project pushed over the link is compiled by the engine's own JIT and
//!   rendered: frames decode off the LED pad.
//!
//! `#[ignore]`d: it needs a split build, named by `LP_EMU_S3_SPLIT` (the
//! directory `lp-fw-split build --out` wrote). `just test-emu-esp32s3-split-boot`
//! builds it and runs this. Numbers printed are `lp-emu:esp32s3:t1`.

use std::path::{Path, PathBuf};

use lp_bootctl::{BOOT_RECORD_SECTORS, BootRecord, LAYOUT_2_PAGE, LAYOUT_2_REGION_START};
use lp_cli::commands::emu::link_host::{EmuLinkHost, S3Board};
use lp_emu_esp32s3::flash::FlashBacking;
use lp_emu_esp32s3::machine::{AppSource, BootMode, Esp32S3Builder, UsbHost};

/// One fixed host nonce, so a run is a function of the image.
const NONCE: u32 = 0x5911_7C53;
/// The boot to the engine's hello: the ROM, the IDF bootloader's segment
/// loads and hash, the loader, the core, the engine (~0.2 s emulated; the
/// spike measured the hello at 0.196 s).
const BOOT_US: u64 = 3_000_000;

#[test]
#[ignore = "needs a split fw-esp32s3 build; `just test-emu-esp32s3-split-boot` runs it"]
fn the_packaged_image_boots_through_the_idf_door_to_the_engines_hello() {
    let Some(split) = split_dir() else { return };
    let report = report(&split);
    let flash = std::fs::read(split.join("merged.bin")).unwrap();
    let engine_off = report["engine"]["offset"].as_u64().unwrap();
    let engine_len = report["engine"]["sizeBytes"].as_u64().unwrap();
    let core_off = report["core"]["offset"].as_u64().unwrap();
    assert_eq!(report["chip"], "esp32s3");
    assert_eq!(report["layout"], 2);
    assert_eq!(
        core_off,
        u64::from(LAYOUT_2_REGION_START),
        "the core low, as flashed"
    );

    let console = boot_to_hello(&split, flash);
    assert!(
        console.contains(&format!("[LOADER] core @{core_off:#x} (proven)")),
        "the loader booted record 0's core:\n{console}"
    );
    let digest = &report["engine"]["sha256"].as_str().unwrap()[..8];
    let line =
        format!("[CORE] engine @{engine_off:#x} ({engine_len} B, digest {digest}), entering");
    assert!(console.contains(&line), "no {line:?} in:\n{console}");
    println!("split boot (lp-emu:esp32s3:t1): {line}");
}

#[test]
#[ignore = "needs a split fw-esp32s3 build; `just test-emu-esp32s3-split-boot` runs it"]
fn a_core_at_the_regions_high_end_maps_the_engine_below_it() {
    let Some(split) = split_dir() else { return };
    let report = report(&split);
    let core = std::fs::read(split.join("core.bin")).unwrap();
    let engine = std::fs::read(split.join("engine.bin")).unwrap();
    let region_end = report["regionEnd"].as_u64().unwrap() as u32;
    let build = record_0_build(&std::fs::read(split.join("app.bin")).unwrap());

    // The flashed chip with its region rewritten: the core at the highest
    // page that holds it, the engine at the region's start, record 0 naming
    // the high core (what an update leaves, with the old pieces gone).
    let mut flash = std::fs::read(split.join("merged.bin")).unwrap();
    let high = (region_end - core.len() as u32) / LAYOUT_2_PAGE * LAYOUT_2_PAGE;
    assert!(LAYOUT_2_REGION_START + engine.len() as u32 <= high);
    flash[LAYOUT_2_REGION_START as usize..region_end as usize].fill(0xff);
    let put = |flash: &mut Vec<u8>, at: u32, bytes: &[u8]| {
        flash[at as usize..at as usize + bytes.len()].copy_from_slice(bytes);
    };
    put(&mut flash, high, &core);
    put(&mut flash, LAYOUT_2_REGION_START, &engine);
    let sector = BOOT_RECORD_SECTORS[0] as usize;
    flash[sector..sector + 0x1000].fill(0xff);
    let record = BootRecord {
        seq: 1,
        core_off: high,
        core_len: core.len() as u32,
        build,
        trial: false,
    };
    put(&mut flash, BOOT_RECORD_SECTORS[0], &record.encode());

    let console = boot_to_hello(&split, flash);
    assert!(
        console.contains(&format!("[LOADER] core @{high:#x} (proven)")),
        "the loader booted the high core:\n{console}"
    );
    let line = format!(
        "[CORE] engine @{LAYOUT_2_REGION_START:#x} ({} B,",
        engine.len()
    );
    assert!(console.contains(&line), "no {line:?} in:\n{console}");
    println!("split boot, core high (lp-emu:esp32s3:t1): core @{high:#x}, {line}");
}

#[test]
#[ignore = "needs a split fw-esp32s3 build; `just test-emu-esp32s3-split-boot` runs it"]
fn a_pushed_project_is_compiled_by_the_engine_and_rendered() {
    let Some(split) = split_dir() else { return };
    let tmp = tempfile::tempdir().unwrap();
    let frames = tmp.path().join("frames.jsonl");
    let project = repo_root().join("projects/test/basic");
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_lp-cli"))
        .args(["emu", "run", "--chip", "esp32s3", "--host-link", "--merged"])
        .arg(split.join("merged.bin"))
        .arg("--upload")
        .arg(&project)
        .arg("--dump-frames")
        .arg(&frames)
        .args(["--timeout", "2s"])
        .status()
        .expect("running lp-cli emu run");
    assert!(status.success(), "lp-cli emu run: {status}");
    let text = std::fs::read_to_string(&frames).expect("frames were dumped");
    let wires: Vec<serde_json::Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).expect("a frame record"))
        .collect();
    assert!(wires.len() >= 2, "frames rendered: {}", wires.len());
    assert!(
        wires.iter().all(|f| f["leds"] == 241),
        "the basic project's strip"
    );
    let distinct: std::collections::HashSet<_> =
        wires.iter().map(|f| f["wire"].as_str().unwrap()).collect();
    assert!(
        distinct.len() >= 2,
        "the shader animates: {} distinct",
        distinct.len()
    );
    println!(
        "split render (lp-emu:esp32s3:t1): {} frames, {} distinct",
        wires.len(),
        distinct.len()
    );
}

/// The split build named by `LP_EMU_S3_SPLIT`, or `None` (a skip, said).
fn split_dir() -> Option<PathBuf> {
    match std::env::var_os("LP_EMU_S3_SPLIT") {
        Some(dir) => Some(PathBuf::from(dir)),
        None => {
            eprintln!(
                "emu_s3_split_boot: skipped — LP_EMU_S3_SPLIT names no split build \
                 (`just test-emu-esp32s3-split-boot`)"
            );
            None
        }
    }
}

fn report(split: &Path) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(split.join("split.json")).unwrap()).unwrap()
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Boot `flash` from the reset vector, attached and draining from power-on,
/// with this process as the host, until the engine's hello; the console.
fn boot_to_hello(split: &Path, flash: Vec<u8>) -> String {
    let len = flash.len() as u32;
    let builder = Esp32S3Builder::new()
        .boot_mode(BootMode::RomUp)
        .app(AppSource::Path(split.join("p2.elf")))
        .flash(FlashBacking::Bytes(flash))
        .flash_len(len);
    let board = S3Board::build(builder, UsbHost::Attached { draining: true })
        .expect("the split image builds a machine");
    let mut host = EmuLinkHost::new(board, NONCE, true);
    let hello = host.wait_for_line("\"hello\":{", BOOT_US).expect("the run");
    let console = host.console().join("\n");
    assert!(hello.is_some(), "no hello on the link:\n{console}");
    assert!(
        console.contains("\"boardId\":\"seeed/xiao-esp32-s3-plus\""),
        "the engine's hello names the board:\n{console}"
    );
    assert_eq!(host.link_errors, 0, "{console}");
    console
}

/// The `build` word of record 0 in an `app.bin` (which starts at the
/// loader's offset): what the high core's record must carry too.
fn record_0_build(app: &[u8]) -> u32 {
    let at = (BOOT_RECORD_SECTORS[0] - lp_bootctl::LOADER_OFFSET) as usize;
    let slot = lp_bootctl::BootSlot::decode(&app[at..at + lp_bootctl::BOOT_RECORD_READ_LEN])
        .expect("app.bin's record 0");
    slot.record.build
}
