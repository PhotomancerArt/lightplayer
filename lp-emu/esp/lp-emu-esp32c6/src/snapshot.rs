//! A machine snapshot: everything a run's future depends on.
//!
//! In memory only — there is no file format in M3, on purpose. A snapshot
//! whose format is committed is a format that has to survive every change to
//! every peripheral's state blob; a snapshot that only ever travels inside
//! one process is free.
//!
//! # What "everything" means, and how it is checked
//!
//! Not by inspection. The test that matters is the one the phase brief names:
//! run to cycle N, snapshot, run on to M, restore, run to M **again**, and
//! require the second stretch to produce an identical trace. Anything the
//! snapshot forgot shows up there as a diverging line, because the trace is
//! a function of every access the machine makes.
//!
//! The pieces:
//!
//! | field | why it is here |
//! |---|---|
//! | `harts` | `MachineHart` is `Clone`, and the clone *is* the architectural state — registers, `pc`, the CSR file, the trigger unit, the cycle and instruction counters, the `wfi` park |
//! | `regions` | guest RAM, ROM and the flash window, byte for byte |
//! | `periph` | each peripheral's own blob, named, in registration order |
//! | `scalars` | the bus's clock, issuing PC, source levels, side-band and the unmapped counters — a restored run that reported different totals would not be the same run |
//! | `sched` | the live event queue, with the sequence numbers that break ties |
//! | `matrix` | the interrupt matrix's configuration (nothing, until P5) |
//! | `rng` | the seeded PRNG's position |
//! | `hook_calls`, `uart0`, `usb_sj` | observables a test compares |
//!
//! Watchpoints are **not** here: they live on the bus but they are derived
//! from the hart's trigger CSRs, so [`crate::machine::Esp32C6Machine::restore`]
//! re-arms them from the restored hart rather than carrying a second copy
//! that could disagree with it.

use lp_emu_core::sched::{Cycles, EventId};
use lp_emu_esp_common::SocBus;
use lp_emu_esp_common::bus::BusScalars;
use lp_riscv_emu::mach::MachineHart;

/// A machine's whole state at one cycle.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub harts: Vec<MachineHart<SocBus>>,
    pub regions: Vec<Vec<u8>>,
    pub periph: Vec<(String, Vec<u8>)>,
    pub scalars: BusScalars,
    pub sched: Vec<(Cycles, u64, EventId)>,
    pub matrix: Vec<u8>,
    pub rng: u64,
    pub hook_calls: u64,
    pub uart0: Vec<u8>,
    /// The USB-Serial-JTAG observation log — same rule as `uart0`: a restored
    /// run must not still hold bytes from a future it no longer has.
    pub usb_sj: Vec<u8>,
}

impl Snapshot {
    /// The guest cycle this snapshot was taken at.
    pub fn cycle(&self) -> Cycles {
        self.harts.first().map_or(0, |h| h.cycle_count())
    }

    /// Roughly how much host memory it holds. Region bytes dominate: the C6's
    /// map is about 17 MiB, most of it the two 8 MiB flash windows.
    pub fn bytes(&self) -> usize {
        self.regions.iter().map(Vec::len).sum::<usize>()
            + self.periph.iter().map(|(_, b)| b.len()).sum::<usize>()
            + self.uart0.len()
            + self.usb_sj.len()
    }
}
