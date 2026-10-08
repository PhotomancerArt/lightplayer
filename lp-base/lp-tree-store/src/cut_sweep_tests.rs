//! The power-cut sweeps: every sampled cut point × every tear model of each
//! step, the whole state old or new after each (`test_support::sweep`).
//! `LP_TREE_STORE_SWEEP_CUTS=1000000 LP_TREE_STORE_SWEEP_STEPS=12 cargo test
//! --release -p lp-tree-store cut_sweep` runs them at every cut point.

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;

use lp_nor_sim::NorGeometry;

extern crate std;

use crate::StoreConfig;
use crate::test_support::{Step, deflate, dial, noise, sweep, text};

fn cfg(record_max: u32) -> StoreConfig {
    StoreConfig {
        record_max,
        ..StoreConfig::default()
    }
}

/// Per-call commits: each call is its own atomic step.
fn per_call_workload() -> Vec<Step> {
    vec![
        Box::new(|st| st.put("/hardware.json", &text(10, 220))),
        Box::new(|st| st.put("/projects/a/modules/m/shader.glsl", &text(12, 3500))),
        Box::new(|st| st.put("/projects/a/.lp/panel.json", &text(13, 120))),
        Box::new(|st| st.put("/projects/a/.lp/panel.json", &text(14, 130))),
        Box::new(|st| st.append("/projects/a/modules/m/shader.glsl", &text(15, 900))),
        Box::new(|st| st.delete_prefix("/projects/a/modules/")),
        Box::new(|st| st.delete("/hardware.json").map(|_| ())),
    ]
}

#[test]
fn cut_sweep_per_call_commits() {
    for rm in [256, 1024] {
        let r = sweep(NorGeometry::c6(16), &cfg(rm), &per_call_workload(), 24);
        std::println!("{}: {r:?}", line!());
        assert!(
            r.cuts > 100 && r.landed_old > 0 && r.landed_new > 0,
            "{rm}: {r:?}"
        );
    }
}

/// Transactions: a push of ten files is old or new as a whole.
fn txn_workload() -> Vec<Step> {
    vec![
        Box::new(|st| {
            st.begin()?;
            for i in 0..10u64 {
                st.put(
                    &alloc::format!("/projects/a/m{i}/shader.glsl"),
                    &text(i, 600),
                )?;
            }
            st.put("/projects/a/.lp/panel.json", &text(40, 100))?;
            st.commit()
        }),
        Box::new(|st| {
            // The push of a new version over the old one, in one slot.
            st.begin()?;
            st.delete_prefix("/projects/a/")?;
            for i in 0..10u64 {
                st.put(
                    &alloc::format!("/projects/a/m{i}/shader.glsl"),
                    &text(i + 100, 640),
                )?;
            }
            st.commit()
        }),
    ]
}

#[test]
fn cut_sweep_transaction_of_ten_puts() {
    let c = StoreConfig {
        txn_delta_max: 400,
        ..cfg(512)
    };
    let r = sweep(NorGeometry::c6(24), &c, &txn_workload(), 40);
    std::println!("{}: {r:?}", line!());
    assert!(r.landed_old > 0 && r.landed_new > 0, "{r:?}");
}

/// A 40 KB file appended in 4 KiB calls, as the push's `WriteChunk`s land.
fn append_workload() -> Vec<Step> {
    let whole = noise(77, 40_000);
    (0..10)
        .map(|i| {
            let piece = whole[i * 4096..((i + 1) * 4096).min(whole.len())].to_vec();
            Box::new(move |st: &mut crate::test_support::Store| st.append("/big.bin", &piece))
                as Step
        })
        .collect()
}

#[test]
fn cut_sweep_append_in_4k_chunks() {
    let r = sweep(NorGeometry::c6(32), &cfg(1024), &append_workload(), 12);
    std::println!("{}: {r:?}", line!());
    assert!(r.landed_old > 0 && r.landed_new > 0, "{r:?}");
}

/// Host-deflated chunks inside a transaction.
fn deflated_workload() -> Vec<Step> {
    vec![Box::new(|st| {
        st.begin()?;
        let a = text(5, 4096);
        let b = text(6, 2500);
        st.put_chunk_deflated("/p/s.glsl", 0, 4096, None, &deflate(&a))?;
        st.put_chunk_deflated("/p/s.glsl", 4096, 2500, None, &deflate(&b))?;
        st.put("/p/.lp/panel.json", b"{}")?;
        st.commit()
    })]
}

#[test]
fn cut_sweep_deflated_push() {
    let r = sweep(NorGeometry::c6(16), &cfg(1024), &deflated_workload(), 48);
    std::println!("{}: {r:?}", line!());
    assert!(r.landed_old > 0 && r.landed_new > 0, "{r:?}");
}

/// GC inside the cut range: churn on a small flash.
fn gc_workload() -> Vec<Step> {
    let mut steps: Vec<Step> = Vec::new();
    steps.push(Box::new(|st| {
        st.begin()?;
        st.put("/keep.json", &text(20, 2500))?;
        st.put("/projects/a/.lp/panel.json", &text(21, 100))?;
        st.commit()
    }));
    for i in 0..dial("LP_TREE_STORE_SWEEP_STEPS", 6) {
        steps.push(Box::new(move |st| {
            st.begin()?;
            st.put(&alloc::format!("/log/{i}.json"), &text(60 + i, 300))?;
            st.put("/churn-a.bin", &noise(30 + i, 3000))?;
            st.put("/churn-b.json", &text(40 + i, 2600))?;
            st.put("/projects/a/.lp/panel.json", &text(50 + i, 110))?;
            st.commit()
        }));
    }
    steps
}

#[test]
fn cut_sweep_through_gc() {
    for rm in [256, 1024] {
        let r = sweep(NorGeometry::c6(10), &cfg(rm), &gc_workload(), 8);
        std::println!("{}: {r:?}", line!());
        assert!(r.gc_runs > 0, "{rm}: no GC ran: {r:?}");
        assert!(r.landed_old > 0 && r.landed_new > 0, "{rm}: {r:?}");
    }
}
