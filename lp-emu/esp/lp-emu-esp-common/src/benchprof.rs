//! Guest-side counters for the speed probes — **off by default**, behind
//! `--features bench`, and never linked into a gate binary.
//!
//! The host-time question ("where do the seconds go?") is answered by the
//! `selfprof` pc sampler in the machine crates. This answers the *guest*-side
//! half, which a host sampler cannot: which guest addresses were fetched, and
//! which peripheral registers the guest touched how often. Three histograms,
//! all keyed by guest address:
//!
//! - [`BenchProf::fetches`] — one entry per **retired** instruction pc, since
//!   [`crate::SocBus`]'s `fetch_bytes` is called exactly once per Xtensa
//!   instruction and nothing caches in front of it today. From it fall the
//!   window-exception rate (fetches landing on `VECBASE + 0x00/0x40/…`) and
//!   the LOOP hotness (fetches inside a statically identified `loop` body).
//! - [`BenchProf::mmio_reads`] / [`BenchProf::mmio_writes`] — MMIO by
//!   register address, so "MMIO's top sites" is a sort rather than a guess.
//!   Names are resolved once, at dump time
//!   ([`crate::SocBus::bench_mmio_site`]), because `reg_name` is a table walk
//!   and this counter sits on every MMIO access.
//!
//! ⚠️ **This is a counter, not a timing.** The hash lookup per fetch costs
//! several times the interpreter's own work, so a `bench` build's *seconds*
//! mean nothing; only its ratios do. Take the seconds from a stock build in
//! the same window (`scripts/emu/bench-esp32v3.sh`) and the shares from here.

use std::collections::HashMap;

/// Every counter the `bench` feature keeps. One instance, on the bus.
#[derive(Default)]
pub struct BenchProf {
    /// Retired instruction pcs. One increment per `fetch_bytes`.
    pub fetches: HashMap<u32, u64>,
    /// `fetches`' total, kept separately so the dump never has to sum a map
    /// that may have been capped.
    pub fetch_total: u64,
    /// MMIO reads by address.
    pub mmio_reads: HashMap<u32, u64>,
    /// MMIO writes by address.
    pub mmio_writes: HashMap<u32, u64>,
    /// RAM (non-MMIO) loads and stores, so MMIO's share of *data* accesses is
    /// readable and not only its share of instructions.
    pub ram_reads: u64,
    pub ram_writes: u64,
}

impl BenchProf {
    pub fn mmio_total(&self) -> u64 {
        self.mmio_reads.values().sum::<u64>() + self.mmio_writes.values().sum::<u64>()
    }
}
