//! The four M-mode triggers — esp-rtos's stack guard, in hardware.
//!
//! Shape and bit numbering from *RISC-V External Debug Support*, version
//! 1.0.0, §5.2 (`Trigger Registers`) and §5.2.9 (`Match Control`,
//! `mcontrol`); the values the firmware actually writes are from the M3
//! discovery §4 (`esp-hal-1.1.1/src/debugger.rs`).
//!
//! What esp-hal does, and therefore what this must support (discovery §4a):
//!
//! ```text
//! set_watchpoint(id, addr, 4):   csrw 0x7a0, id            // tselect
//!                                csrw 0x7a5, 0x8           // tcontrol.mte
//!                                csrw 0x7a1, 0xC2          // tdata1: store|m|match=NAPOT
//!                                csrw 0x7a2, (addr&!3)|1   // tdata2: 4-byte NAPOT
//! clear_watchpoint(id):          csrw  0x7a0, id
//!                                csrrw old, 0x7a1, 0        // atomic read-and-zero
//!                                csrr  old2, 0x7a2
//! watchpoint_hit(id):            csrw 0x7a0, id ; csrr 0x7a1 ; test bit 20
//! ```
//!
//! The unit does not check addresses. Every write re-derives the affected
//! slot's [`Watchpoint`] and hands it to the bus, so the *bus* checks — and
//! only for slots that are actually armed. That is the whole point of
//! [`lp_emu_core::Bus::set_watchpoint`].

use lp_emu_core::Watchpoint;

/// `tdata1.load`, bit 0 — fire on a matching load.
pub const TDATA1_LOAD: u32 = 1 << 0;
/// `tdata1.store`, bit 1 — fire on a matching store. The bit esp-hal sets.
pub const TDATA1_STORE: u32 = 1 << 1;
/// `tdata1.execute`, bit 2 — fire on a matching instruction fetch.
pub const TDATA1_EXECUTE: u32 = 1 << 2;
/// `tdata1.m`, bit 6 — the trigger is enabled in M-mode. esp-hal sets it.
pub const TDATA1_M: u32 = 1 << 6;
/// `tdata1.match`, bits 10:7. `0` = exact address, `1` = NAPOT.
pub const TDATA1_MATCH_SHIFT: u32 = 7;
/// `tdata1.action`, bits 15:12. Only `0` — "raise a breakpoint exception" —
/// is implemented; a debug-mode action has nowhere to go in this hart.
pub const TDATA1_ACTION_SHIFT: u32 = 12;
/// `tdata1.hit`, bit 20 — set by the hart when the trigger fires, cleared by
/// any software write. `ExceptionHandler` probes it to name the offender
/// (discovery §4d).
pub const TDATA1_HIT: u32 = 1 << 20;
/// `tdata1.type`, bits 31:28. WARL-forced to `2` (`mcontrol`) on read:
/// esp-hal never writes the field and relies on exactly that (discovery §4b).
pub const TDATA1_TYPE_MCONTROL: u32 = 2 << 28;

/// The bits a `tdata1` write may set: `load`/`store`/`execute`/`u`, `m`,
/// `match`, `action`, `hit`, `dmode`. Everything else is WARL-zero.
pub const TDATA1_WRITE_MASK: u32 = 0x0810_F7CF;

/// `tcontrol.mte`, bit 3 — M-mode trigger enable. With it clear, no trigger
/// fires in M-mode, whatever `tdata1` says.
pub const TCONTROL_MTE: u32 = 1 << 3;
/// `tcontrol.mpte`, bit 7 — the saved `mte`.
pub const TCONTROL_MPTE: u32 = 1 << 7;
/// The writable bits of `tcontrol`.
pub const TCONTROL_WRITE_MASK: u32 = TCONTROL_MTE | TCONTROL_MPTE;

/// How many triggers the hart implements. `esp-hal`'s `set_watchpoint`
/// asserts `id < 4` and never probes for the count (discovery §4a).
pub const TRIGGER_COUNT: usize = 4;

/// One trigger's `tdata1`/`tdata2` pair.
#[derive(Clone, Copy, Debug, Default)]
struct Trigger {
    /// Stored already masked to [`TDATA1_WRITE_MASK`]; the `type` field is
    /// added on read.
    tdata1: u32,
    tdata2: u32,
}

/// The trigger unit: `tselect`, `tcontrol`, and four `tdata1`/`tdata2` pairs.
#[derive(Clone, Debug, Default)]
pub struct TriggerUnit {
    tselect: u32,
    tcontrol: u32,
    triggers: [Trigger; TRIGGER_COUNT],
}

impl TriggerUnit {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            tselect: 0,
            tcontrol: 0,
            triggers: [Trigger {
                tdata1: 0,
                tdata2: 0,
            }; TRIGGER_COUNT],
        }
    }

    /// The currently selected slot, always in range.
    #[inline]
    #[must_use]
    pub const fn selected(&self) -> usize {
        self.tselect as usize
    }

    #[inline]
    #[must_use]
    pub const fn tselect(&self) -> u32 {
        self.tselect
    }

    /// WARL: `tselect` holds only implemented trigger indices, so a write of
    /// 4 or more clamps to the last one (spec §5.2.2).
    #[inline]
    pub fn write_tselect(&mut self, value: u32) {
        self.tselect = value.min(TRIGGER_COUNT as u32 - 1);
    }

    /// `tdata1` of the selected trigger, with `type` forced to `mcontrol`.
    #[inline]
    #[must_use]
    pub const fn tdata1(&self) -> u32 {
        self.triggers[self.selected()].tdata1 | TDATA1_TYPE_MCONTROL
    }

    #[inline]
    #[must_use]
    pub const fn tdata2(&self) -> u32 {
        self.triggers[self.selected()].tdata2
    }

    #[inline]
    #[must_use]
    pub const fn tcontrol(&self) -> u32 {
        self.tcontrol
    }

    /// Write `tdata1`. The written `hit` bit is kept verbatim, which is what
    /// makes `clear_watchpoint`'s `csrrw tdata1, 0` clear a stale hit.
    #[inline]
    pub fn write_tdata1(&mut self, value: u32) {
        let slot = self.selected();
        self.triggers[slot].tdata1 = value & TDATA1_WRITE_MASK;
    }

    #[inline]
    pub fn write_tdata2(&mut self, value: u32) {
        let slot = self.selected();
        self.triggers[slot].tdata2 = value;
    }

    #[inline]
    pub fn write_tcontrol(&mut self, value: u32) {
        self.tcontrol = value & TCONTROL_WRITE_MASK;
    }

    /// Mark trigger `slot` as having fired. The hart calls this when the bus
    /// reports [`lp_emu_core::MemoryError::Watchpoint`], before delivering
    /// the breakpoint exception.
    #[inline]
    pub fn set_hit(&mut self, slot: usize) {
        if let Some(t) = self.triggers.get_mut(slot) {
            t.tdata1 |= TDATA1_HIT;
        }
    }

    /// True when trigger `slot` has fired since software last wrote it.
    #[inline]
    #[must_use]
    pub fn hit(&self, slot: usize) -> bool {
        self.triggers
            .get(slot)
            .is_some_and(|t| t.tdata1 & TDATA1_HIT != 0)
    }

    /// The bus-facing form of trigger `slot`, or `None` when the slot is
    /// disarmed.
    ///
    /// A slot is disarmed when `tcontrol.mte` is clear (globally), when
    /// `tdata1.m` is clear, when no access-type bit is set, or when the
    /// trigger asks for something this hart does not implement — a non-zero
    /// `action`, or a `match` mode other than exact (0) or NAPOT (1). The
    /// unsupported cases are logged rather than silently approximated: an
    /// approximated watchpoint is a watchpoint that reports the wrong
    /// address.
    #[must_use]
    pub fn watchpoint(&self, slot: usize) -> Option<Watchpoint> {
        if self.tcontrol & TCONTROL_MTE == 0 {
            return None;
        }
        let t = self.triggers.get(slot)?;
        if t.tdata1 & TDATA1_M == 0 {
            return None;
        }

        let action = (t.tdata1 >> TDATA1_ACTION_SHIFT) & 0xF;
        if action != 0 {
            log::warn!(
                "mach: trigger {slot} requests action {action}; only action 0 (breakpoint \
                 exception) is implemented — leaving the slot disarmed"
            );
            return None;
        }

        let napot = match (t.tdata1 >> TDATA1_MATCH_SHIFT) & 0xF {
            0 => false,
            1 => true,
            other => {
                log::warn!(
                    "mach: trigger {slot} requests match mode {other}; only 0 (exact) and 1 \
                     (NAPOT) are implemented — leaving the slot disarmed"
                );
                return None;
            }
        };

        let on_load = t.tdata1 & TDATA1_LOAD != 0;
        let on_store = t.tdata1 & TDATA1_STORE != 0;
        let on_execute = t.tdata1 & TDATA1_EXECUTE != 0;
        if !(on_load || on_store || on_execute) {
            return None;
        }

        Some(Watchpoint {
            address: t.tdata2,
            napot,
            on_store,
            on_load,
            on_execute,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact four-CSR sequence from `esp-hal`'s `set_watchpoint`.
    fn arm_stack_guard(u: &mut TriggerUnit, id: u32, guard: u32) {
        u.write_tselect(id);
        u.write_tcontrol(TCONTROL_MTE);
        u.write_tdata1(0xC2);
        u.write_tdata2((guard & !3) | 1);
    }

    #[test]
    fn esp_hal_sequence_arms_a_napot_store_watchpoint() {
        let mut u = TriggerUnit::new();
        arm_stack_guard(&mut u, 0, 0x4080_7F00);
        let wp = u.watchpoint(0).expect("armed");
        assert!(wp.on_store && !wp.on_load && !wp.on_execute);
        assert!(wp.napot);
        assert_eq!(wp.address, 0x4080_7F01);
        assert!(u.watchpoint(1).is_none(), "other slots stay disarmed");
    }

    #[test]
    fn tdata1_reads_back_as_mcontrol_and_clear_disarms() {
        let mut u = TriggerUnit::new();
        arm_stack_guard(&mut u, 0, 0x4080_7F00);
        assert_eq!(u.tdata1(), TDATA1_TYPE_MCONTROL | 0xC2);
        // clear_watchpoint: csrrw tdata1, 0
        u.write_tdata1(0);
        assert!(u.watchpoint(0).is_none());
        assert_eq!(u.tdata1(), TDATA1_TYPE_MCONTROL);
    }

    #[test]
    fn mte_gates_every_slot_and_a_write_clears_hit() {
        let mut u = TriggerUnit::new();
        arm_stack_guard(&mut u, 0, 0x4080_7F00);
        u.set_hit(0);
        assert!(u.hit(0));
        assert_eq!(u.tdata1() & TDATA1_HIT, TDATA1_HIT);

        u.write_tcontrol(0);
        assert!(u.watchpoint(0).is_none(), "mte == 0 disarms everything");

        u.write_tdata1(0xC2);
        assert!(!u.hit(0), "a software write to tdata1 clears hit");
    }

    #[test]
    fn tselect_clamps_to_the_last_implemented_trigger() {
        let mut u = TriggerUnit::new();
        u.write_tselect(9);
        assert_eq!(u.tselect(), 3);
    }
}
