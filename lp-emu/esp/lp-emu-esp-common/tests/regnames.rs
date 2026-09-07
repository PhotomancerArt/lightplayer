//! The generated register-name tables, used the way a peripheral uses them.
//!
//! The tables under `tests/regs/` are produced by
//! `scripts/emu/pac-regnames.py` from the `esp32c6` PAC and checked by
//! `just lint-emu-regnames`. They live in `tests/` rather than `src/`
//! because this crate holds no chip data: from P4 on, the generator writes
//! the real tables into the chip crate, and these two stay as the proof that
//! the generator, the provenance header and the lookup all work together.
//!
//! What the tests below assert is the join: a `RegFile` with a generated
//! table produces the named lines in the bus trace, at the offsets the
//! discovery report measured.

use lp_emu_core::bus::Bus;
use lp_emu_esp_common::bus::{RamRegion, SocBus};
use lp_emu_esp_common::regfile::RegFile;
use lp_emu_esp_common::trace::{SharedBuffer, Trace};

#[path = "regs/systimer.rs"]
mod systimer;
#[path = "regs/uart0.rs"]
mod uart0;

#[test]
fn the_generated_tables_are_sorted_and_non_empty() {
    uart0::UART0.assert_sorted();
    systimer::SYSTIMER.assert_sorted();
    assert!(uart0::UART0.len() >= 38);
    assert!(systimer::SYSTIMER.len() >= 36);
    assert_eq!(uart0::UART0.qualified_block(), "uart0");
    assert_eq!(systimer::SYSTIMER.qualified_block(), "systimer");
}

/// The offsets the M3 discovery report measured against esp-hal's driver
/// paths. If a PAC upgrade moves one of these, the emulator's stubs are
/// wrong and this is where it shows.
#[test]
fn the_offsets_match_the_registers_the_discovery_report_named() {
    // UART0: the FIFO the ROM writes a byte at a time, the status register
    // whose txfifo_cnt the driver spins on, and reg_update, which must read
    // back 0 after a 1-write.
    assert_eq!(uart0::UART0.name(0x000), Some("fifo"));
    assert_eq!(uart0::UART0.name(0x01c), Some("status"));
    assert_eq!(uart0::UART0.name(0x070), Some("fsm_status"));
    assert_eq!(uart0::UART0.name(0x098), Some("reg_update"));

    // SYSTIMER: `Instant::now()` writes unit0_op and reads unit0_value.
    assert_eq!(systimer::SYSTIMER.name(0x004), Some("unit0_op"));
    assert_eq!(systimer::SYSTIMER.name(0x040), Some("unit0_value.hi"));
    assert_eq!(systimer::SYSTIMER.name(0x044), Some("unit0_value.lo"));
    assert_eq!(systimer::SYSTIMER.name(0x034), Some("target0_conf"));
}

/// The array-of-cluster shape svd2rust does NOT expand into per-index
/// accessors. Without the generator's second pass these would be missing,
/// and a trace would show a bare offset for the timer's compare values.
#[test]
fn array_clusters_without_expanded_siblings_are_still_named() {
    assert_eq!(systimer::SYSTIMER.name(0x01c), Some("trgt0.hi"));
    assert_eq!(systimer::SYSTIMER.name(0x030), Some("trgt2.lo"));
    assert_eq!(systimer::SYSTIMER.name(0x074), Some("real_target0.lo"));
    assert_eq!(systimer::SYSTIMER.name(0x088), Some("real_target2.hi"));
}

#[test]
fn a_regfile_with_a_generated_table_names_registers_in_the_bus_trace() {
    let buf = SharedBuffer::new();
    let mut bus = SocBus::new();
    bus.trace = Trace::to_sink(Box::new(buf.clone()));
    bus.add_region(RamRegion::new("hp-ram", 0x4080_0000, 0x100));
    bus.add_peripheral(
        0x6000_0000,
        0x100,
        Box::new(
            RegFile::new("UART0", 0x100)
                .with_names(uart0::UART0)
                // The register the driver writes 1 to and spins until it
                // reads 0 (esp-hal `uart/mod.rs:955`).
                .with_write_one_pulse(0x98, 1),
        ),
    );

    bus.set_time(1_000);
    bus.set_pc(0x4200_1234);
    // The ROM's `uart_tx_one_char` writes the FIFO as a byte.
    bus.write_byte(0x6000_0000, b'H' as i8).unwrap();
    bus.write_word(0x6000_0098, 1).unwrap();
    bus.read_word(0x6000_0098).unwrap();
    bus.read_word(0x6000_001c).unwrap();

    assert_eq!(
        buf.lines(),
        [
            "cyc=1000 pc=0x42001234 W1 UART0+0x000 fifo = 0x00000048",
            "cyc=1000 pc=0x42001234 W4 UART0+0x098 reg_update = 0x00000001",
            "cyc=1000 pc=0x42001234 R4 UART0+0x098 reg_update = 0x00000000",
            "cyc=1000 pc=0x42001234 R4 UART0+0x01c status = 0x00000000",
        ]
    );
}

/// The bring-up tool, end to end: a firmware polling a status bit that never
/// changes gets one line naming the register it is stuck on.
#[test]
fn a_spin_on_a_named_register_reports_the_name() {
    let buf = SharedBuffer::new();
    let mut bus = SocBus::new();
    bus.trace = Trace::to_sink(Box::new(buf.clone()))
        .with_block_filter(["nothing"])
        .with_spin_threshold(1_000);
    bus.add_peripheral(
        0x6000_a000,
        0x100,
        Box::new(RegFile::new("SYSTIMER", 0x100).with_names(systimer::SYSTIMER)),
    );

    bus.set_pc(0x4200_9a1c);
    for cyc in 0..2_000 {
        bus.set_time(cyc);
        // Polling unit0_op's `value_valid`, which nothing ever sets.
        bus.read_word(0x6000_a004).unwrap();
    }

    assert_eq!(
        buf.lines(),
        ["cyc=999 pc=0x42009a1c SPIN SYSTIMER+0x004 unit0_op = 0x00000000 x1000"]
    );
}
