//! The store over [`WindowedFlash`] and the simulated cache (stale lines
//! served until invalidated): it reads exactly what it reads without one —
//! through GC, remounts on a cold cache, and power cuts at every sampled op
//! of a GC-heavy workload — and its own invalidates keep it from ever
//! seeing a stale line (no verify failure, no sector retired). The mutant
//! that drops the invalidates (`reads_without_invalidate`) is the bench's to
//! catch (`lp-store-bench mutants`, whose drivers run over this cache).

extern crate std;

use alloc::format;
use alloc::vec::Vec;

use lp_nor_sim::{FaultPlan, NorFlashSim, NorGeometry};

use crate::read_window::{SimLineCache, WindowedFlash};
use crate::test_support::{State, formatted, noise, sweep_tears, text};
use crate::{SoftSha256, StoreConfig, TreeStore};

type Cached = TreeStore<WindowedFlash<NorFlashSim, SimLineCache>, SoftSha256>;

/// What a power-on mount sees: the flash, and an empty cache.
fn power_on(f: NorFlashSim, c: &StoreConfig) -> Cached {
    match TreeStore::mount(
        WindowedFlash::new(f, SimLineCache::default()),
        SoftSha256,
        c.clone(),
    ) {
        Ok(s) => s,
        Err((e, _, _)) => panic!("mount: {e:?}"),
    }
}

fn snap(st: &mut Cached) -> State {
    let mut out = State::new();
    for p in st.list("/").unwrap() {
        let b = st.get(&p).unwrap().unwrap();
        out.insert(p, b);
    }
    out
}

fn step(st: &mut Cached, i: u64) -> Result<(), crate::StoreError<lp_nor_sim::NorError>> {
    match i % 5 {
        0 => st.put("/churn.bin", &noise(i, 5000)),
        1 => st.put(&format!("/keep/k{:02}.json", i % 12), &text(i, 700)),
        2 => st.put("/p/.lp/panel.json", &text(i, 300)),
        3 => st.append("/log.txt", &text(i, 200)),
        _ if i % 3 == 0 => st.delete_prefix("/keep/"),
        _ => st
            .delete(&format!("/keep/k{:02}.json", (i + 5) % 12))
            .map(|_| ()),
    }
}

#[test]
fn reads_through_the_cache_match_the_flash_through_gc_and_remounts() {
    let c = StoreConfig::default();
    let mut st = power_on(formatted(NorGeometry::c6(12), &c), &c);
    for i in 0..400u64 {
        step(&mut st, i).unwrap();
        if i % 37 == 36 {
            let want = snap(&mut st);
            st = power_on(st.into_flash().into_parts().0, &c);
            assert_eq!(snap(&mut st), want, "after step {i}");
        }
    }
    let s = st.stats();
    assert!(s.gc_runs > 10, "{s:?}");
    assert_eq!(s.verify_failures, 0, "a stale line was read: {s:?}");
    assert_eq!(s.retired_sectors, 0, "{s:?}");
    assert!(st.flash().window().fills > 0);
}

/// Every sampled cut of a GC-heavy run, each tear model: power-cycle, mount
/// on a cold cache, and the state is the old or the new one; re-run, and
/// the new one, with nothing retired.
#[test]
fn cuts_through_the_cache_read_one_state() {
    let c = StoreConfig::default();
    let mut flash = formatted(NorGeometry::c6(10), &c);
    let mut old = snap(&mut power_on(flash.clone(), &c));
    let mut cuts = 0;
    for i in 0..24u64 {
        let pre = flash.clone();
        let mut st = power_on(pre.clone(), &c);
        st.flash_mut().flash_mut().set_plan(FaultPlan::none());
        step(&mut st, i).unwrap();
        let n = st.flash().flash().ops_since_plan();
        let new = snap(&mut st);
        flash = st.into_flash().into_parts().0;
        let points: Vec<u64> = (0..=n).step_by((n as usize / 6).max(1)).collect();
        for k in points {
            for tear in sweep_tears() {
                let mut st = power_on(pre.clone(), &c);
                st.flash_mut()
                    .flash_mut()
                    .set_plan(FaultPlan::cut(k, tear, i << 32 | k));
                let _ = step(&mut st, i);
                let mut f = st.into_flash().into_parts().0;
                f.power_cycle(FaultPlan::none());
                let mut st = power_on(f, &c);
                let got = snap(&mut st);
                let ctx = format!("step {i} cut {k}/{n} {tear:?}");
                assert!(got == old || got == new, "{ctx}: neither old nor new");
                if got == old {
                    step(&mut st, i).unwrap_or_else(|e| panic!("{ctx}: re-run: {e:?}"));
                }
                assert!(snap(&mut st) == new, "{ctx}: re-run state");
                assert_eq!(st.stats().retired_sectors, 0, "{ctx}");
                cuts += 1;
            }
        }
        old = new;
    }
    assert!(cuts > 500, "{cuts}");
}
