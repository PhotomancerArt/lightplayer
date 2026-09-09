//! Register-name tables: offset → the name silicon's datasheet uses, and
//! the reset value it reads before anyone writes it.
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

/// How the part lets the guest touch a register, from the SVD's `access`
/// attribute as svd2rust writes it: a register has `impl Readable`, `impl
/// Writable`, both, or (for a few reserved words) neither.
///
/// It is what lets an accept block grade itself. Accept-and-remember **is**
/// the documented behaviour of a [`ReadWrite`](Access::ReadWrite) register —
/// the document says it holds what you write, and that is what a `RegFile`
/// does. On a [`ReadOnly`](Access::ReadOnly) one the value comes from
/// hardware nobody here models, so whatever we answer is a stand-in; on a
/// [`WriteOnly`](Access::WriteOnly) one the document does not say what a
/// read returns at all.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Access {
    /// The default, and the one this table does not list.
    ReadWrite,
    ReadOnly,
    WriteOnly,
    /// Neither — a reserved word svd2rust names but does not expose.
    NoAccess,
}

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
    /// `(offset, reset value)` for every register whose PAC reset is not
    /// zero, sorted by offset. Zero resets are left out: they are the
    /// window's own default, and listing 400 of them would bury the 40 that
    /// say something.
    ///
    /// [`RegFile::with_names`](crate::RegFile::with_names) seeds from this,
    /// which is what turns "the resets a boot was observed to need" into
    /// "the resets the PAC states" — see
    /// `docs/defects/2026-09-07-accept-blocks-carry-only-the-reset-values-a-boot-needed.md`.
    pub resets: &'static [(u32, u32)],
    /// `(offset, access)` for every register that is **not** plain
    /// read-write, sorted by offset. Read-write is the common case and the
    /// default, so listing it would bury the exceptions.
    pub access: &'static [(u32, Access)],
}

/// A table for a block with no names yet. Peripherals default to it so that
/// "no table" and "empty table" behave the same.
pub const EMPTY: RegNames = RegNames {
    block: "",
    entries: &[],
    resets: &[],
    access: &[],
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

    /// The PAC's reset value for the register containing `off`, or `None`
    /// where the PAC says zero (or names no register there).
    pub fn reset(&self, off: u32) -> Option<u32> {
        let word = off & !3;
        self.resets
            .binary_search_by_key(&word, |(o, _)| *o)
            .ok()
            .map(|i| self.resets[i].1)
    }

    /// How the part lets the guest touch the register containing `off`, or
    /// `None` where the block names no register there.
    pub fn access(&self, off: u32) -> Option<Access> {
        let word = off & !3;
        self.name(word)?;
        Some(
            self.access
                .binary_search_by_key(&word, |(o, _)| *o)
                .map(|i| self.access[i].1)
                .unwrap_or(Access::ReadWrite),
        )
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

    /// Panics if either table is not sorted by offset or has duplicates.
    /// The generator emits sorted tables; this is what proves a hand-edited
    /// one is not silently mis-binary-searched.
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
        for w in self.resets.windows(2) {
            assert!(
                w[0].0 < w[1].0,
                "RegNames reset table for `{}` is not strictly sorted by offset: \
                 0x{:03x} then 0x{:03x}",
                self.block,
                w[0].0,
                w[1].0
            );
        }
        for w in self.access.windows(2) {
            assert!(
                w[0].0 < w[1].0,
                "RegNames access table for `{}` is not strictly sorted by offset: \
                 0x{:03x} then 0x{:03x}",
                self.block,
                w[0].0,
                w[1].0
            );
        }
        // An entry at an offset the block does not name is a parse that went
        // wrong, not a fact about silicon.
        for (off, _) in self.resets {
            assert!(
                self.name(*off).is_some(),
                "RegNames reset table for `{}` has 0x{off:03x}, which names no register",
                self.block
            );
        }
        for (off, _) in self.access {
            assert!(
                self.name(*off).is_some(),
                "RegNames access table for `{}` has 0x{off:03x}, which names no register",
                self.block
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
        resets: &[(0x01c, 0x0000_0060)],
        access: &[(0x01c, Access::ReadOnly)],
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
        assert_eq!(EMPTY.reset(0), None);
        EMPTY.assert_sorted();
    }

    #[test]
    fn a_reset_is_found_by_the_register_that_carries_it() {
        assert_eq!(SAMPLE.reset(0x01c), Some(0x60));
        // A byte lane still names its register's reset.
        assert_eq!(SAMPLE.reset(0x01e), Some(0x60));
        // A register the PAC resets to zero is simply absent.
        assert_eq!(SAMPLE.reset(0x000), None);
        assert_eq!(SAMPLE.reset(0x008), None);
    }

    #[test]
    fn access_defaults_to_read_write_and_only_the_exceptions_are_listed() {
        assert_eq!(SAMPLE.access(0x01c), Some(Access::ReadOnly));
        assert_eq!(SAMPLE.access(0x01e), Some(Access::ReadOnly), "byte lane");
        assert_eq!(SAMPLE.access(0x000), Some(Access::ReadWrite));
        assert_eq!(SAMPLE.access(0x004), Some(Access::ReadWrite));
        // An offset that names no register has no access either.
        assert_eq!(SAMPLE.access(0x008), None);
        assert_eq!(EMPTY.access(0), None);
    }

    #[test]
    fn assert_sorted_accepts_a_sorted_table() {
        SAMPLE.assert_sorted();
    }

    #[test]
    #[should_panic(expected = "names no register")]
    fn assert_sorted_rejects_a_reset_for_a_register_that_is_not_there() {
        RegNames {
            block: "bad",
            entries: &[(0x000, "a")],
            resets: &[(0x008, 1)],
            access: &[],
        }
        .assert_sorted();
    }

    #[test]
    #[should_panic(expected = "not strictly sorted")]
    fn assert_sorted_rejects_an_unsorted_one() {
        RegNames {
            block: "bad",
            entries: &[(0x004, "b"), (0x000, "a")],
            resets: &[],
            access: &[],
        }
        .assert_sorted();
    }
}
