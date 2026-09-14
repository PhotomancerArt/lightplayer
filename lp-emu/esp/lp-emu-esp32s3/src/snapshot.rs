//! A machine snapshot: everything a run's future depends on.
//!
//! In memory only — there is no file format in M6, on purpose. A snapshot
//! whose format is committed is a format that has to survive every change to
//! every peripheral's state blob; a snapshot that only ever travels inside
//! one process is free.
//!
//! | field | why it is here |
//! |---|---|
//! | `harts` | `XtHart` is `Clone`, and the clone *is* the architectural state — the AR file, `WindowBase`/`WindowStart`, `PS`, the special registers, the interrupt unit, the timers, the DBREAK slots and the counters |
//! | `stalled` | which cores the **machine** holds. Machine state, not hart state, and a restore that forgot it could resume with core 1 running |
//! | `core_1_control` | which cores the **chip** holds (`SYSTEM.core_1_control_0`). A second input to the same question, and it lives behind an `Arc` the `SYSTEM` view shares with the machine, so it rides here rather than in that view's blob |
//! | `matrix` | the interrupt matrix's routing (`CpuIntMatrix::save_state`). It lives on the bus, not on either `INTERRUPT_CORE` view, so it is carried as its own field — the classic's snapshot does the same |
//! | `regions` | guest RAM, the mask ROM and the flash windows, byte for byte |
//! | `periph` | each peripheral's own blob, named, in registration order |
//! | `scalars` | the bus's clock, issuing PC, side-band and the unmapped counters — a restored run that reported different totals would not be the same run |
//! | `sched` | the live event queue, with the sequence numbers that break ties |
//! | `rng` | the seeded PRNG's position |
//! | `hook_calls`, `idle_skips`, `wfi_ends` | observables a test compares |
//! | `core_quantum` | the per-core window bound. A run's future depends on it — two quanta are two interleavings — so a restore adopts the snapshot's rather than keeping the machine's |
//! | `console` | what the console has said. Empty until P05 gives it a producer; carried from the first commit so the determinism test's comparison is unchanged when it stops being empty |
//!
//! **Watchpoints are not here**: they live on the bus but are derived from
//! the hart's `DBREAK` registers, so a restore re-arms them from the restored
//! hart rather than carrying a second copy that could disagree. See
//! [`crate::machine::Machine::restore`] for the half of that fix `lp-xt-emu`
//! does not yet let this crate make.
//!
//! ⚠️ **The SRAM1 RAM alias is not here either, and does not need to be.** The
//! bus's own `save_regions`/`restore_regions` carry the one store both doors
//! address, and the alias table itself is configuration — rebuilt by
//! [`crate::bus_setup::build`] on every machine — not state. A snapshot that
//! carried the table would be able to restore a *different map*, which is not
//! a thing this machine should be able to do.

use lp_emu_core::sched::{Cycles, EventId};
use lp_emu_esp_common::SocBus;
use lp_emu_esp_common::bus::BusScalars;
use lp_xt_emu::mach::XtHart;

use crate::machine::{CORES, CoreOneControl};

/// A machine's whole state at one cycle.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub harts: Vec<XtHart<SocBus>>,
    pub stalled: [bool; CORES],
    /// `SYSTEM.core_1_control_0` — see the module docs.
    pub core_1_control: CoreOneControl,
    pub regions: Vec<Vec<u8>>,
    pub periph: Vec<(String, Vec<u8>)>,
    /// The interrupt matrix's routing — see the module docs.
    pub matrix: Vec<u8>,
    pub scalars: BusScalars,
    pub sched: Vec<(Cycles, u64, EventId)>,
    pub rng: u64,
    pub hook_calls: u64,
    pub idle_skips: u64,
    /// How many windows each core has ended in `waiti`.
    pub wfi_ends: [u64; CORES],
    /// The per-core window bound the run was using (`--core-quantum`).
    pub core_quantum: u64,
    /// Everything the console had said at this cycle.
    pub console: Vec<u8>,
    /// Everything the mask ROM's UART0 console had put on the wire (P06).
    pub uart0: Vec<u8>,
    /// What the pads had decoded (P07): one WS281x decoder per routed pad,
    /// its completed frames, the routing and the edge counts. A decoder
    /// caught **mid-frame** is state — one restored without its half-shifted
    /// bits would resume a frame that never existed.
    pub pins: crate::machine::PinState,
}

impl Snapshot {
    /// The guest cycle this snapshot was taken at: the machine's one clock,
    /// which is the furthest any hart has got.
    pub fn cycle(&self) -> Cycles {
        self.harts
            .iter()
            .map(|h| h.cycle_count())
            .max()
            .unwrap_or(0)
    }

    /// Roughly how much host memory it holds. Region bytes dominate, and on
    /// this chip they dominate hard: the two flash cache windows are 32 MiB
    /// each because `memory.x` declares 32 MiB windows, so a snapshot is
    /// ~64 MiB. That is the price of a map that says what the linker script
    /// says; P06, which serves those windows through the MMU instead of
    /// pre-filling them, is where it would change.
    pub fn bytes(&self) -> usize {
        self.regions.iter().map(Vec::len).sum::<usize>()
            + self.periph.iter().map(|(_, b)| b.len()).sum::<usize>()
            + self.console.len()
    }
}
