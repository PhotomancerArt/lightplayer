//! The split image's boot, heard by a host on its link: the ESP32-C6
//! product image (`lp-fw-split`: loader, boot records, core, engine) booted
//! from the reset vector on `lp-emu:esp32c6`, with lp-cli's in-process host
//! on its USB link.
//!
//! The emulator's own gate (`lp-emu-esp32c6/tests/split_boot.rs`) proves the
//! boot chain up to the engine's raw server-loop line and that the direct
//! load agrees with ROM-up. What rides lp-link past the boot text — the
//! core's one boot-state line on the log channel and the engine's hello —
//! only a product crate may host (the MIT fence), so it is read here.
//!
//! `#[ignore]`d: it needs a split build (`LP_EMU_BUILD_FW=1`, or
//! `LP_EMU_C6_SPLIT_ESP32C6_SERVER_RADIO`), and `just test-emu-c6-cli` runs
//! it. Numbers printed are `lp-emu:esp32c6:t1`.

use lp_cli::commands::emu::link_host::{C6Board, EmuLinkHost};
use lp_emu_esp_common::Strap;
use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::loader::ResetCause;
use lp_emu_esp32c6::machine::{AppSource, BootMode, Esp32C6Builder, UsbHost};
use lp_emu_esp32c6::test_support::{FwImage, SplitImage, split_image};

/// One fixed host nonce, so a run is a function of the image.
const NONCE: u32 = 0x5911_7C06;

#[test]
#[ignore = "needs a split fw-esp32c6 build; `just test-emu-c6-cli` runs it"]
fn the_core_states_its_boot_and_the_engine_says_hello() {
    let split = match split_image(&FwImage::SHIPPED) {
        Ok(split) => split,
        Err(reason) => {
            eprintln!("emu_split_boot: skipped — {reason}");
            return;
        }
    };
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(split.split_json()).unwrap()).unwrap();
    let mut host = hosted(&split);
    host.run_until(3_000_000, None).expect("the run");
    let console = host.console().join("\n");

    // The core's boot-state line, every field from the build's own report:
    // the flashed core (proven, record 0's), the loader that started it, the
    // region end the partition table gave, the MMU page, the engine the
    // header admitted, and the digest slot the packager filled.
    let int = |key: &[&str]| -> u64 {
        key.iter()
            .fold(&report, |v, k| &v[*k])
            .as_u64()
            .unwrap_or_else(|| panic!("split.json has no {key:?}"))
    };
    let engine_sha = report["engine"]["sha256"].as_str().unwrap();
    let core_line = format!(
        "[CORE] core @{:#x} +{} (proven) build {} · loader v{} · region end {:#x} · page {:#x} · \
         engine {} B · digest {}",
        int(&["core", "offset"]),
        int(&["core", "sizeBytes"]),
        report["buildId"].as_str().unwrap(),
        int(&["loaderVersion"]),
        int(&["regionEnd"]),
        int(&["page"]),
        int(&["engine", "sizeBytes"]),
        &engine_sha[..8],
    );
    let core_at = console
        .find(&core_line)
        .unwrap_or_else(|| panic!("no {core_line:?} in:\n{console}"));
    assert!(
        !console.contains("[CORE] boot state not trusted"),
        "the flashed image's boot state is trusted:\n{console}"
    );
    let hello = "M!{\"id\":0,\"msg\":{\"hello\":{\"proto\":";
    let hello_at = console
        .find(hello)
        .unwrap_or_else(|| panic!("no hello in:\n{console}"));
    assert!(core_at < hello_at, "the core spoke before the engine");
    assert!(
        console[hello_at..].contains("\"boardId\":\"seeed/xiao-esp32-c6\""),
        "the engine's hello names the board"
    );
    assert_eq!(host.link_errors, 0, "{console}");
    println!(
        "split boot (lp-emu:esp32c6:t1): {core_line}; hello after {} console lines",
        console[..hello_at].lines().count()
    );
}

/// The split image booted from the reset vector, attached and draining from
/// power-on, with this process as the host on its link.
fn hosted(split: &SplitImage) -> EmuLinkHost<C6Board> {
    let len = std::fs::metadata(split.merged()).unwrap().len() as u32;
    let machine = Esp32C6Builder::new()
        .boot_mode(BootMode::RomUp)
        .app(AppSource::Path(split.p2_elf()))
        .flash(FlashBacking::Copy(split.merged()))
        .flash_len(len)
        .reset_cause(ResetCause::UsbUartHpSys)
        .strap(Strap::App)
        .usb_host(UsbHost::Attached { draining: true })
        .usb_sj_queue_source()
        .build()
        .expect("the split image builds a machine");
    EmuLinkHost::new(C6Board::new(machine).expect("a hosted board"), NONCE, true)
}
