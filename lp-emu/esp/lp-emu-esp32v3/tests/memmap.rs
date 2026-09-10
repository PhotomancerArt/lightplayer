//! The classic's memory map does not drift.
//!
//! Every assertion here is against a number `memmap.rs` cites to
//! `third_party/esp-hal/ld/esp32/memory.x`, `lp-xt-emu`'s hardware-measured
//! board profile, the vendored ROM ELF or the `esp32` PAC. A test that failed
//! would mean a constant moved away from its citation — which is the one way
//! a map like this goes wrong quietly.

use lp_emu_esp32v3::memmap::{self, Span, periph};

#[test]
fn no_two_declared_regions_overlap() {
    for (i, a) in memmap::RAM_SPANS.iter().enumerate() {
        for b in &memmap::RAM_SPANS[i + 1..] {
            assert!(
                a.end() <= b.base || b.end() <= a.base,
                "`{}` (0x{:08x}..0x{:08x}) overlaps `{}` (0x{:08x}..0x{:08x})",
                a.name,
                a.base,
                a.end(),
                b.name,
                b.base,
                b.end()
            );
        }
    }
    for ram in memmap::RAM_SPANS {
        for mmio in memmap::MMIO_WINDOWS {
            assert!(
                ram.end() <= mmio.base || mmio.end() <= ram.base,
                "RAM `{}` overlaps MMIO window `{}`",
                ram.name,
                mmio.name
            );
        }
        for gap in memmap::UNMAPPED_BY_DESIGN {
            assert!(
                ram.end() <= gap.base || gap.end() <= ram.base,
                "RAM `{}` overlaps `{}`, which this machine deliberately does not map",
                ram.name,
                gap.name
            );
        }
    }
}

#[test]
fn the_ram_spans_are_in_address_order() {
    // Not decode-critical, but a reader of `--map` output and of the module
    // doc's ASCII table is entitled to one order.
    for pair in memmap::RAM_SPANS.windows(2) {
        assert!(
            pair[0].base < pair[1].base,
            "`{}` is declared after `{}` but starts below it",
            pair[1].name,
            pair[0].name
        );
    }
}

#[test]
fn every_peripheral_base_is_inside_a_declared_mmio_window() {
    let dport = Span {
        name: "mmio-dport",
        base: memmap::MMIO_BASE,
        len: memmap::MMIO_LEN,
    };
    let ahb = Span {
        name: "mmio-ahb",
        base: memmap::MMIO_AHB_BASE,
        len: memmap::MMIO_AHB_LEN,
    };
    assert_eq!(
        memmap::MMIO_WINDOWS.len(),
        2,
        "the DPORT window and its AHB mirror (P3)"
    );
    // Two bases are on the AHB bus — the analog I2C master the ROM drives
    // and the PAC's `RNG`, whose 0x6003_5000 P1 had excluded as an SVD leak
    // and P3 found to be the AHB address of WDEV (`memmap::MMIO_AHB_BASE`).
    for (name, base) in [("I2C_ANA_MST", periph::I2C_ANA_MST), ("RNG", periph::RNG)] {
        assert!(
            ahb.contains(base),
            "{name} at 0x{base:08x} is on the AHB bus"
        );
        assert!(!dport.contains(base));
        let twin = memmap::ahb_to_dport(base).expect("inside the mirror");
        assert!(dport.contains(twin), "{name}'s DPORT twin 0x{twin:08x}");
    }
    assert_eq!(
        memmap::ahb_to_dport(periph::RNG),
        Some(periph::WIFI + 0x2000),
        "WDEV, the WiFi window's second page, seen from the DPORT side"
    );
    assert_eq!(memmap::ahb_to_dport(0x3FF4_0000), None);
    let window = dport;
    for (name, base) in [
        ("DPORT", periph::DPORT),
        ("AES", periph::AES),
        ("RSA", periph::RSA),
        ("SHA", periph::SHA),
        ("UART0", periph::UART0),
        ("SPI1", periph::SPI1),
        ("SPI0", periph::SPI0),
        ("GPIO", periph::GPIO),
        ("FLASH_ENCRYPTION", periph::FLASH_ENCRYPTION),
        ("FRC_TIMER", periph::FRC_TIMER),
        ("RTC_CNTL", periph::RTC_CNTL),
        ("RTC_IO", periph::RTC_IO),
        ("SENS", periph::SENS),
        ("RTC_I2C", periph::RTC_I2C),
        ("IO_MUX", periph::IO_MUX),
        ("UART1", periph::UART1),
        ("RMT", periph::RMT),
        ("RMT_RAM", periph::RMT_RAM),
        ("EFUSE", periph::EFUSE),
        ("TIMG0", periph::TIMG0),
        ("TIMG1", periph::TIMG1),
        ("APB_CTRL", periph::APB_CTRL),
        ("UART2", periph::UART2),
        ("WIFI", periph::WIFI),
        ("FLASH_MMU_PRO", memmap::FLASH_MMU_PRO),
        ("FLASH_MMU_APP", memmap::FLASH_MMU_APP),
    ] {
        assert!(
            window.contains(base),
            "{name} at 0x{base:08x} is outside the declared MMIO window \
             0x{:08x}..0x{:08x}",
            window.base,
            window.end()
        );
    }
}

#[test]
fn dram_seg_starts_eight_kib_above_the_roms_reserve() {
    // `memory.x:19`: `dram_seg ORIGIN = 0x3FFAE000 + 8K + RESERVE_DRAM`.
    // RESERVE_DRAM is `third_party/esp-hal/build.rs:262-266` — 0x10000 under
    // the `__bluetooth` feature and 0 otherwise, and `fw-esp32v3` enables no
    // Bluetooth, so it is zero here. If that ever changes, this assertion is
    // the first thing that says so.
    assert_eq!(memmap::RESERVE_DRAM, 0, "no __bluetooth in fw-esp32v3");
    assert_eq!(memmap::DRAM_SEG_BASE, 0x3FFA_E000 + 8 * 1024);
    assert_eq!(memmap::DRAM_SEG_BASE, 0x3FFB_0000, "L0's bootloader agrees");
    assert_eq!(memmap::DRAM_SEG_LEN, 192 * 1024);
}

#[test]
fn the_sram0_vector_table_is_one_kib_below_the_iram() {
    // `memory.x:14`: `vectors_seg len = 1k`, and `:15` starts `iram_seg` at
    // `0x40080400`. Two lines of the same linker script, and the gap between
    // them is the vector table.
    assert_eq!(memmap::SRAM0_VECTORS + 0x400, memmap::SRAM0_IRAM);
    assert_eq!(memmap::SRAM0_VECTORS, 0x4008_0000);
}

#[test]
fn sram0_is_one_region_that_covers_the_cache_segment_and_the_iram() {
    // Director ruling DD24: one exec region from `reserved_cache_seg`,
    // because the IDF bootloader executes from 0x40078000 inside it.
    let sram0 = memmap::RAM_SPANS
        .iter()
        .find(|s| s.name == "sram0")
        .expect("SRAM0 is one declared region");
    assert_eq!(sram0.base, 0x4007_0000, "reserved_cache_seg, memory.x:13");
    assert_eq!(sram0.end(), memmap::SRAM0_END);
    assert!(
        sram0.contains(0x4007_8000),
        "the IDF bootloader's own text segment must be inside SRAM0"
    );
    assert!(sram0.contains(memmap::SRAM0_VECTORS));
    assert!(sram0.contains(memmap::SRAM0_IRAM));
    assert!(
        sram0.contains(0x4008_8000) && sram0.contains(0x4009_7FFF),
        "the JIT code region 0x40088000 +64 KiB lives in SRAM0"
    );
    assert_eq!(memmap::SRAM0_LEN, 192 * 1024);
    assert!(memmap::SRAM0_WORD_ONLY, "board.rs:156-172, measured");
}

#[test]
fn the_sram1_ibus_alias_is_named_and_not_mapped() {
    // The whole point of the constant: a strict stop can name the window.
    assert_eq!(memmap::SRAM1_IBUS_ALIAS_BASE, 0x400A_0000);
    assert_eq!(
        memmap::SRAM1_IBUS_ALIAS_BASE + memmap::SRAM1_IBUS_ALIAS_LEN,
        memmap::RTC_FAST_IBUS,
        "the alias runs from the end of SRAM0 to RTC fast memory's I-bus view"
    );
    assert_eq!(memmap::SRAM1_IBUS_ALIAS_BASE, memmap::SRAM0_END);
    for address in [0x400A_0000, 0x400A_8000, 0x400B_FFFC] {
        assert!(
            !memmap::RAM_SPANS.iter().any(|s| s.contains(address)),
            "0x{address:08x} is inside the SRAM1 I-bus alias and must stay unmapped"
        );
        assert!(
            memmap::UNMAPPED_BY_DESIGN
                .iter()
                .any(|s| s.contains(address)),
            "0x{address:08x} must still be NAMED, so a stop says which window it was"
        );
    }
    assert_eq!(
        memmap::SRAM1_IBUS_ALIAS_LEN,
        memmap::SRAM1_DBUS_LEN,
        "the alias mirrors the whole of SRAM1"
    );
}

#[test]
fn the_regions_that_abut_in_memory_x_abut_here() {
    // A gap or an overlap on any of these would put a real segment somewhere
    // the ELF or the linker script does not say.
    assert_eq!(
        memmap::MMIO_BASE + memmap::MMIO_LEN,
        memmap::RTC_FAST_DBUS,
        "the peripheral window ends where RTC fast memory's data view begins"
    );
    assert_eq!(
        memmap::SRAM2_ROM_RESERVED + 8 * 1024,
        memmap::DRAM_SEG_BASE,
        "the ROM's 8 KiB sits directly below dram_seg"
    );
    assert_eq!(
        memmap::DRAM_SEG_BASE + memmap::DRAM_SEG_LEN,
        memmap::SRAM1_DBUS_BASE,
        "dram_seg ends where SRAM1's data view begins"
    );
    assert_eq!(
        memmap::SRAM1_DBUS_BASE + memmap::SRAM1_DBUS_LEN,
        memmap::ROM_MASK_BASE,
        "SRAM1 ends at 0x40000000, where the mask ROM window begins"
    );
    assert_eq!(
        memmap::RTC_FAST_IBUS + memmap::RTC_FAST_LEN,
        0x400C_2000,
        "RTC fast memory is 8 KiB (memory.x:51)"
    );
}

#[test]
fn the_rom_windows_are_the_extents_the_vendored_elf_reports() {
    // `m3/notes.md` §2, read from `esp32_rev300_rom.elf`'s own program
    // headers. P2 parses the ELF and can assert these from the file itself;
    // until then they are here so a hand edit has something to fail against.
    assert_eq!(
        memmap::ROM_MASK_BASE,
        0x4000_0000,
        "the vector table's base"
    );
    assert_eq!(
        memmap::ROM_MASK_BASE + memmap::ROM_MASK_LEN,
        0x4006_5D90,
        "the highest code byte: .secureboot_* at 0x40065000 + 0xD90"
    );
    assert_eq!(memmap::ROM_DATA_BASE, 0x3FF9_6000, ".rodata's vaddr");
    assert_eq!(
        memmap::ROM_DATA_BASE + memmap::ROM_DATA_LEN,
        0x3FF9_F42A,
        "through the second read-only chunk at 0x3FF9F100 + 0x32A"
    );
    // The reset vector is inside the mask ROM window, which is the one thing
    // a ROM-up boot cannot do without.
    assert!(
        memmap::RAM_SPANS
            .iter()
            .any(|s| s.name == "rom-mask" && s.contains(0x4000_0400))
    );
}

#[test]
fn the_two_rtc_fast_views_are_the_same_size_and_do_not_overlap() {
    // `memory.x:51` and `:54`: "RTC fast memory (same block as above), viewed
    // from data bus". Two addresses, one memory — P2 backs them with one
    // store or the machine has two answers for one byte.
    let ibus = memmap::RAM_SPANS.iter().find(|s| s.name == "rtc-fast-ibus");
    let dbus = memmap::RAM_SPANS.iter().find(|s| s.name == "rtc-fast-dbus");
    let (ibus, dbus) = (ibus.expect("declared"), dbus.expect("declared"));
    assert_eq!(ibus.len, dbus.len);
    assert_eq!(ibus.len, 8 * 1024);
    assert!(ibus.end() <= dbus.base || dbus.end() <= ibus.base);
}

#[test]
fn one_emulated_microsecond_is_240_cycles() {
    // `CpuClock::max()` is 240 MHz on this chip, and `init_board` sets it:
    // `lp-fw/fw-esp32v3/src/board/esp32v3/init.rs:49-53`.
    assert_eq!(memmap::CPU_HZ, 240_000_000);
    assert_eq!(memmap::CYCLES_PER_US, 240);
}
