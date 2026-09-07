//! Register-name tables: offset → the name silicon's datasheet uses.
//!
//! A bus log that says `UART0+0x01c` is a log you have to decode by hand
//! against a PAC; one that says `UART0+0x01c status` is a log you can read.
//! The tables are **generated**, never hand-written, by
//! `scripts/emu/pac-regnames.py` from the svd2rust offset comments in the
//! `esp32c6` PAC — the same source the register inventory came from — and
//! each generated file carries the provenance header the license ADR
//! requires (`docs/adr/2026-07-29-license-provenance-discipline.md`).
//! `just lint-emu-regnames` regenerates and diffs, so a hand edit is caught
//! the way `lint-vec-corpus` catches one in the shader corpus.
//!
//! No table lives in this crate: the generated files land in the chip crate
//! that uses them, because a register layout is chip-family data and this
//! crate holds no chip numbers. The type and the lookup live here.

/// A block's register names, sorted by offset.
///
/// Names are the block-local ones (`fifo`, `rtccalicfg`, `unit0load.hi`),
/// not `uart0.fifo`: the block token in a trace line comes from the
/// peripheral *instance* (`UART0` vs `UART1`), while a PAC register block is
/// a *type* that several instances share. Qualifying a name with the type
/// would print `UART1+0x000 uart0.fifo`, which is worse than either half
/// alone. [`RegNames::qualified_block`] carries the type name for anyone who
/// wants it.
#[derive(Copy, Clone, Debug)]
pub struct RegNames {
    /// The PAC register-block name the table was generated from (`uart0`).
    /// Provenance in the type system, not just in a comment.
    pub block: &'static str,
    /// `(offset, name)`, sorted by offset, no duplicates.
    pub entries: &'static [(u32, &'static str)],
}

/// A table for a block with no names yet. Peripherals default to it so that
/// "no table" and "empty table" behave the same.
pub const EMPTY: RegNames = RegNames {
    block: "",
    entries: &[],
};

impl RegNames {
    /// The name of the register containing `off`.
    ///
    /// `off` may carry byte-lane bits; the lookup rounds down to the word,
    /// so a byte write to `UART0+0x001` still names `fifo`.
    pub fn name(&self, off: u32) -> Option<&'static str> {
        let word = off & !3;
        self.entries
            .binary_search_by_key(&word, |(o, _)| *o)
            .ok()
            .map(|i| self.entries[i].1)
    }

    /// The PAC register-block type this table describes (`uart0`).
    pub fn qualified_block(&self) -> &'static str {
        self.block
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Panics if the table is not sorted by offset or has duplicates. The
    /// generator emits sorted tables; this is what proves a hand-edited one
    /// is not silently mis-binary-searched.
    pub fn assert_sorted(&self) {
        for w in self.entries.windows(2) {
            assert!(
                w[0].0 < w[1].0,
                "RegNames table for `{}` is not strictly sorted by offset: \
                 0x{:03x} ({}) then 0x{:03x} ({})",
                self.block,
                w[0].0,
                w[0].1,
                w[1].0,
                w[1].1
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static SAMPLE: RegNames = RegNames {
        block: "uart0",
        entries: &[(0x000, "fifo"), (0x004, "int_raw"), (0x01c, "status")],
    };

    #[test]
    fn looks_up_by_exact_offset() {
        assert_eq!(SAMPLE.name(0x000), Some("fifo"));
        assert_eq!(SAMPLE.name(0x004), Some("int_raw"));
        assert_eq!(SAMPLE.name(0x01c), Some("status"));
    }

    #[test]
    fn a_byte_lane_offset_still_names_its_register() {
        assert_eq!(SAMPLE.name(0x001), Some("fifo"));
        assert_eq!(SAMPLE.name(0x003), Some("fifo"));
        assert_eq!(SAMPLE.name(0x01e), Some("status"));
    }

    #[test]
    fn an_offset_with_no_register_has_no_name() {
        assert_eq!(SAMPLE.name(0x008), None);
        assert_eq!(SAMPLE.name(0xfff), None);
    }

    #[test]
    fn empty_is_empty() {
        assert!(EMPTY.is_empty());
        assert_eq!(EMPTY.len(), 0);
        assert_eq!(EMPTY.name(0), None);
        EMPTY.assert_sorted();
    }

    #[test]
    fn assert_sorted_accepts_a_sorted_table() {
        SAMPLE.assert_sorted();
    }

    #[test]
    #[should_panic(expected = "not strictly sorted")]
    fn assert_sorted_rejects_an_unsorted_one() {
        RegNames {
            block: "bad",
            entries: &[(0x004, "b"), (0x000, "a")],
        }
        .assert_sorted();
    }
}
