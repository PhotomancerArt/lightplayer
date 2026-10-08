//! Test-only helpers: synthetic content, whole-state snapshots, and the
//! exhaustive power-cut sweep over a short workload.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lp_nor_sim::{FaultPlan, NorError, NorFlashSim, NorGeometry, TearModel};

use crate::{StoreConfig, StoreError, TreeStore};

pub type Store = TreeStore<NorFlashSim>;
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
    match TreeStore::mount(flash, cfg.clone()) {
        Ok(s) => s,
        Err((e, _)) => panic!("mount failed: {e:?}"),
    }
}

/// A formatted flash.
pub fn formatted(geom: NorGeometry, cfg: &StoreConfig) -> NorFlashSim {
    let mut f = NorFlashSim::new(geom);
    TreeStore::format(&mut f, cfg).expect("format");
    f
}

/// What a sweep did.
#[derive(Debug, Default)]
pub struct SweepReport {
    pub cuts: u64,
    pub ops_total: u64,
    pub landed_old: u64,
    pub landed_new: u64,
    pub gc_runs: u64,
}

/// For each step: run it fault-free to count its ops `n`, then for sampled
/// `k in 0..=n` (`LP_TREE_STORE_SWEEP_CUTS` overrides `max_cuts`) and every
/// tear model, fork the pre-step flash, cut at `k`,
/// power-cycle, mount, and require the whole state to be the old or the new
/// one; then re-run the step (every third cut with a second cut first) and
/// require the new state, also after a remount. `max_cuts` bounds the `k`
/// sample per step (evenly spread, both ends included).
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
            for tear in TearModel::ALL {
                let seed = (si as u64) << 32 | k << 2 | tear as u64;
                let mut st = mount(pre.clone(), cfg);
                st.flash_mut().set_plan(FaultPlan::cut(k, tear, seed));
                let cut_result = step(&mut st);
                let mut f = st.into_flash();
                let ctx = || format!("step {si} cut {k}/{n} {tear:?}");
                if k < n {
                    assert!(cut_result.is_err(), "{}: step survived its cut", ctx());
                }
                f.power_cycle(FaultPlan::none());
                let mut st = mount(f, cfg);
                let got = snapshot(&mut st);
                if got == new {
                    report.landed_new += 1;
                } else {
                    assert!(got == old, "{}: state is neither old nor new", ctx());
                    report.landed_old += 1;
                }
                if k % 3 == 1 {
                    // A double cut: tear the recovery run too.
                    let k2 = (k * 7 + 3) % (n + 1);
                    st.flash_mut()
                        .set_plan(FaultPlan::cut(k2, tear, seed ^ 0xD0B1E));
                    let _ = step(&mut st);
                    let mut f = st.into_flash();
                    f.power_cycle(FaultPlan::none());
                    st = mount(f, cfg);
                    let got = snapshot(&mut st);
                    assert!(got == old || got == new, "{}: double cut at {k2}", ctx());
                }
                step(&mut st).unwrap_or_else(|e| panic!("{}: re-run failed: {e:?}", ctx()));
                assert!(snapshot(&mut st) == new, "{}: re-run state", ctx());
                let mut st = mount(st.into_flash(), cfg);
                assert!(snapshot(&mut st) == new, "{}: remount after re-run", ctx());
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
    extern crate std;
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn sample(n: u64, max: u64) -> Vec<u64> {
    if n < max {
        return (0..=n).collect();
    }
    let mut v: Vec<u64> = (0..max).map(|i| i * n / (max - 1)).collect();
    v.dedup();
    v
}
