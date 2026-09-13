//! The map does not drift, and it holds together.
//!
//! Every constant in [`lp_emu_esp32s3::memmap`] carries the `file:line` it
//! was read from; this file asserts the relationships **between** them — the
//! things a single citation cannot say, and the things a later edit would
//! quietly break.

use lp_emu_esp32s3::bus_setup;
use lp_emu_esp32s3::memmap::{self, Span};

/// No two regions overlap, and no region overlaps a RAM alias, an MMIO window
/// or a window this machine names and does not map.
///
/// `SocBus` asserts most of this at registration; asserting it here too means
/// a map edit fails in a test with a readable message rather than in a panic
/// inside the bus, and it covers the two the bus does not check:
/// region-vs-MMIO-window and region-vs-deliberately-unmapped.
#[test]
fn nothing_in_the_map_overlaps_anything_else() {
    let mut spans: Vec<Span> = memmap::RAM_SPANS.to_vec();
    spans.extend(memmap::RAM_ALIASES.iter().map(|(s, _)| *s));
    spans.extend(memmap::MMIO_WINDOWS.iter().copied());
    spans.extend(memmap::UNMAPPED_BY_DESIGN.iter().copied());
    spans.sort_by_key(|s| s.base);

    for pair in spans.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        assert!(
            a.end() <= b.base,
            "`{}` ({:#010x}..{:#010x}) overlaps `{}` ({:#010x}..{:#010x})",
            a.name,
            a.base,
            a.end(),
            b.name,
            b.base,
            b.end()
        );
    }

    for span in &spans {
        assert_ne!(span.len, 0, "`{}` has no length", span.name);
        assert!(
            u64::from(span.base) + u64::from(span.len) <= 1 << 32,
            "`{}` wraps the address space",
            span.name
        );
    }
}

/// **The alias arithmetic, three ways.**
///
/// `0x4037_8000 − 0x3FC8_8000 = 0x6F_0000` is the number the product's JIT
/// adds to every entry address it publishes
/// (`lp-shader/lpvm-native/src/exec_addr.rs:37`), the distance
/// `memory.x:13-14` draws, and the difference between the two bases this map
/// declares. If any of the three moved without the others, a shader would
/// execute from the wrong place and nothing would say so.
#[test]
fn the_sram1_ibus_view_is_exactly_0x6f0000_above_the_dbus_one() {
    assert_eq!(
        memmap::SRAM1_IBUS_BASE - memmap::SRAM1_DBUS_BASE,
        0x6F_0000,
        "the two views' distance"
    );
    assert_eq!(memmap::SRAM1_IBUS_OFFSET, 0x6F_0000);

    // And the alias covers the whole region, not part of it: a shader
    // compiled into the top of the heap must be fetchable too.
    let (alias, target) = memmap::RAM_ALIASES[0];
    assert_eq!(alias.base, memmap::SRAM1_IBUS_BASE);
    assert_eq!(alias.len, memmap::SRAM1_LEN);
    assert_eq!(target, memmap::SRAM1_DBUS_BASE);
}

/// The alias resolves on a real bus, and it is **one store**: a byte written
/// through the D-bus view is the byte the I-bus view reads back.
///
/// This is the whole reason M6 P02 exists, so it is asserted against the bus
/// the machine actually builds rather than against the constants.
#[test]
fn the_alias_is_one_store_with_two_doors() {
    use lp_emu_core::Bus;

    let mut bus = bus_setup::build();
    assert_eq!(
        bus.ram_aliases(),
        vec![(
            memmap::SRAM1_IBUS_BASE,
            memmap::SRAM1_LEN,
            memmap::SRAM1_DBUS_BASE
        )],
        "one alias, and it is SRAM1's"
    );

    // The JIT heap's own base, from `m6/notes.md` §2.6 — a real address the
    // product writes through, rounded down to a word.
    let write_at = 0x3FC9_12B0u32;
    let fetch_at = write_at + memmap::SRAM1_IBUS_OFFSET;
    bus.write_word(write_at, 0x1234_5678u32 as i32)
        .expect("writing through the D-bus view");
    assert_eq!(
        bus.read_word(fetch_at)
            .expect("reading through the I-bus view") as u32,
        0x1234_5678,
        "the I-bus view reads back what the D-bus view wrote"
    );

    // And the region behind both doors is executable, which is what makes a
    // fetch through the alias legal (see `bus_setup`'s module docs).
    let sram1 = bus
        .regions()
        .iter()
        .find(|r| r.name == "sram1-dbus")
        .expect("sram1-dbus is registered");
    assert!(
        sram1.is_executable(),
        "the D-bus region carries the executable flag, because an aliased \
         fetch arrives at the canonical address"
    );
}

/// Every declared sub-span is inside the region that owns it.
///
/// The linker script's `dram_seg`/`dram2_seg`, the vectors and `iram_seg`,
/// and the mask ROM's two stacks are all *descriptions* of memory this map
/// registers as larger regions. A constant that fell outside its owner would
/// be a map bug that nothing else would catch.
#[test]
fn every_declared_base_is_inside_its_window() {
    let sram1 = Span {
        name: "sram1-dbus",
        base: memmap::SRAM1_DBUS_BASE,
        len: memmap::SRAM1_LEN,
    };
    // The linker's two halves of one memory.
    assert!(sram1.contains(memmap::DRAM_SEG_BASE));
    assert!(sram1.contains(memmap::DRAM2_SEG_BASE));
    assert_eq!(
        memmap::DRAM_SEG_BASE + memmap::DRAM_SEG_LEN,
        memmap::DRAM2_SEG_BASE,
        "dram_seg runs up to dram2_seg's origin (memory.x:30)"
    );
    assert!(
        memmap::DRAM2_SEG_BASE + memmap::DRAM2_SEG_LEN <= sram1.end(),
        "and dram2_seg ends inside the block"
    );
    // The ROM's own stacks live in the same block, which is why the ROM ELF's
    // first PT_LOAD lands there.
    assert!(sram1.contains(memmap::ROM_PRO_STACK_BASE));
    assert!(sram1.contains(memmap::ROM_PRO_STACK_TOP));
    assert!(sram1.contains(memmap::ROM_APP_STACK_TOP - 1));
    assert!(
        memmap::ROM_PRO_STACK_BASE < memmap::ROM_PRO_STACK_TOP,
        "the PRO stack grows down from its top"
    );

    // The I-bus side: the vectors and `iram_seg` are the first bytes of the
    // alias, and the icache reserve is immediately below it.
    let alias = memmap::RAM_ALIASES[0].0;
    assert!(alias.contains(memmap::VECTORS_BASE));
    assert!(alias.contains(memmap::IRAM_SEG_BASE));
    assert_eq!(
        memmap::ICACHE_RESERVE_BASE + memmap::RESERVE_ICACHE,
        memmap::VECTORS_BASE,
        "vectors_seg starts at 0x4037_0000 + RESERVE_ICACHE (memory.x:25)"
    );
    assert!(
        memmap::IRAM_SEG_BASE + memmap::IRAM_SEG_LEN <= alias.end(),
        "the linker's iram_seg stops inside the block it is carved from"
    );

    // And the mask ROM's data is where the ROM ELF's `.rodata` says.
    let rom_data = Span {
        name: "rom-data",
        base: memmap::ROM_DATA_BASE,
        len: memmap::ROM_DATA_LEN,
    };
    assert!(rom_data.contains(0x3FF1_8C00));
}

/// **`.rwdata_dummy` is why the apparent `memory.x` overlap is not one.**
///
/// `SIZEOF(.vectors) + SIZEOF(.rwtext)` = `0x400 + 0x2F20` = `0x3320`, and
/// `.data` starts immediately above it. A reader who sees `dram_seg` and
/// `iram_seg` describing the same physical memory and does not know about the
/// reservation will think this map double-books it.
#[test]
fn the_rwdata_dummy_reservation_accounts_for_the_apparent_overlap() {
    assert_eq!(
        memmap::RWDATA_DUMMY_LEN,
        memmap::VECTORS_LEN + 0x2F20,
        "SIZEOF(.vectors) + SIZEOF(.rwtext) (esp32s3.x:33-37, m6/notes.md §2.6)"
    );
    assert_eq!(
        memmap::DATA_START,
        0x3FC8_B320,
        ".data starts immediately above the reservation"
    );
}

/// RTC fast is memory, not a peripheral, and the declared MMIO window stops
/// where it starts.
#[test]
fn the_mmio_window_stops_where_rtc_fast_begins() {
    assert_eq!(
        memmap::MMIO_BASE + memmap::MMIO_LEN,
        memmap::RTC_FAST_BASE,
        "so a strict stop inside RTC fast cannot report `an unmodelled block`"
    );
    assert_eq!(
        memmap::RTC_FAST_BASE + memmap::RTC_FAST_LEN,
        0x6010_0000,
        "RTC fast is the top 8 KiB of the nominal 1 MiB aperture"
    );
    // Every peripheral base the census named is inside the declared window.
    for (name, base) in [
        ("UART0", memmap::periph::UART0),
        ("SPI1", memmap::periph::SPI1),
        ("SPI0", memmap::periph::SPI0),
        ("GPIO", memmap::periph::GPIO),
        ("FE2", memmap::periph::FE2),
        ("FE", memmap::periph::FE),
        ("EFUSE", memmap::periph::EFUSE),
        ("RTC_CNTL", memmap::periph::RTC_CNTL),
        ("IO_MUX", memmap::periph::IO_MUX),
        ("I2C_ANA_MST", memmap::periph::I2C_ANA_MST),
        ("RMT", memmap::periph::RMT),
        ("RMT_RAM", memmap::periph::RMT_RAM),
        ("NRX", memmap::periph::NRX),
        ("BB", memmap::periph::BB),
        ("TIMG0", memmap::periph::TIMG0),
        ("TIMG1", memmap::periph::TIMG1),
        ("SYSTIMER", memmap::periph::SYSTIMER),
        ("APB_CTRL", memmap::periph::APB_CTRL),
        ("USB_DEVICE", memmap::periph::USB_DEVICE),
        ("SHA", memmap::periph::SHA),
        ("SYSTEM", memmap::periph::SYSTEM),
        ("INTERRUPT_CORE0", memmap::periph::INTERRUPT_CORE0),
        ("INTERRUPT_CORE1", memmap::periph::INTERRUPT_CORE1),
        ("EXTMEM", memmap::periph::EXTMEM),
    ] {
        assert!(
            memmap::MMIO_WINDOWS[0].contains(base),
            "{name} at {base:#010x} is outside the declared MMIO window"
        );
    }
    // The two interrupt blocks are one 4 KiB window, core 1 at +0x800.
    assert_eq!(
        memmap::periph::INTERRUPT_CORE1 - memmap::periph::INTERRUPT_CORE0,
        0x800
    );
    // And the register this phase wires the core-1 hold to is SYSTEM's first.
    assert_eq!(memmap::SYSTEM_CORE_1_CONTROL_0, memmap::periph::SYSTEM);
}

/// 240 cycles to the microsecond.
#[test]
fn the_clock_is_240_mhz() {
    assert_eq!(memmap::CPU_HZ, 240_000_000);
    assert_eq!(memmap::CYCLES_PER_US, 240);
}

/// The two cores are told apart by bit 13, which is the only field anything
/// in this repository reads (`esp-hal`'s `raw_core()`).
#[test]
fn the_two_prid_words_differ_in_the_bit_esp_hal_reads() {
    assert_eq!(memmap::PRID_CORE0 & 0x2000, 0, "core 0: bit 13 clear");
    assert_eq!(memmap::PRID_CORE1 & 0x2000, 0x2000, "core 1: bit 13 set");
}

/// The icache reserve is named and **not** registered — the map's one
/// deliberate hole, so a strict stop inside it says which window it was.
#[test]
fn the_icache_reserve_is_named_and_unmapped() {
    let bus = bus_setup::build();
    for (base, len, _) in bus.region_spans() {
        let span = Span {
            name: "registered",
            base,
            len,
        };
        assert!(
            !span.contains(memmap::ICACHE_RESERVE_BASE),
            "the icache reserve must not be behind a region"
        );
    }
    let (span, why) = bus_setup::unmapped_window(memmap::ICACHE_RESERVE_BASE + 4)
        .expect("a strict stop inside the reserve names the window");
    assert_eq!(span.name, "icache-reserve");
    assert!(why.contains("RESERVE_ICACHE"), "{why}");
    // And an address that is not in a named hole gets no name.
    assert!(bus_setup::unmapped_window(0x1234_5678).is_none());
}

/// The alias reporter names the door and the canonical address behind it.
///
/// DD81 says a fault through the alias reports the **canonical** address, so
/// nothing downstream can name the alias; this is what lets `--map` and a
/// stop message say which door an address was anyway.
#[test]
fn the_alias_reporter_names_the_door() {
    let (span, canonical) = bus_setup::ram_alias_of(memmap::SRAM1_IBUS_BASE + 0x1234)
        .expect("an address inside the alias");
    assert_eq!(span.name, "sram1-ibus");
    assert_eq!(canonical, memmap::SRAM1_DBUS_BASE + 0x1234);
    assert!(bus_setup::ram_alias_of(memmap::SRAM1_DBUS_BASE).is_none());
}
