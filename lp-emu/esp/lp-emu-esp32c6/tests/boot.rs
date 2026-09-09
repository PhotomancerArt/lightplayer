//! The shipped `fw-esp32c6` image (default features: `esp32c6,server,radio`),
//! direct-loaded and run.
//!
//! `#[ignore]`d: needs the shipped ELF (`just test-emu-c6` builds it, or
//! set `LP_EMU_C6_ELF_ESP32C6_SERVER_RADIO`). See `test_support`.
//!
//! P5 pinned the order: `bootctl::read_and_consume` reads flash before
//! anything radio-shaped runs, so the shipped image got exactly as far as
//! the no-radio one — the ROM's `esp_rom_spiflash_read` spinning on
//! `SPI1.cmd` — and the test said "the day M4 lands, this test is the one
//! that moves to the radio window".
//!
//! **M4 landed.** The flash read completes, the boot-control sector reads
//! as erased, the radio window is reached and P6's stub answers it, and the
//! image runs to the idle loop. This test now pins that whole order in one
//! run: flash first, radio after, idle at the end.

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
fn the_shipped_image_reads_flash_then_the_radio_window_then_idles() {
    let buf = SharedBuffer::new();
    let Some(mut m) = shipped(true, &buf) else {
        return;
    };
    // Only SPIN/UNMAPPED lines: the filter names no block.
    m.bus.trace = Trace::to_sink(Box::new(buf.clone())).with_block_filter(["NOTHING"]);

    let outcome = m.run_until(&StopCondition::after_micros(3_000_000));
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "no strict stop: every block it touches is mapped ({outcome:?})"
    );
    assert_eq!(m.bus.unmapped_reads() + m.bus.unmapped_writes(), 0);

    // The flash read that used to end this run now completes. `bootctl` and
    // the littlefs mount are the traffic; the `SPI1.cmd` spin is gone.
    let lines = buf.lines();
    let spins: Vec<&String> = lines.iter().filter(|l| l.contains(" SPIN ")).collect();
    assert!(
        !spins.iter().any(|l| l.contains("SPI1")),
        "M4 models the controller; a SPI1 spin is a regression: {spins:?}"
    );
    let census = m.flash_census();
    assert!(census.reads > 0, "the boot read flash: {census}");
    assert!(
        census.sector_erases > 0 && census.programs > 0,
        "and formatted an erased chip: {census}"
    );

    // The radio window comes after, and P6's stub answers it: the blob got
    // far enough to publish its RX DMA base.
    assert!(
        lines
            .iter()
            .any(|l| l.contains("WIFI RX config: dma_base=0x408")),
        "the radio window was not reached"
    );

    // And the image reaches the idle loop, which is the whole point.
    assert!(m.idle_skips() > 100, "{} idle skips", m.idle_skips());
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
