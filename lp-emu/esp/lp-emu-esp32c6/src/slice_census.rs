//! M7b P4: a census of what bounds a hart slice, and what a slice boundary
//! does.
//!
//! **Off unless `LP_EMU_SLICE_CENSUS` is set**, in the shape of
//! [`crate::jit::mmio_census`] and for the same reasons: it is a diagnostic
//! nobody runs by accident, it allocates per distinct scheduler event, and it
//! perturbs the wall clock the rest of the run reports.
//!
//! # The question it answers
//!
//! [`crate::machine::Esp32C6Machine::run_until`] runs the hart in slices, and
//! a slice's length is the **minimum** of six terms: the
//! [`MAX_SLICE_CYCLES`](crate::machine::MAX_SLICE_CYCLES) cap, the
//! scheduler's next live deadline, the next `--probe`, the next host service,
//! the strict-bus cap, and the run's own stop cycle. Every boundary then pays
//! the same fixed list of work — due events, pins, the radio log, the host,
//! the interrupt resample, the cache refill, the strict check, the request
//! check — whether or not any of it has anything to do.
//!
//! Before M7b P4 the slice *count* on `render-basic` t2 was an inference:
//! 2,704,058 budget exits × 2.82 instructions each said "at most 2.7 M
//! slices over 880 M cycles". This census counts them directly, says which
//! term bounded each one, and — when it was the scheduler — which event.
//!
//! # Why a thread-local and not a field
//!
//! The same reason the MMIO census is one: the report is printed by the
//! binary after the run, from a `&Esp32C6Machine`, and a census kept in the
//! machine would have to be threaded through every constructor a test uses.
//! The emulator is single-threaded on every target this runs on
//! (`wasm32-wasip1` has no threads at all), so a `thread_local!` is the run.
//!
//! # What it does *not* do
//!
//! It does not change one arithmetic operation in the slice's deadline. The
//! run loop computes the same `deadline` it always did; the census is told
//! which of the `min` terms achieved it, in the order the loop applies them,
//! and is called only when [`on`] is true.

use std::cell::RefCell;
use std::collections::BTreeMap;

use lp_emu_core::sched::{Cycles, EventId};
use lp_emu_esp_common::bus::SocBus;
use lp_emu_esp_common::periph::Peripheral;
use lp_emu_esp_common::{event_local, event_peripheral};

/// Which term of the `min` set the slice's deadline.
///
/// The order is the order [`run_until`](crate::machine::Esp32C6Machine::run_until)
/// applies the terms in, and a tie is credited to the **first** term that
/// reached the value — the same rule the loop's chain of `min` calls has.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Bound {
    /// `MAX_SLICE_CYCLES` — the 8,192-cycle block budget.
    Cap,
    /// The run's own `--until` / deadline cycle.
    StopCycle,
    /// `SocBus::sched.next_deadline()`.
    Scheduler,
    /// The next `--probe`'s cycle.
    Probe,
    /// `next_host_service` — a scripted command, the socket poll cadence, or
    /// the pin script.
    HostService,
    /// `--strict-bus`'s 1,024-cycle cap.
    Strict,
}

impl Bound {
    const ALL: [Bound; 6] = [
        Bound::Cap,
        Bound::StopCycle,
        Bound::Scheduler,
        Bound::Probe,
        Bound::HostService,
        Bound::Strict,
    ];

    fn label(self) -> &'static str {
        match self {
            Bound::Cap => "cap (MAX_SLICE_CYCLES)",
            Bound::StopCycle => "stop cycle",
            Bound::Scheduler => "scheduler",
            Bound::Probe => "probe",
            Bound::HostService => "host service",
            Bound::Strict => "strict-bus cap",
        }
    }
}

/// How the slice ended — the `SliceEnd` the hart handed back.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum End {
    BudgetExhausted,
    BusYield,
    Wfi,
    Ebreak,
    Fault,
}

impl End {
    const ALL: [End; 5] = [
        End::BudgetExhausted,
        End::BusYield,
        End::Wfi,
        End::Ebreak,
        End::Fault,
    ];

    fn label(self) -> &'static str {
        match self {
            End::BudgetExhausted => "BudgetExhausted",
            End::BusYield => "BusYield",
            End::Wfi => "Wfi",
            End::Ebreak => "Ebreak",
            End::Fault => "Fault",
        }
    }
}

/// One bit of boundary work, and the question "did this slice's boundary do
/// anything at all" is the OR of them.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Work {
    /// `SocBus::run_due_events` dispatched at least one event.
    DueEvent,
    /// `drain_pins` found at least one edge on the wire.
    PinEdge,
    /// `service_host` did something — a scripted command, a socket edge.
    HostService,
    /// `code_writes_pending()` was true: a host-side write to guest code.
    CodeWrite,
    /// The external interrupt number the hart is told about changed.
    ExternalChanged,
    /// `take_sideband()` was set at the boundary.
    Sideband,
}

impl Work {
    const ALL: [Work; 6] = [
        Work::DueEvent,
        Work::PinEdge,
        Work::HostService,
        Work::CodeWrite,
        Work::ExternalChanged,
        Work::Sideband,
    ];

    fn label(self) -> &'static str {
        match self {
            Work::DueEvent => "a due event fired",
            Work::PinEdge => "a pin edge drained",
            Work::HostService => "a host service ran",
            Work::CodeWrite => "a code write landed",
            Work::ExternalChanged => "the external interrupt number changed",
            Work::Sideband => "the bus had a side-band flag set",
        }
    }
}

/// How many length buckets the histogram has: `1`, `2..=3`, `4..=7`, … up to
/// "8,192 and over", which is 15 rows on a run whose cap is 8,192.
const BUCKETS: usize = 22;

#[derive(Default)]
struct Census {
    slices: u64,
    cycles: u64,
    /// Slice length in cycles, bucketed by `64 - len.leading_zeros()`, so
    /// bucket `b` holds lengths in `2^(b-1) ..= 2^b - 1` and bucket 0 holds
    /// the (impossible) zero-length slice.
    lengths: [u64; BUCKETS],
    by_bound: BTreeMap<Bound, u64>,
    /// Only for `Bound::Scheduler`: which event owned the deadline.
    by_event: BTreeMap<u32, u64>,
    by_end: BTreeMap<End, u64>,
    by_work: BTreeMap<Work, u64>,
    /// Slices whose boundary did *none* of the [`Work`] items.
    idle_boundaries: u64,
    /// Total events dispatched by `run_due_events`, and total pin edges.
    due_events: u64,
    pin_edges: u64,
    /// The scheduler's heap, as `next_deadline` sees it: how many entries it
    /// scans, and how many of those are live. The gap between the two is the
    /// tombstone load, which is what decides whether the scan is long or
    /// merely expensive per entry.
    heap_entries: u64,
    heap_live: u64,
    heap_entries_max: u64,
}

thread_local! {
    static CENSUS: RefCell<Option<Census>> = const { RefCell::new(None) };
}

/// Whether the census is on, read once from the environment.
///
/// A `OnceLock` rather than a per-call `var_os`: this is consulted once per
/// slice — millions of times on `render-basic` t2 — and an environment
/// lookup there would be the measurement's own cost.
#[must_use]
pub fn on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("LP_EMU_SLICE_CENSUS").is_some())
}

/// Everything one slice has to say, filled in by the run loop as it goes.
#[derive(Copy, Clone, Debug)]
pub struct Slice {
    pub bound: Bound,
    pub event: Option<EventId>,
    pub length: Cycles,
    pub end: End,
    pub due_events: u32,
    pub pin_edges: u32,
    pub host_service: bool,
    pub code_write: bool,
    pub external_changed: bool,
    pub sideband: bool,
    /// `Scheduler::pending()` and `Scheduler::live()` at the moment
    /// `next_deadline` was asked.
    pub heap_entries: u32,
    pub heap_live: u32,
}

/// Record one slice. Cheap enough to call unconditionally; it tests [`on`]
/// first, exactly as [`crate::jit::mmio_census::note`] does.
pub fn note(s: &Slice) {
    if !on() {
        return;
    }
    CENSUS.with(|c| {
        let mut slot = c.borrow_mut();
        let c = slot.get_or_insert_with(Census::default);
        c.slices += 1;
        c.cycles += s.length;
        let bucket = (64 - s.length.leading_zeros()) as usize;
        c.lengths[bucket.min(BUCKETS - 1)] += 1;
        *c.by_bound.entry(s.bound).or_default() += 1;
        if let Some(id) = s.event {
            *c.by_event.entry(id.0).or_default() += 1;
        }
        *c.by_end.entry(s.end).or_default() += 1;
        c.due_events += u64::from(s.due_events);
        c.pin_edges += u64::from(s.pin_edges);
        let mut any = false;
        let mut hit = |on: bool, w: Work| {
            if on {
                any = true;
                *c.by_work.entry(w).or_default() += 1;
            }
        };
        hit(s.due_events > 0, Work::DueEvent);
        hit(s.pin_edges > 0, Work::PinEdge);
        hit(s.host_service, Work::HostService);
        hit(s.code_write, Work::CodeWrite);
        hit(s.external_changed, Work::ExternalChanged);
        hit(s.sideband, Work::Sideband);
        if !any {
            c.idle_boundaries += 1;
        }
        c.heap_entries += u64::from(s.heap_entries);
        c.heap_live += u64::from(s.heap_live);
        c.heap_entries_max = c.heap_entries_max.max(u64::from(s.heap_entries));
    });
}

/// The census as report lines, or `None` when it was never turned on.
///
/// `bus` turns an [`EventId`] back into its peripheral's own name, the same
/// resolution `SocBus::run_due_events` dispatches through — so there is no
/// second copy of the event map here to drift away from the machine's.
#[must_use]
pub fn report(bus: &SocBus) -> Option<String> {
    CENSUS.with(|c| {
        let slot = c.borrow();
        let c = slot.as_ref()?;
        let n = c.slices.max(1);
        let of = |x: u64| 100.0 * x as f64 / n as f64;
        let mut out = String::new();
        out.push_str(&format!(
            "slice census: {} slice(s) over {} cycle(s) — {:.1} cycles per slice\n",
            c.slices,
            c.cycles,
            c.cycles as f64 / n as f64,
        ));

        for b in Bound::ALL {
            let k = c.by_bound.get(&b).copied().unwrap_or(0);
            out.push_str(&format!(
                "slice census: bound by {:<24} {k:>12} ({:>6.2} %)\n",
                b.label(),
                of(k),
            ));
        }

        // Which event owned the deadline, when it was the scheduler.
        let mut events: Vec<(u32, u64)> = c.by_event.iter().map(|(&k, &v)| (k, v)).collect();
        events.sort_by_key(|(_, v)| std::cmp::Reverse(*v));
        for (raw, k) in events.iter().take(16) {
            let id = EventId(*raw);
            let index = event_peripheral(id);
            let name = bus.peripheral(index).map_or("?", Peripheral::name);
            out.push_str(&format!(
                "slice census: scheduler event {:<22} {k:>12} ({:>6.2} %)\n",
                format!("{name}/local {}", event_local(id)),
                of(*k),
            ));
        }

        for e in End::ALL {
            let k = c.by_end.get(&e).copied().unwrap_or(0);
            out.push_str(&format!(
                "slice census: ended {:<29} {k:>12} ({:>6.2} %)\n",
                e.label(),
                of(k),
            ));
        }

        for (b, k) in c.lengths.iter().enumerate() {
            if *k == 0 {
                continue;
            }
            let (lo, hi) = if b == 0 {
                (0u64, 0u64)
            } else {
                (1u64 << (b - 1), (1u64 << (b - 1)).saturating_mul(2) - 1)
            };
            out.push_str(&format!(
                "slice census: length {:>10}..={:<10} {k:>12} ({:>6.2} %)\n",
                lo,
                hi,
                of(*k),
            ));
        }

        for w in Work::ALL {
            let k = c.by_work.get(&w).copied().unwrap_or(0);
            out.push_str(&format!(
                "slice census: boundary work: {:<40} {k:>12} ({:>6.2} %)\n",
                w.label(),
                of(k),
            ));
        }
        out.push_str(&format!(
            "slice census: boundary work: {:<40} {:>12} ({:>6.2} %)\n",
            "NOTHING AT ALL",
            c.idle_boundaries,
            of(c.idle_boundaries),
        ));
        out.push_str(&format!(
            "slice census: {} event(s) dispatched and {} pin edge(s) drained in total\n",
            c.due_events, c.pin_edges,
        ));
        out.push_str(&format!(
            "slice census: scheduler heap: {:.2} entrie(s) scanned per slice, {:.2} of them \
             live, {} at the worst slice\n",
            c.heap_entries as f64 / n as f64,
            c.heap_live as f64 / n as f64,
            c.heap_entries_max,
        ));
        Some(out)
    })
}
