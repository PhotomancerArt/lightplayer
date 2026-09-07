//! The C6's interrupt matrix — the seam, not yet the semantics.
//!
//! **This is a P5 deliverable.** What P4 owns is the *plumbing*: a
//! [`CpuIntMatrix`] living on the bus, so `Bus::pending_cpu_interrupt` has
//! something to ask and the hart's polling point (c) can change an outcome.
//! What it deliberately does not own is any PLIC_MX behaviour, because a
//! matrix that guessed which source maps to which CPU interrupt would be
//! indistinguishable from one that had been measured.
//!
//! So [`Esp32C6IntMatrix`] asserts nothing, always. A machine built today
//! never takes an interrupt, which is honest: no peripheral models one yet.
//!
//! # What P5 has to decide, written down while it is fresh
//!
//! The matrix's *configuration* arrives as MMIO writes to two blocks:
//!
//! - `INTERRUPT_CORE0` at `0x6001_0000` — `core_0_intr_map[n]` at `+4n`,
//!   `n` in `0..=76`, each holding the CPU interrupt a source is routed to
//!   (31 = disabled). `_setup_interrupts` writes all 77 of them (discovery
//!   §1d).
//! - PLIC_MX — `MXINT_ENABLE`, `MXINT_TYPE`, `MXINT_CLEAR`, `MXINT_THRESH`
//!   (discovery §2b/§2g). `MXINT_CLEAR` is written read-modify-write and
//!   must read back **0**, or every interrupt ends up permanently held clear.
//!
//! Those writes land on a `Peripheral`, which the bus owns; the matrix is a
//! second object the bus owns. Keeping the two in step is P5's design
//! question and this phase does not pre-empt it. The two shapes that work:
//! the matrix register block mirrors each write into the matrix through a
//! shared handle, or the matrix is rebuilt from the block's registers each
//! time it is asked. The second is simpler and the ask is not hot (it happens
//! once per consumed side-band, not per instruction).

use lp_emu_esp_common::{CpuIntMatrix, IrqLines};

/// The number of peripheral interrupt **sources** the C6 declares
/// (`esp32c6-0.23.2/src/interrupt.rs`, 0..=76).
pub const SOURCE_COUNT: u16 = 77;

/// The CPU interrupt number that means "disabled" on the C6
/// (`esp-metadata-generated`: `interrupts.disabled_interrupt`).
pub const DISABLED_CPU_INTERRUPT: u8 = 31;

/// The C6's interrupt matrix. Asserts nothing until P5 fills it in.
#[derive(Clone, Copy, Debug, Default)]
pub struct Esp32C6IntMatrix {
    /// Set once P5 has real routing, so a reader of a run can tell "no
    /// interrupts happened" from "interrupts are not modelled yet".
    modelled: bool,
}

impl Esp32C6IntMatrix {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether this matrix has real routing behind it. `false` in P4.
    pub fn is_modelled(&self) -> bool {
        self.modelled
    }
}

impl CpuIntMatrix for Esp32C6IntMatrix {
    fn cpu_interrupt(&self, _hart: usize, _irq: &IrqLines) -> Option<u8> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stub_asserts_nothing_even_with_every_source_high() {
        let mut irq = IrqLines::new();
        for source in 0..SOURCE_COUNT {
            irq.set_level(source, true);
        }
        let matrix = Esp32C6IntMatrix::new();
        assert_eq!(matrix.cpu_interrupt(0, &irq), None);
        assert!(
            !matrix.is_modelled(),
            "P4 ships the seam, not the semantics"
        );
    }

    #[test]
    fn the_source_count_matches_the_pacs_interrupt_enum() {
        // 0..=76 in `esp32c6-0.23.2/src/interrupt.rs`, mirrored by
        // `core_0_intr_map[77]`. UART0 = 43 and SYSTIMER_TARGET0 = 57 are the
        // two P5/P6 reach for first.
        assert_eq!(SOURCE_COUNT, 77);
        assert!(43 < SOURCE_COUNT && 57 < SOURCE_COUNT);
        assert_eq!(DISABLED_CPU_INTERRUPT, 31);
    }
}
