//! Generated register-name tables for the C6's blocks.
//!
//! A bus log that says `LP_CLKRST+0x010` has to be decoded by hand against a
//! PAC; one that says `LP_CLKRST+0x010 reset_cause` can be read. The tables
//! are **generated** from the `esp32c6` PAC's svd2rust offset comments by
//! `scripts/emu/pac-regnames.py`, carry the provenance header
//! `docs/adr/2026-07-29-license-provenance-discipline.md` requires, and are
//! checked by `just lint-emu-regnames` — a hand edit is reverted by the next
//! regeneration and takes its provenance with it.
//!
//! They live in this crate, not in `lp-emu-esp-common`, because a register
//! layout is chip-family data and that crate holds no chip numbers.
//!
//! # What is here, and why only this much
//!
//! Two blocks: the two the P4 boot trace actually names. P4 models no
//! peripherals at all, so a table for a block nothing reads would be
//! generated data nobody has checked. P5/P6/M4 add a row to the generator's
//! `TARGETS` as they model each block.
//!
//! - [`LP_CLKRST`] — `reset_cause` at `+0x10` is the very first MMIO access
//!   of every boot, from inside the mask ROM's `rtc_get_reset_reason`. This
//!   table is what caught that `0x600B_0410` is LP_CLKRST and **not** LP_AON
//!   (which is at `0x600B_1000`); reading the address as "LP_AON plus
//!   something" would have sent P5 to model the wrong block.
//! - [`INTERRUPT_CORE0`] — `_setup_interrupts` writes all 77
//!   `core_0_intr_map` entries before anything else happens.

mod interrupt_core0;
mod lp_clkrst;

pub use interrupt_core0::INTERRUPT_CORE0;
pub use lp_clkrst::LP_CLKRST;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_generated_tables_are_sorted_and_non_empty() {
        for table in [&LP_CLKRST, &INTERRUPT_CORE0] {
            table.assert_sorted();
            assert!(!table.is_empty(), "`{}` is empty", table.block);
        }
    }

    #[test]
    fn interrupt_core0_expands_all_77_map_entries() {
        // svd2rust leaves some register arrays unexpanded, and a missing
        // element is silent — the trace just stops naming a register. The
        // count comes from `esp32c6-0.23.2/src/interrupt.rs` (sources 0..=76,
        // mirrored by `core_0_intr_map[77]`), M3 discovery §6c.
        let maps = INTERRUPT_CORE0
            .entries
            .iter()
            .filter(|(_, name)| name.starts_with("core_0_intr_map"))
            .count();
        assert_eq!(maps, 77);
        assert_eq!(INTERRUPT_CORE0.name(0x000), Some("core_0_intr_map0"));
        assert_eq!(INTERRUPT_CORE0.name(0x130), Some("core_0_intr_map76"));
        // UART0 = 43, SYSTIMER_TARGET0 = 57, GPIO = 30: the three the
        // discovery spells out, at base + 4n.
        assert_eq!(INTERRUPT_CORE0.name(4 * 43), Some("core_0_intr_map43"));
        assert_eq!(INTERRUPT_CORE0.name(4 * 57), Some("core_0_intr_map57"));
        assert_eq!(INTERRUPT_CORE0.name(4 * 30), Some("core_0_intr_map30"));
    }

    #[test]
    fn lp_clkrst_names_the_register_the_mask_rom_reads() {
        assert_eq!(LP_CLKRST.name(0x010), Some("reset_cause"));
        // A byte-lane offset still names its register, which is what makes
        // the trace readable for a sub-word access.
        assert_eq!(LP_CLKRST.name(0x012), Some("reset_cause"));
        assert_eq!(LP_CLKRST.qualified_block(), "lp_clkrst");
    }
}
