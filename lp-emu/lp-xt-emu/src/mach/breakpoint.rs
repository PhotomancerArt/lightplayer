//! `DBREAK` (two data-breakpoint slots, mirrored onto the bus as
//! [`Watchpoint`]s) and `IBREAK` (two instruction breakpoints, checked at
//! fetch on the hart). The RV32 twin is `mach/trigger.rs`.
//!
//! Semantics: ISA RM §4.7.6.3 "Using Breakpoints" (the DBREAKC format, Table
//! 4-124's size table, and the IBREAK rule) and Tables 5-177..5-180.

use lp_emu_core::Watchpoint;

use super::trap::{NUM_DBREAK, NUM_IBREAK};

/// `DBREAKC` bit 31: break on stores.
pub const DBREAKC_STORE: u32 = 1 << 31;
/// `DBREAKC` bit 30: break on loads.
pub const DBREAKC_LOAD: u32 = 1 << 30;
/// `DBREAKC` bits 5:0: the address mask — a run of ones from bit 5 down, one
/// zero per doubling of the covered block (`111111` = 1 byte, `111100` = 4,
/// `000000` = 64).
pub const DBREAKC_MASK: u32 = 0x3F;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BreakUnit {
    pub ibreaka: [u32; NUM_IBREAK],
    /// `IBREAKENABLE` (SR 96), the one debug register the RM does reset (to
    /// 0, Table 5-177).
    pub ibreakenable: u32,
    pub dbreaka: [u32; NUM_DBREAK],
    pub dbreakc: [u32; NUM_DBREAK],
}

impl BreakUnit {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            ibreaka: [0; NUM_IBREAK],
            ibreakenable: 0,
            dbreaka: [0; NUM_DBREAK],
            dbreakc: [0; NUM_DBREAK],
        }
    }

    /// The instruction breakpoint that matches a fetch at `pc`, if any (RM
    /// §4.7.6.3: `IBREAKA[i] == pc` with `IBREAKENABLE[i]` set).
    #[inline]
    #[must_use]
    pub fn ibreak_hit(&self, pc: u32) -> Option<usize> {
        (0..NUM_IBREAK).find(|&i| self.ibreakenable & (1 << i) != 0 && self.ibreaka[i] == pc)
    }

    /// The bus-facing form of DBREAK slot `slot`, or `None` when it is
    /// disarmed.
    ///
    /// The mapping onto [`Watchpoint`] is exact for every mask the RM
    /// defines (Table 4-124): `111111` is an exact address, and each
    /// further low zero doubles the block, which is NAPOT's shape — a run of
    /// `k-1` low one-bits under a zero names a naturally aligned `2^k`-byte
    /// block. A mask that is not a top-contiguous run is *undefined* by the
    /// RM ("the result of other combinations ... is not defined"); rather
    /// than approximate it, the slot is left disarmed and the fact logged,
    /// which is what the RV32 trigger unit does with a match mode it does
    /// not implement.
    #[must_use]
    pub fn watchpoint(&self, slot: usize) -> Option<Watchpoint> {
        let c = *self.dbreakc.get(slot)?;
        let on_store = c & DBREAKC_STORE != 0;
        let on_load = c & DBREAKC_LOAD != 0;
        if !(on_store || on_load) {
            return None;
        }
        let mask = c & DBREAKC_MASK;
        // `zeros` low address bits are ignored: the block is `2^zeros` bytes.
        let zeros = mask.trailing_zeros().min(6);
        if mask != (DBREAKC_MASK << zeros) & DBREAKC_MASK {
            log::warn!(
                "mach: DBREAKC{slot} mask {mask:#08b} is not a contiguous run (RM Table 4-124 \
                 defines no such block) — leaving the slot disarmed"
            );
            return None;
        }
        let a = self.dbreaka[slot];
        let (address, napot) = if zeros == 0 {
            (a, false)
        } else {
            let block = 1u32 << zeros;
            ((a & !(block - 1)) | ((block >> 1) - 1), true)
        };
        Some(Watchpoint {
            address,
            napot,
            on_store,
            on_load,
            on_execute: false,
        })
    }
}

impl Default for BreakUnit {
    fn default() -> Self {
        Self::new()
    }
}
