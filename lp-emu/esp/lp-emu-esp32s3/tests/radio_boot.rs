//! S3 radio M1's gate (AC1): **an image that starts the radio boots on this
//! machine, both ways, with no unmapped stop.**
//!
//! The image is `fw-esp32s3 --features radio_boot_probe`: the shipped boot
//! with esp-radio's Wi‑Fi (`wifi::new`, a station, one scan) and the BTDM
//! Bluetooth controller (`BleConnector::new`, one HCI Reset) started beside
//! it. Every window those blobs touch that the PAC does not name is a
//! [`RadioStub`] here (`src/periph/radio_stub.rs`); this file holds that the
//! set is whole — a strict run reaches the server loop, the controller
//! answers its HCI Reset, the scan finishes, and nothing was refused —
//! and that every window it declares was reached, so a window that stops
//! being needed is noticed rather than carried.
//!
//! What it does not claim: any radio *behaviour*. Nothing here receives or
//! transmits; the scan finishes with no access points because no air is
//! modelled, and the controller's answer to a Reset is the controller's own
//! (a ROM routine on a register file), not a radio's. That the same frame
//! renders with the radio on is `lp-cli/tests/emu_s3_link_gates.rs`'s
//! claim, because a render needs a project pushed over the link, and a link
//! host is a product crate.
//!
//! `#[ignore]`d for the usual reason (`lp_emu_esp32s3::test_support`): a
//! plain `cargo test --workspace` must never start a cross-target firmware
//! build. `just test-emu-esp32s3-boot` builds the radio image, merges it and
//! names both files.

use lp_emu_esp32s3::flash::FlashBacking;
use lp_emu_esp32s3::intmatrix::{ESP_HAL_DISABLED_CPU_INTERRUPT, Esp32S3IntMatrix};
use lp_emu_esp32s3::machine::{
    AppSource, BootMode, Esp32S3Builder, Machine, Outcome, StopCondition, UsbHost,
};
use lp_emu_esp32s3::periph::radio_stub::{RadioStub, WINDOWS};
use lp_emu_esp32s3::regs::INTERRUPT_SOURCES;
use lp_emu_esp32s3::test_support::{radio_image, radio_merged_image, skip_notice};

/// Direct load to the server loop, the HCI event and the scan's end, with
/// the PHY calibration in between.
const DIRECT_US: u64 = 1_500_000;

/// The same after the ROM and the IDF bootloader (~2.7 s on the shipped
/// image to its first frame).
const ROM_UP_US: u64 = 4_500_000;

/// The lines a radio boot must print: the probe's last stage, the server
/// loop, the controller's Command Complete for the Reset (`04 0e 04 05 03
/// 0c 00`: event 0x0e, 4 bytes, 5 packets, opcode 0x0c03, status 0) and the
/// scan's end.
const RADIO_BOOT_LINES: &[&str] = &[
    "[RADIO] stage=answers_task_spawned ",
    "[INIT] fw-esp32 initialized, starting server loop",
    "[RADIO] ble HCI event: [04, 0e, 04, 05, 03, 0c, 00]",
    "[RADIO] wifi scan done: 0 access points",
];

#[test]
#[ignore = "needs LP_EMU_ESP32S3_RADIO_ELF and LP_EMU_ESP32S3_RADIO_MERGED; run through `just test-emu-esp32s3-boot`"]
fn the_radio_image_boots_to_the_server_by_direct_load() {
    let (elf, merged) = match (radio_image(), radio_merged_image()) {
        (Ok(e), Ok(m)) => (e, m),
        (Err(r), _) | (_, Err(r)) => {
            skip_notice("the_radio_image_boots_to_the_server_by_direct_load", &r);
            return;
        }
    };
    let mut m = Esp32S3Builder::new()
        .app(AppSource::Path(elf))
        .flash(FlashBacking::Copy(merged))
        .strict(true)
        .usb_host(UsbHost::Attached { draining: true })
        .build()
        .expect("the radio image direct-loads");
    let outcome = m.run_until(&StopCondition::after_micros(DIRECT_US));
    holds_a_radio_boot("direct", &mut m, &outcome);
}

#[test]
#[ignore = "needs LP_EMU_ESP32S3_RADIO_MERGED; run through `just test-emu-esp32s3-boot`"]
fn the_radio_image_boots_to_the_server_rom_up() {
    let merged = match radio_merged_image() {
        Ok(m) => m,
        Err(r) => {
            skip_notice("the_radio_image_boots_to_the_server_rom_up", &r);
            return;
        }
    };
    let len = std::fs::metadata(&merged).expect("the merged image").len() as u32;
    let mut b = Esp32S3Builder::new()
        .boot_mode(BootMode::RomUp)
        .flash(FlashBacking::Copy(merged))
        .flash_len(len)
        .strict(true)
        .usb_host(UsbHost::Attached { draining: true });
    if let Ok(elf) = radio_image() {
        b = b.app(AppSource::Path(elf));
    }
    let mut m = b.build().expect("the ROM-up machine builds");
    let outcome = m.run_until(&StopCondition::after_micros(ROM_UP_US));
    holds_a_radio_boot("rom-up", &mut m, &outcome);
}

/// The whole claim, the same on both paths.
fn holds_a_radio_boot(path: &str, m: &mut Machine, outcome: &Outcome) {
    let text = String::from_utf8_lossy(&m.usb_sj()).into_owned();
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "{path}: the run must reach its deadline, not stop: {outcome:?}\n{text}"
    );
    assert!(
        m.first_strict_violation().is_none(),
        "{path}: a strict stop: {:?}",
        m.first_strict_violation()
    );
    assert_eq!(
        m.bus().unmapped_reads() + m.bus().unmapped_writes(),
        0,
        "{path}: unmapped accesses"
    );
    for line in RADIO_BOOT_LINES {
        assert!(text.contains(line), "{path}: no `{line}` in:\n{text}");
    }
    assert!(!text.contains("PANIC"), "{path}: a panic:\n{text}");

    // The radio's interrupt sources (0..=10: WIFI_MAC … RWBLE_NMI) that the
    // blobs mapped to a CPU interrupt on core 0 — esp-hal's `setup_interrupts`
    // disables every map register at boot (writes 16), so a source not left
    // at 16 is one the radio claimed. Nothing raises them (M5's).
    let matrix = m
        .bus()
        .matrix()
        .as_any()
        .downcast_ref::<Esp32S3IntMatrix>()
        .expect("the S3's matrix");
    let claimed: Vec<(u16, &str, u32)> = INTERRUPT_SOURCES
        .iter()
        .filter(|(n, _)| *n <= 10)
        .filter_map(|(n, name)| {
            let to = matrix.map(0, usize::from(*n))?;
            (to != ESP_HAL_DISABLED_CPU_INTERRUPT).then_some((*n, *name, to))
        })
        .collect();
    println!("{path}: radio sources mapped on core 0 (source, name, cpu interrupt): {claimed:?}");
    // Read off this image on 2026-10-21 (all three to CPU interrupt 1, the
    // S3 Wi-Fi ISR's line esp-radio forces): Wi-Fi's MAC and PWR, and the
    // BLE link layer. A change is the blobs claiming something else — read
    // it, then move this list.
    assert_eq!(
        claimed.iter().map(|(n, _, _)| *n).collect::<Vec<_>>(),
        [0, 2, 8],
        "{path}: the radio's claimed sources: {claimed:?}"
    );

    let mut touched = Vec::new();
    for window in WINDOWS {
        let index = m
            .bus()
            .peripheral_index(window.name)
            .unwrap_or_else(|| panic!("{} is registered", window.name));
        let n = m
            .bus_mut()
            .with_peripheral::<RadioStub, _>(index, |stub, _| stub.touched().len())
            .unwrap_or_else(|| panic!("{} is a RadioStub", window.name));
        touched.push((window.name, n));
    }
    println!("{path}: distinct offsets touched per radio window (lp-emu:esp32s3:t1): {touched:?}");
    for (window, (name, n)) in WINDOWS.iter().zip(&touched) {
        if window.reached_by_the_probe {
            assert!(
                *n > 0,
                "{path}: the radio window {name} says the probe reaches it and it was never \
                 reached — a window with no evidence behind it: {touched:?}"
            );
        } else {
            assert_eq!(
                *n, 0,
                "{path}: {name} is marked census-only and the probe reached it — update \
                 `reached_by_the_probe` and its doc: {touched:?}"
            );
        }
    }
}
