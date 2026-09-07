//! The shipped `fw-esp32c6` image (default features: `esp32c6,server,radio`),
//! direct-loaded and run.
//!
//! `#[ignore]`d: needs the shipped ELF (`just test-emu-c6` builds it, or
//! set `LP_EMU_C6_ELF_ESP32C6_SERVER_RADIO`). See `test_support`.
//!
//! In P5 the shipped image gets exactly as far as the plain no-radio one:
//! `bootctl::read_and_consume` reads flash before anything radio-shaped
//! runs, and a flash read is the ROM's `esp_rom_spiflash_read` spinning on
//! `SPI1.cmd` until M4 models the controller. The radio window
//! (`0x600A_0000..0x600A_9800`, left unmapped for P6's stub) is therefore
//! not reached yet; the test pins the order — flash first — so the day M4
//! lands, this test is the one that moves to the radio window.

use lp_emu_esp_common::Trace;
use lp_emu_esp_common::trace::SharedBuffer;

use lp_emu_esp32c6::machine::{AppSource, Esp32C6Builder, Esp32C6Machine, Outcome, StopCondition};
use lp_emu_esp32c6::memmap;
use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image, skip_notice};

/// Build a machine on the shipped image, or `None` with a printed reason.
fn shipped(strict: bool, buf: &SharedBuffer) -> Option<Esp32C6Machine> {
    let elf = match fw_esp32c6_image(&FwImage::SHIPPED) {
        Ok(path) => path,
        Err(reason) => {
            skip_notice("boot", &reason);
            return None;
        }
    };
    Some(
        Esp32C6Builder::new()
            .app(AppSource::Path(elf))
            .strict(strict)
            .trace(Box::new(buf.clone()), vec!["INTPRI".to_string()])
            .build()
            .expect("the shipped image builds a machine"),
    )
}

#[test]
#[ignore = "needs the fw-esp32c6 ELF; run through `just test-emu-c6`"]
fn the_shipped_image_boots_to_the_flash_read_which_is_m4s_and_never_the_radio_window() {
    let buf = SharedBuffer::new();
    let Some(mut m) = shipped(true, &buf) else {
        return;
    };
    // Only SPIN/UNMAPPED lines: the filter names no block.
    m.bus.trace = Trace::to_sink(Box::new(buf.clone())).with_block_filter(["NOTHING"]);

    let outcome = m.run_until(&StopCondition::after_micros(30_000));
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "no strict stop: every block it touches is mapped ({outcome:?})"
    );
    assert_eq!(m.bus.unmapped_reads() + m.bus.unmapped_writes(), 0);
    let spins: Vec<String> = buf
        .lines()
        .into_iter()
        .filter(|l| l.contains(" SPIN "))
        .collect();
    assert!(
        spins
            .iter()
            .any(|l| l.contains("SPIN SPI1+0x000 cmd = 0x10000000")),
        "the flash read's spin on SPI1.cmd: {spins:?}"
    );
    assert_eq!(m.idle_skips(), 0, "stuck in the flash read, never idle");
    // The radio window was not touched — it comes after the flash read.
    assert!(
        !buf.lines().iter().any(|l| l.contains("UNMAPPED+0x600a")),
        "the radio window is reached before the flash read?"
    );
}

#[test]
#[ignore = "needs the fw-esp32c6 ELF; run through `just test-emu-c6`"]
fn the_shipped_image_loads_where_the_memory_map_says() {
    let buf = SharedBuffer::new();
    let Some(m) = shipped(false, &buf) else {
        return;
    };

    // The ROM first, then the app on top of it.
    assert_eq!(m.rom_segments().len(), 4);
    assert_eq!(m.app_segments().len(), 7);

    let app = m.app().expect("an app was loaded");
    assert_eq!(
        m.symbolize(app.entry).as_deref(),
        Some("_start"),
        "the entry point is `_start`, inside a placed segment"
    );

    // The three regions the app spans, and nothing outside them.
    for seg in m.app_segments() {
        assert!(
            seg.regions
                .iter()
                .all(|r| matches!(*r, "hp-sram" | "flash-cache" | "lp-sram")),
            "segment at {:#010x} landed in {:?}",
            seg.vaddr,
            seg.regions
        );
    }

    // `dram2_seg` is the second heap region and it is plain zeroed RAM: the
    // bootloader's loader segment is gone, which is the whole point.
    let mut m = m;
    assert_eq!(m.peek_word(memmap::DRAM2_BASE), Some(0));
    assert_eq!(m.peek_word(memmap::DRAM2_END - 4), Some(0));
    // The stack top is where the linker script puts it.
    assert_eq!(memmap::APP_RAM_END, 0x4086_E610);
}
