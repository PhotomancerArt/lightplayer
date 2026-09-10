//! A machine snapshot: everything a run's future depends on.
//!
//! In memory only — there is no file format in M3, on purpose. A snapshot
//! whose format is committed is a format that has to survive every change to
//! every peripheral's state blob; a snapshot that only ever travels inside
//! one process is free.
//!
//! | field | why it is here |
//! |---|---|
//! | `harts` | `XtHart` is `Clone`, and the clone *is* the architectural state — the AR file, `WindowBase`/`WindowStart`, `PS`, the special registers, the interrupt unit, the timers, the DBREAK slots and the counters |
//! | `stalled` | which cores are held. It is machine state, not hart state, and a restore that forgot it could resume with core 1 running |
//! | `regions` | guest RAM, the mask ROM and the flash windows, byte for byte |
//! | `periph` | each peripheral's own blob, named, in registration order. Empty in P2, and the field is here anyway so P3's first block does not change the struct |
//! | `scalars` | the bus's clock, issuing PC, side-band, the pin fabric and the unmapped counters — a restored run that reported different totals would not be the same run |
//! | `sched` | the live event queue, with the sequence numbers that break ties |
//! | `rng` | the seeded PRNG's position |
//! | `hook_calls`, `idle_skips` | observables a test compares |
//!
//! Watchpoints are **not** here: they live on the bus but they are derived
//! from the hart's `DBREAK` registers, so a restore re-arms them from the
//! restored hart rather than carrying a second copy that could disagree.

use lp_emu_core::sched::{Cycles, EventId};
use lp_emu_esp_common::SocBus;
use lp_emu_esp_common::bus::BusScalars;
use lp_xt_emu::mach::XtHart;

use crate::machine::CORES;

/// A machine's whole state at one cycle.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub harts: Vec<XtHart<SocBus>>,
    pub stalled: [bool; CORES],
    pub regions: Vec<Vec<u8>>,
    pub periph: Vec<(String, Vec<u8>)>,
    pub scalars: BusScalars,
    /// The interrupt matrix's routing (`CpuIntMatrix::save_state`). It lives
    /// on the bus rather than in a peripheral — DPORT is a *view* onto it —
    /// so a snapshot that only carried `periph` would restore a machine whose
    /// interrupt map was reset while the guest believed it was programmed.
    pub matrix: Vec<u8>,
    pub sched: Vec<(Cycles, u64, EventId)>,
    pub rng: u64,
    pub hook_calls: u64,
    pub idle_skips: u64,
}

impl Snapshot {
    /// The guest cycle this snapshot was taken at.
    pub fn cycle(&self) -> Cycles {
        self.harts.first().map_or(0, |h| h.cycle_count())
    }

    /// Roughly how much host memory it holds. Region bytes dominate: the
    /// classic's map is about 8 MiB, most of it the two flash cache windows.
    pub fn bytes(&self) -> usize {
        self.regions.iter().map(Vec::len).sum::<usize>()
            + self.periph.iter().map(|(_, b)| b.len()).sum::<usize>()
    }
}
