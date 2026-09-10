//! Interrupt state and level selection: `INTERRUPT`/`INTSET`/`INTCLEAR`,
//! `INTENABLE`, the per-line type and level table, and the pick.
//!
//! Semantics: ISA RM §4.4.4 (Interrupt Option — the interrupt types of Table
//! 4-73 and how each is set and cleared, the level-1 process), §4.4.5.3-4
//! (the high-priority process and `checkInterrupts`), and Tables 5-169..172
//! for the four registers.
//!
//! The hart resolves interrupts itself, because `INTENABLE` is a **CPU**
//! register the SoC's interrupt matrix cannot see. What the matrix hands the
//! hart is an asserted bitmask ([`InterruptUnit::set_external`]); each line's
//! *level* and *type* are core configuration, supplied by the machine through
//! [`IntLine`] — putting them in the hart as constants would be the SoC
//! knowledge this crate must not hold.

use super::trap::{EXCM_LEVEL, NMI_LEVEL, NUM_INTERRUPTS};

/// How one CPU interrupt line is set and cleared (RM Table 4-73).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum IntKind {
    /// Not configured: never pending, its `INTENABLE` bit is meaningless.
    #[default]
    Unused,
    /// Level-sensitive external input: pending exactly while the line is
    /// asserted; cleared at the device, not writable.
    Level,
    /// Edge-triggered external input: latched on the rising edge, cleared by
    /// `wsr.intclear`.
    Edge,
    /// Software: set by `wsr.intset`, cleared by `wsr.intclear`.
    Software,
    /// Internal timer `n` (`CCOMPARE[n]`): set when `CCOUNT == CCOMPARE[n]`,
    /// cleared by writing `CCOMPARE[n]`. Not `INTCLEAR`.
    Timer(u8),
    /// The NMI: rising edge, no `INTERRUPT`/`INTENABLE` bit, taken
    /// regardless of `CINTLEVEL` (RM §4.4.5.3), cleared by being taken.
    Nmi,
}

/// One entry of the 32-line core configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct IntLine {
    /// The line's fixed priority level, 1..=7 (7 = NMI). Ignored for
    /// [`IntKind::Unused`].
    pub level: u8,
    pub kind: IntKind,
}

impl IntLine {
    pub const UNUSED: IntLine = IntLine {
        level: 0,
        kind: IntKind::Unused,
    };

    #[must_use]
    pub const fn new(level: u8, kind: IntKind) -> Self {
        Self { level, kind }
    }
}

/// The interrupt to take, as [`InterruptUnit::select`] reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Take {
    /// A level-1 interrupt: through the general vector with
    /// `EXCCAUSE = Level1InterruptCause`.
    Level1,
    /// A level 2..=6 interrupt (or the NMI at 7): through
    /// `InterruptVector[level]`.
    Level(u8),
}

/// The pending/enable state and the pick.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterruptUnit {
    lines: [IntLine; NUM_INTERRUPTS],
    /// Bits that are pending because they were *latched*: edge, software
    /// and timer lines. Level lines are never latched — they follow
    /// `external`.
    latched: u32,
    /// The bitmask the machine (or the bus, at poll point (c)) last
    /// asserted. Level lines read it live; edge and NMI lines latch its
    /// rising edges.
    external: u32,
    /// `INTENABLE` (SR 228).
    pub intenable: u32,
    /// The NMI is pending (rising edge seen, not yet taken).
    nmi_pending: bool,
    /// Derived: which lines are `Level` (read live), which are `Timer(n)`.
    level_lines: u32,
    timer_bits: [u32; super::trap::NUM_TIMERS],
}

impl InterruptUnit {
    #[must_use]
    pub fn new(lines: [IntLine; NUM_INTERRUPTS]) -> Self {
        let mut level_lines = 0u32;
        let mut timer_bits = [0u32; super::trap::NUM_TIMERS];
        for (i, line) in lines.iter().enumerate() {
            match line.kind {
                IntKind::Level => level_lines |= 1 << i,
                IntKind::Timer(n) => {
                    if let Some(bit) = timer_bits.get_mut(usize::from(n)) {
                        *bit |= 1 << i;
                    }
                }
                _ => {}
            }
        }
        Self {
            lines,
            latched: 0,
            external: 0,
            // Undefined at reset (RM Table 5-172); zero is the choice that
            // makes a hart that forgot to enable anything take nothing.
            intenable: 0,
            nmi_pending: false,
            level_lines,
            timer_bits,
        }
    }

    #[inline]
    #[must_use]
    pub const fn lines(&self) -> &[IntLine; NUM_INTERRUPTS] {
        &self.lines
    }

    /// The `INTERRUPT` register (SR 226, read): level lines as asserted
    /// right now, plus every latched edge/software/timer bit.
    #[inline]
    #[must_use]
    pub const fn pending(&self) -> u32 {
        (self.external & self.level_lines) | self.latched
    }

    #[inline]
    #[must_use]
    pub const fn external(&self) -> u32 {
        self.external
    }

    /// Replace the asserted-line bitmask. Level lines follow it; an edge line
    /// or the NMI latches on a 0->1 transition against the previous mask.
    pub fn set_external(&mut self, mask: u32) {
        let rising = mask & !self.external;
        self.external = mask;
        for i in 0..NUM_INTERRUPTS {
            let bit = 1u32 << i;
            if rising & bit == 0 {
                continue;
            }
            match self.lines[i].kind {
                IntKind::Edge => self.latched |= bit,
                IntKind::Nmi => self.nmi_pending = true,
                _ => {}
            }
        }
    }

    /// `wsr.intset` (Table 5-170): only software lines can be set.
    pub fn intset(&mut self, value: u32) {
        for i in 0..NUM_INTERRUPTS {
            let bit = 1u32 << i;
            if value & bit != 0 && self.lines[i].kind == IntKind::Software {
                self.latched |= bit;
            }
        }
    }

    /// `wsr.intclear` (Table 5-171): clears edge and software lines. Timer
    /// lines are cleared by writing their `CCOMPARE`, never by this (RM
    /// §4.4.6.2); level lines are cleared at the device.
    pub fn intclear(&mut self, value: u32) {
        for i in 0..NUM_INTERRUPTS {
            let bit = 1u32 << i;
            if value & bit != 0 && matches!(self.lines[i].kind, IntKind::Edge | IntKind::Software) {
                self.latched &= !bit;
            }
        }
    }

    /// Timer `n` matched: raise its line.
    #[inline]
    pub fn timer_fired(&mut self, n: usize) {
        if let Some(bit) = self.timer_bits.get(n) {
            self.latched |= *bit;
        }
    }

    /// `CCOMPARE[n]` was written: its interrupt request is cleared.
    #[inline]
    pub fn timer_cleared(&mut self, n: usize) {
        if let Some(bit) = self.timer_bits.get(n) {
            self.latched &= !*bit;
        }
    }

    #[inline]
    #[must_use]
    pub const fn nmi_pending(&self) -> bool {
        self.nmi_pending
    }

    /// The NMI was taken; the hardware clears it (Table 4-73).
    #[inline]
    pub fn nmi_taken(&mut self) {
        self.nmi_pending = false;
    }

    /// `CINTLEVEL` for a PS value (RM §4.4.1.4):
    /// `max(PS.INTLEVEL, PS.EXCM * EXCM_LEVEL)`.
    #[inline]
    #[must_use]
    pub const fn cintlevel(ps: u32) -> u8 {
        let intlevel = (ps & super::sr::PS_INTLEVEL_MASK) as u8;
        if ps & super::sr::PS_EXCM != 0 && intlevel < EXCM_LEVEL {
            EXCM_LEVEL
        } else {
            intlevel
        }
    }

    /// Pick the interrupt to take, or `None`.
    ///
    /// The RM's `checkInterrupts` (§4.4.5.4): the NMI first, regardless of
    /// `CINTLEVEL` and `INTENABLE`; then the **highest level** that has a
    /// line (a) pending, (b) enabled in `INTENABLE`, and (c) strictly above
    /// `CINTLEVEL`; level 1 last, through the general vector.
    #[must_use]
    pub fn select(&self, ps: u32) -> Option<Take> {
        if self.nmi_pending {
            return Some(Take::Level(NMI_LEVEL));
        }
        let candidates = self.pending() & self.intenable;
        if candidates == 0 {
            return None;
        }
        let cintlevel = Self::cintlevel(ps);
        let mut best: u8 = 0;
        for i in 0..NUM_INTERRUPTS {
            if candidates & (1 << i) == 0 {
                continue;
            }
            let line = self.lines[i];
            if matches!(line.kind, IntKind::Unused | IntKind::Nmi) {
                continue;
            }
            if line.level > cintlevel && line.level > best {
                best = line.level;
            }
        }
        match best {
            0 => None,
            1 => Some(Take::Level1),
            n => Some(Take::Level(n)),
        }
    }
}
