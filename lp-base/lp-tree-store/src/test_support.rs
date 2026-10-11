//! Test-only helpers: synthetic content, whole-state snapshots, the
//! power-cut sweep, a host deflater, and a per-thread counting allocator
//! (the ground truth the RAM figures are checked against).

extern crate std;

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use lp_nor_sim::{FaultPlan, NorError, NorFlashSim, NorGeometry, TearModel};

use crate::{SoftSha256, StoreConfig, StoreError, TreeStore};

pub type Store = TreeStore<NorFlashSim, SoftSha256>;
pub type Step = Box<dyn Fn(&mut Store) -> Result<(), StoreError<NorError>>>;
pub type State = BTreeMap<String, Vec<u8>>;

/// Deterministic JSON-ish text (compressible, like the real corpus).
pub fn text(seed: u64, len: usize) -> Vec<u8> {
    const WORDS: [&str; 12] = [
        "\"kind\": ",
        "\"shader\"",
        ", ",
        "\"speed\": ",
        "0.25",
        "{\n  ",
        "\n}",
        "\"palette\"",
        "[1, 2, 3]",
        "\"phase\": ",
        "uniform float ",
        "vec3 color = ",
    ];
    let mut r = Lcg(seed);
    let mut out = Vec::with_capacity(len + 16);
    while out.len() < len {
        out.extend_from_slice(WORDS[(r.next() % WORDS.len() as u64) as usize].as_bytes());
        if r.next() % 5 == 0 {
            out.extend_from_slice(format!("{}", r.next() % 1000).as_bytes());
        }
    }
    out.truncate(len);
    out
}

/// Deterministic incompressible bytes.
pub fn noise(seed: u64, len: usize) -> Vec<u8> {
    let mut r = Lcg(seed ^ 0x9E37_79B9_7F4A_7C15);
    (0..len).map(|_| (r.next() >> 24) as u8).collect()
}

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 11
    }
}

/// Raw deflate, as a host would send it.
pub fn deflate(bytes: &[u8]) -> Vec<u8> {
    miniz_oxide::deflate::compress_to_vec(bytes, 10)
}

/// Every file and its bytes.
pub fn snapshot(st: &mut Store) -> State {
    let mut out = BTreeMap::new();
    for p in st.list("/").expect("list") {
        let b = st.get(&p).expect("get").expect("listed file");
        out.insert(p, b);
    }
    out
}

pub fn mount(flash: NorFlashSim, cfg: &StoreConfig) -> Store {
    match TreeStore::mount(flash, SoftSha256, cfg.clone()) {
        Ok(s) => s,
        Err((e, _, _)) => panic!("mount failed: {e:?}"),
    }
}

/// A formatted flash.
pub fn formatted(geom: NorGeometry, cfg: &StoreConfig) -> NorFlashSim {
    match TreeStore::format(NorFlashSim::new(geom), SoftSha256, cfg.clone()) {
        Ok(st) => st.into_flash(),
        Err((e, _, _)) => panic!("format failed: {e:?}"),
    }
}

/// What a sweep did.
#[derive(Debug, Default)]
pub struct SweepReport {
    pub cuts: u64,
    pub ops_total: u64,
    pub landed_old: u64,
    pub landed_new: u64,
    pub gc_runs: u64,
    /// Cuts that tore a sector erase (the rest tore a program page, or
    /// landed after the step's last op).
    pub torn_erases: u64,
    /// Recovery runs cut a second time.
    pub double_cuts: u64,
}

/// For each step: run it fault-free to count its ops `n`, then for sampled
/// `k in 0..=n` (`LP_TREE_STORE_SWEEP_CUTS` overrides `max_cuts`) and every
/// tear model ([`sweep_tears`]: the three guessed ones unless
/// `LP_TREE_STORE_SWEEP_TEARS` names others), fork the pre-step flash, cut at `k`, power-cycle, mount, and
/// require the **whole state** to be the old or the new one; every third
/// cut tears the recovery run too (a double cut); then re-run the step and
/// require the new state, also after a remount, with no sector retired.
pub fn sweep(geom: NorGeometry, cfg: &StoreConfig, steps: &[Step], max_cuts: u64) -> SweepReport {
    let mut report = SweepReport::default();
    let mut flash = formatted(geom, cfg);
    let mut old = snapshot(&mut mount(flash.clone(), cfg));
    for (si, step) in steps.iter().enumerate() {
        let pre = flash.clone();
        let mut st = mount(pre.clone(), cfg);
        st.flash_mut().set_plan(FaultPlan::none());
        step(&mut st).unwrap_or_else(|e| panic!("step {si} fault-free: {e:?}"));
        let n = st.flash().ops_since_plan();
        report.gc_runs += st.stats().gc_runs;
        let new = snapshot(&mut st);
        flash = st.into_flash();
        report.ops_total += n;
        for k in sample(n, dial("LP_TREE_STORE_SWEEP_CUTS", max_cuts)) {
            for tear in sweep_tears() {
                let seed = (si as u64) << 32 | k << 2 | tear as u64;
                let mut st = mount(pre.clone(), cfg);
                st.flash_mut().set_plan(FaultPlan::cut(k, tear, seed));
                let torn_before = st.flash().stats().torn_erases;
                let cut_result = step(&mut st);
                report.torn_erases += st.flash().stats().torn_erases - torn_before;
                let mut f = st.into_flash();
                let ctx = || format!("step {si} cut {k}/{n} {tear:?}");
                if k < n {
                    assert!(cut_result.is_err(), "{}: step survived its cut", ctx());
                }
                f.power_cycle(FaultPlan::none());
                let mut st = mount(f, cfg);
                let mut got = snapshot(&mut st);
                if got == new {
                    report.landed_new += 1;
                } else {
                    assert!(got == old, "{}: state is neither old nor new", ctx());
                    report.landed_old += 1;
                }
                // Steps need not be idempotent (an append is not): re-run
                // only a step that did not land.
                if got == old && k % 3 == 1 {
                    // A double cut: tear the recovery run too.
                    let k2 = (k * 7 + 3) % (n + 1);
                    st.flash_mut()
                        .set_plan(FaultPlan::cut(k2, tear, seed ^ 0xD0B1E));
                    let _ = step(&mut st);
                    report.double_cuts += 1;
                    let mut f = st.into_flash();
                    f.power_cycle(FaultPlan::none());
                    st = mount(f, cfg);
                    got = snapshot(&mut st);
                    assert!(got == old || got == new, "{}: double cut at {k2}", ctx());
                }
                if got == old {
                    step(&mut st).unwrap_or_else(|e| panic!("{}: re-run failed: {e:?}", ctx()));
                }
                assert!(snapshot(&mut st) == new, "{}: re-run state", ctx());
                let mut st = mount(st.into_flash(), cfg);
                assert!(snapshot(&mut st) == new, "{}: remount after re-run", ctx());
                // No sweep wears a sector out: a cut, whatever it tore, must
                // never cost one (a retirement is persisted for good).
                assert_eq!(
                    st.stats().retired_sectors,
                    0,
                    "{}: a cut retired a sector",
                    ctx()
                );
                report.cuts += 1;
            }
        }
        old = new;
    }
    report
}

/// A sweep dial: `default`, or the environment variable `name` when set
/// (e.g. `LP_TREE_STORE_SWEEP_CUTS=100000` for an exhaustive local run).
pub fn dial(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// The tear models a sweep runs: [`TearModel::ALL`] (the three guessed
/// ones), or the comma-separated names in `LP_TREE_STORE_SWEEP_TEARS`
/// (`calibrated`, `calibrated_zeroing`, … — `lp-nor-sim`'s
/// [`TearModel::NAMED`]). An unknown name panics rather than running less.
pub fn sweep_tears() -> Vec<TearModel> {
    match std::env::var("LP_TREE_STORE_SWEEP_TEARS") {
        Ok(v) if !v.trim().is_empty() => v
            .split(',')
            .map(|t| {
                TearModel::from_name(t.trim()).unwrap_or_else(|| {
                    panic!("LP_TREE_STORE_SWEEP_TEARS: unknown tear model {t:?}")
                })
            })
            .collect(),
        _ => TearModel::ALL.to_vec(),
    }
}

/// A sweep's geometry: `sectors` 4 KiB sectors, or
/// `LP_TREE_STORE_SWEEP_SECTORS` when set (the device's 128 or 176). The
/// GC sweep keeps its own small flash: GC inside the cut range is its point.
pub fn sweep_geometry(sectors: u32) -> NorGeometry {
    NorGeometry::c6(dial("LP_TREE_STORE_SWEEP_SECTORS", sectors as u64) as u32)
}

fn sample(n: u64, max: u64) -> Vec<u64> {
    if n < max {
        return (0..=n).collect();
    }
    let mut v: Vec<u64> = (0..max).map(|i| i * n / (max - 1)).collect();
    v.dedup();
    v
}

// ---- the counting allocator ---------------------------------------------

std::thread_local! {
    static COUNTING: Cell<bool> = const { Cell::new(false) };
    static LIVE: Cell<isize> = const { Cell::new(0) };
    static PEAK: Cell<isize> = const { Cell::new(0) };
}

struct CountingAlloc;

fn count(delta: isize) {
    let _ = COUNTING.try_with(|on| {
        if on.get() {
            let _ = LIVE.try_with(|l| {
                let v = l.get() + delta;
                l.set(v);
                let _ = PEAK.try_with(|p| p.set(p.get().max(v)));
            });
        }
    });
}

// SAFETY: every call forwards to `System`; the counters are plain
// thread-local cells that never allocate.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(layout.size() as isize);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        count(-(layout.size() as isize));
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count(new_size as isize - layout.size() as isize);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

/// Heap this thread allocated while `f` ran: (result, peak above the start,
/// net held at the end).
pub fn heap_use<T>(f: impl FnOnce() -> T) -> (T, usize, isize) {
    LIVE.with(|l| l.set(0));
    PEAK.with(|p| p.set(0));
    COUNTING.with(|c| c.set(true));
    let out = f();
    COUNTING.with(|c| c.set(false));
    (
        out,
        PEAK.with(Cell::get).max(0) as usize,
        LIVE.with(Cell::get),
    )
}
