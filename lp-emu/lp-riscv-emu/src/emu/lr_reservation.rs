//! The hart's load-reserved reservation: the state `lr.w` leaves for `sc.w`.
//!
//! *The RISC-V Instruction Set Manual, Volume I: Unprivileged ISA*, the "A"
//! extension, "Load-Reserved/Store-Conditional Instructions". `lr.w` reads a
//! word and registers a *reservation set* that holds it. `sc.w` writes only
//! while that reservation is still held and covers the word it names, and
//! writes `0` to `rd` on success and a nonzero code (`1`) on failure.
//! Executing an `sc.w` gives up the hart's reservation either way, so a second
//! `sc.w` after one `lr.w` always fails.
//!
//! This hart's reservation set is exactly the reserved word. The spec allows a
//! bigger one; a smaller one is not allowed.
//!
//! # What ends a reservation here
//!
//! - **Any `sc.w`**, to any address, whether or not it succeeded.
//! - **A trap the hart takes** (`MachineHart`'s trap entry). The spec lets an
//!   `sc.w` fail in more cases than it lists: a trap is one of the events its
//!   forward-progress guarantee for an LR/SC loop allows to happen. It is also
//!   the case that matters. An interrupt handler that runs between an
//!   interrupted `lr.w` and its `sc.w` may change the reserved word, and a
//!   resumed `sc.w` that still succeeds writes over that change. That is how
//!   esp-rtos's run queue lost a task on the emulated C6
//!   (`docs/defects/2026-10-09-the-emulated-sc-w-ignored-its-reservation.md`).
//! - **A newer `lr.w`**, which replaces it.
//!
//! The privileged spec also lets `mret` clear a reservation, without
//! requiring it. This model does not clear one there. Trap entry already
//! clears it, so the only reservation an `mret` could carry out of a handler
//! is one the handler took itself and never paired with an `sc.w`.
//!
//! Not modelled: a store by the same hart to the reserved word (the spec does
//! not require it to end the reservation), and a write by a device other than
//! a hart (DMA). The C6 emulator has one hart, so the spec's "store from
//! another hart" case cannot happen.

/// The word the most recent `lr.w` reserved, if the reservation still holds.
///
/// Architectural state: a hart's snapshot carries it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LrReservation {
    word: Option<u32>,
}

impl LrReservation {
    /// No reservation held, which is how a hart comes out of reset.
    #[must_use]
    pub const fn new() -> Self {
        Self { word: None }
    }

    /// `lr.w` at `address`: reserve that word, replacing any older reservation.
    #[inline]
    pub fn reserve(&mut self, address: u32) {
        self.word = Some(address);
    }

    /// `sc.w` at `address`: whether the store may happen. The reservation is
    /// given up whatever the answer.
    #[inline]
    pub fn take_for_store(&mut self, address: u32) -> bool {
        self.word.take() == Some(address)
    }

    /// Drop the reservation without a store: the hart took a trap.
    #[inline]
    pub fn clear(&mut self) {
        self.word = None;
    }

    /// The reserved word, if the reservation still holds.
    #[inline]
    #[must_use]
    pub const fn held(&self) -> Option<u32> {
        self.word
    }
}
