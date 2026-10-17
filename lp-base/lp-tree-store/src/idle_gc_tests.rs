//! Idle steps (`store_idle.rs`): they change no file, settle, never run in
//! a transaction, forget garbage-only sectors (so a mount reads less),
//! keep the low water, survive a cut anywhere (and every remount reads one
//! state), and — the point — keep a near-full store's writes from stalling
//! in GC.

extern crate std;

use alloc::boxed::Box;
use alloc::format;
use alloc::vec::Vec;

use lp_nor_sim::{NorError, NorFlashSim, NorGeometry, NorStats, TearModel};

use crate::sector_header::{SECTOR_HEADER_LEN, SectorHeader, SectorRead};
use crate::test_support::{
    Step, Store, formatted, mount, noise, remount_sweep, snapshot, sweep, text,
};
use crate::{StoreConfig, StoreError};

#[test]
fn a_step_changes_no_file() {
    let c = cfg(100);
    let mut st = churned(&c);
    let before = snapshot(&mut st);
    let steps = idle(&mut st).unwrap();
    assert!(steps > 0, "nothing to clean");
    assert_eq!(snapshot(&mut st), before);
    let mut st = mount(st.into_flash(), &c);
    assert_eq!(snapshot(&mut st), before);
}

/// A step that finds nothing settles the store: later steps return at once
/// (not a byte read) until a write wakes them.
#[test]
fn steps_settle_until_the_next_write() {
    let c = cfg(100);
    let mut st = churned(&c);
    idle(&mut st).unwrap();
    st.flash_mut().reset_stats();
    assert!(!st.collect_step().unwrap());
    assert_eq!(
        st.flash().stats().read_calls,
        0,
        "a settled step read flash"
    );
    st.put("/again.json", &text(3, 2000)).unwrap();
    st.put("/again.json", &text(4, 2000)).unwrap();
    st.flash_mut().reset_stats();
    let _ = st.collect_step().unwrap();
    assert!(st.flash().stats().read_calls > 0, "a write did not wake it");
}

#[test]
fn no_step_runs_inside_a_transaction() {
    let c = cfg(100);
    let mut st = churned(&c);
    st.begin().unwrap();
    st.put("/in-txn.json", &text(9, 3000)).unwrap();
    st.flash_mut().reset_stats();
    assert!(!st.collect_step().unwrap());
    assert_eq!(st.flash().stats().read_calls, 0);
    st.commit().unwrap();
    assert!(idle(&mut st).unwrap() > 0);
    assert_eq!(st.get("/in-txn.json").unwrap().unwrap(), text(9, 3000));
}

/// Garbage-only sectors stop being trusted: the next mount reads 24 B of
/// each instead of its records (and finds the same tree).
#[test]
fn forgotten_sectors_make_the_mount_read_less() {
    let c = cfg(0);
    let mut st = churned(&c);
    let want = snapshot(&mut st);
    let flash = st.into_flash();
    let (before, trusted_before) = mount_reads(flash.clone(), &c);
    let mut st = mount(flash, &c);
    idle(&mut st).unwrap();
    assert!(st.stats().idle_forgets > 0, "nothing forgotten");
    assert_eq!(st.stats().idle_collections, 0, "the low water was 0 %");
    let flash = st.into_flash();
    let (after, trusted_after) = mount_reads(flash.clone(), &c);
    std::println!(
        "mount: {before} B over {trusted_before} trusted sectors; after idle steps {after} B \
         over {trusted_after}"
    );
    assert!(trusted_after < trusted_before && after < before);
    assert_eq!(snapshot(&mut mount(flash, &c)), want);
}

/// Below the low water an idle step collects; the store ends with at least
/// the target free, or with no sector holding `idle_min_garbage` of garbage.
#[test]
fn steps_keep_the_low_water() {
    let c = cfg(50);
    // Small files across every sector, then every other one rewritten: the
    // old sectors hold live records and garbage side by side.
    let mut st = mount(formatted(NorGeometry::c6(SECTORS), &c), &c);
    for i in 0..40u64 {
        st.put(&format!("/f/{i:02}.json"), &noise(i, 900)).unwrap();
    }
    for i in (0..40u64).step_by(2) {
        st.put(&format!("/f/{i:02}.json"), &noise(i + 100, 900))
            .unwrap();
    }
    let want = snapshot(&mut st);
    let free_before = st.free_sectors();
    idle(&mut st).unwrap();
    assert_eq!(snapshot(&mut st), want);
    let target = SECTORS.div_ceil(2);
    std::println!(
        "free {free_before} -> {} (target {target}), {} collections",
        st.free_sectors(),
        st.stats().idle_collections
    );
    assert!(st.stats().idle_collections > 0);
    assert!(st.free_sectors() >= target.min(free_before + 1));
}

/// A cut anywhere in an idle step (a mark, a forget, a victim's copies and
/// erase), under the sweep's tear models: the state is the one before, and
/// a re-run settles. Interleaved with the churn that made the garbage.
#[test]
fn cut_sweep_through_idle_steps() {
    for rm in [256, 1024] {
        let c = StoreConfig {
            record_max: rm,
            ..cfg(100)
        };
        let r = sweep(NorGeometry::c6(12), &c, &idle_workload(), 12);
        std::println!("{}: {r:?}", line!());
        assert!(r.landed_old > 0, "{rm}: {r:?}");
    }
}

/// Every remount after a cut in an idle step reads one state (a torn kill
/// of a forgotten header leaves weak bits; the sector was garbage).
#[test]
fn idle_cuts_remount_to_one_state() {
    let mid = [TearModel::CalibratedMidWord, TearModel::CalibratedFirstLift];
    let r = remount_sweep(
        NorGeometry::c6(12),
        &cfg(100),
        &idle_workload(),
        12,
        &mid,
        8,
        3,
    );
    std::println!("{}: {r:?}", line!());
    assert!(r.cuts > 100, "{r:?}");
}

/// The soak's near-full fill, on the C6's 176 sectors: c40-sized content,
/// then incompressible 4 KiB files until the store says no space — once
/// with no idle time, once with up to 40 idle steps between writes.
/// Each write's flash work is priced by a silicon model (an erase 19 ms, a
/// program 0.5 ms per 256 B, a read 0.42 µs per byte: CX1's c40 mount),
/// a host estimate — silicon numbers come from CX1, never from this.
#[test]
fn idle_steps_keep_near_full_writes_from_stalling() {
    let c = StoreConfig::default();
    let without = fill(&c, false);
    let with = fill(&c, true);
    for (label, f) in [("no idle", &without), ("idle steps", &with)] {
        let over = f.costs.iter().filter(|&&ms| ms > 1000.0).count();
        let worst = f.costs.iter().cloned().fold(0.0, f64::max);
        let last20: Vec<_> = f.costs.iter().rev().take(20).collect();
        let over20 = last20.iter().filter(|&&&ms| ms > 1000.0).count();
        std::println!(
            "{label}: {} files, {over} writes over 1 s (last 20: {over20}), worst {worst:.0} ms \
             (model); {} erases in all; idle steps {} ({:.0} ms modelled, worst {:.0} ms); then \
             a mount reads {} B",
            f.costs.len(),
            f.erases,
            f.idle_steps,
            f.idle_ms,
            f.idle_worst_ms,
            f.mount_read
        );
    }
    let over = |costs: &[f64]| costs.iter().filter(|&&ms| ms > 1000.0).count();
    assert!(
        over(&without.costs) > 1,
        "the fill no longer stalls without idle steps"
    );
    // Not even the write that takes the store's last room. (While a write
    // restarted the packing pass, and every pass re-packed the whole store
    // for a write's few hundred bytes of garbage, the last write still
    // re-packed in front of itself: 2.5 s here, 4.5 s on CX1.)
    assert_eq!(over(&with.costs), 0, "an idle store still stalled a write");
    // No more erases than the write path's GC would have made.
    assert!(
        with.erases <= without.erases + without.erases / 20,
        "{} erases with idle steps, {} without",
        with.erases,
        without.erases
    );
    assert!(with.mount_read < without.mount_read);
    // An idle step is bounded: one mark and at most one victim.
    assert!(with.idle_worst_ms < 300.0, "{}", with.idle_worst_ms);
    // Idle collection does not cost the store files.
    assert!(with.costs.len() + 2 >= without.costs.len());
}

/// Near full, a store given all the quiet it wants between small writes
/// (the board's panel saves) wears no more than the write path's own GC
/// would. Before idle packing waited for a sector's worth of new waste, at
/// 93 files every 450 B save bought a whole re-pack: 50,913 erases over
/// these 300 writes against the write path's 93. A mount forgets what the
/// last packing left, so the first quiet after one packs once (`settle`).
#[test]
fn idle_packing_near_full_wears_no_more_than_the_write_path() {
    let c = StoreConfig::default();
    for files in [86u32, 91, 93] {
        let mut erases = [0u64; 2];
        let mut worst = [0f64; 2];
        let mut settle = 0;
        for (k, idle_between) in [false, true].into_iter().enumerate() {
            let mut st = near_full(&c, files);
            let e0 = st.flash().stats().erases_total();
            if idle_between {
                idle_until_settled(&mut st);
                settle = st.flash().stats().erases_total() - e0;
            }
            let e0 = st.flash().stats().erases_total();
            for i in 0..300u64 {
                if idle_between {
                    idle_until_settled(&mut st);
                }
                let before = st.flash().stats().clone();
                st.put("/projects/demo/.lp/panel.json", &text(9000 + i, 450))
                    .unwrap();
                worst[k] = worst[k].max(cost_ms(&before, st.flash().stats()));
            }
            erases[k] = st.flash().stats().erases_total() - e0;
        }
        std::println!(
            "{files} files: 300 panel writes: {} erases with no idle time, {} with idle steps \
             until settled before each (after {settle} settling once); worst write {:.0} / \
             {:.0} ms (model)",
            erases[0],
            erases[1],
            worst[0],
            worst[1]
        );
        assert!(
            erases[1] <= erases[0] + erases[0] / 4,
            "{files} files: idle packing wore the store: {} erases against {}",
            erases[1],
            erases[0]
        );
        assert!(
            settle <= 2 * 176,
            "{files} files: settling took {settle} erases"
        );
    }
}

/// A day of churn on the C6's 176 sectors (about c40's content; then a
/// re-push, 10 saves and 1,440 panel saves — the endurance driver's day),
/// once back to back and once with the quiet between them a board has
/// (idle steps until settled): what the next mount reads, and the erases.
/// The mount is priced by the silicon model's read rate (a host estimate).
#[test]
fn a_day_of_churn_leaves_less_for_the_mount() {
    let c = StoreConfig::default();
    let mut out = [(0u64, 0u32, 0u64); 2];
    for (k, idle_between) in [false, true].into_iter().enumerate() {
        let mut st = mount(formatted(NorGeometry::c6(176), &c), &c);
        demo_project(&mut st, 0);
        let e0 = st.flash().stats().erases_total();
        let mut quiet = |st: &mut Store| {
            if idle_between {
                idle_until_settled(st);
            }
        };
        demo_project(&mut st, 1);
        quiet(&mut st);
        for i in 0..1440u64 {
            if i % 144 == 0 {
                let m = i / 144;
                st.put(
                    &format!("/projects/demo/modules/m{m:02}/node.json"),
                    &text(700 + m, 1800),
                )
                .unwrap();
                quiet(&mut st);
            }
            st.put("/projects/demo/.lp/panel.json", &text(5000 + i, 450))
                .unwrap();
            quiet(&mut st);
        }
        let erases = st.flash().stats().erases_total() - e0;
        let want = snapshot(&mut st);
        let flash = st.into_flash();
        let (read, trusted) = mount_reads(flash.clone(), &c);
        assert_eq!(snapshot(&mut mount(flash, &c)), want);
        out[k] = (read, trusted, erases);
    }
    let ms = |b: u64| b as f64 * 0.000_42;
    std::println!(
        "a day of churn, then a mount: back to back {} B over {} trusted sectors (~{:.0} ms, \
         {} erases); with idle steps {} B over {} (~{:.0} ms, {} erases)",
        out[0].0,
        out[0].1,
        ms(out[0].0),
        out[0].2,
        out[1].0,
        out[1].1,
        ms(out[1].0),
        out[1].2
    );
    assert!(out[1].0 < out[0].0 && out[1].1 < out[0].1);
    assert!(
        out[1].2 <= out[0].2 + out[0].2 / 10,
        "idle steps wore the store"
    );
}

// ---- helpers ---------------------------------------------------------------

const SECTORS: u32 = 16;

fn cfg(idle_free_percent: u32) -> StoreConfig {
    StoreConfig {
        idle_free_percent,
        ..StoreConfig::default()
    }
}

/// Idle steps until one does nothing (at most 4,000).
fn idle_until_settled(st: &mut Store) {
    for _ in 0..4000 {
        if !st.collect_step().unwrap() {
            return;
        }
    }
    panic!("4,000 idle steps did not settle");
}

/// Idle steps until one does nothing (at most 64).
fn idle(st: &mut Store) -> Result<u32, StoreError<NorError>> {
    let mut n = 0;
    while n < 64 && st.collect_step()? {
        n += 1;
    }
    Ok(n)
}

/// A store with garbage: files rewritten, panel writes, a deleted tree.
fn churned(c: &StoreConfig) -> Store {
    let mut st = mount(formatted(NorGeometry::c6(SECTORS), c), c);
    st.put("/keep.json", &text(1, 3000)).unwrap();
    for i in 0..10u64 {
        st.put("/churn.bin", &noise(i, 3500)).unwrap();
        st.put("/projects/a/.lp/panel.json", &text(20 + i, 120))
            .unwrap();
        st.put(&format!("/projects/b/m{i}.json"), &text(40 + i, 900))
            .unwrap();
    }
    st.delete_prefix("/projects/b/").unwrap();
    st
}

/// Churn and idle steps, interleaved, on a 12-sector flash.
fn idle_workload() -> Vec<Step> {
    let mut steps: Vec<Step> = Vec::new();
    steps.push(Box::new(|st| st.put("/keep.json", &text(1, 2500))));
    for round in 0..3u64 {
        steps.push(Box::new(move |st| {
            st.begin()?;
            for i in 0..4u64 {
                st.put("/churn.bin", &noise(round * 10 + i, 3000))?;
                st.put("/projects/a/.lp/panel.json", &text(round * 10 + i, 110))?;
            }
            st.put(&format!("/log/{round}.json"), &text(round + 60, 700))?;
            st.commit()
        }));
        steps.push(Box::new(|st| idle(st).map(drop)));
    }
    steps
}

/// A mount's flash reads, and how many sectors it trusted.
fn mount_reads(mut f: NorFlashSim, c: &StoreConfig) -> (u64, u32) {
    let trusted = (0..f.geometry().sector_count)
        .filter(|&s| {
            let mut h = [0u8; SECTOR_HEADER_LEN as usize];
            f.peek(s * 4096, &mut h);
            matches!(SectorHeader::decode(&h, 4096), SectorRead::Trusted { .. })
        })
        .count() as u32;
    f.reset_stats();
    let st = mount(f, c);
    (st.flash().stats().read_bytes, trusted)
}

struct Fill {
    /// Each fill write's modelled cost (ms).
    costs: Vec<f64>,
    /// Erases from the start of the fill to its end, idle steps included.
    erases: u64,
    /// Bytes the mount after the fill reads.
    mount_read: u64,
    idle_steps: u32,
    idle_ms: f64,
    idle_worst_ms: f64,
}

fn fill(c: &StoreConfig, idle_between: bool) -> Fill {
    let mut st = mount(formatted(NorGeometry::c6(176), c), c);
    // About c40's live bytes: forty-odd modules with shader and panel files,
    // a project file, written as a push would, then a few saves.
    st.begin().unwrap();
    for m in 0..44u64 {
        st.put(
            &format!("/projects/demo/modules/m{m:02}/shader.glsl"),
            &text(m, 2900),
        )
        .unwrap();
        st.put(
            &format!("/projects/demo/modules/m{m:02}/node.json"),
            &text(m + 100, 1800),
        )
        .unwrap();
    }
    st.put("/projects/demo/.lp/panel.json", &text(7, 450))
        .unwrap();
    st.commit().unwrap();
    for k in 0..20u64 {
        st.put("/projects/demo/modules/m00/node.json", &text(200 + k, 1800))
            .unwrap();
        st.put("/projects/demo/.lp/panel.json", &text(300 + k, 450))
            .unwrap();
    }
    let erases0 = st.flash().stats().erases_total();
    let mut out = Fill {
        costs: Vec::new(),
        erases: 0,
        mount_read: 0,
        idle_steps: 0,
        idle_ms: 0.0,
        idle_worst_ms: 0.0,
    };
    for i in 0..400u32 {
        let steps_before = out.idle_steps;
        if idle_between {
            // 20 s of quiet at one step per 500 ms, as CX1's fill gives.
            for _ in 0..40 {
                let before = st.flash().stats().clone();
                let did = st.collect_step().unwrap();
                let ms = cost_ms(&before, st.flash().stats());
                out.idle_ms += ms;
                out.idle_worst_ms = out.idle_worst_ms.max(ms);
                if !did {
                    break;
                }
                out.idle_steps += 1;
            }
        }
        // The board's own writes go on as the store fills (panel saves).
        st.put(
            "/projects/demo/.lp/panel.json",
            &text(500 + u64::from(i), 450),
        )
        .unwrap();
        let before = st.flash().stats().clone();
        let s0 = st.stats();
        match st.put(
            &format!("/fill/f{i:03}.bin"),
            &noise(u64::from(i) + 9000, 4096),
        ) {
            Ok(()) => {
                let ms = cost_ms(&before, st.flash().stats());
                if ms > 1000.0 || std::env::var("LP_FILL_TRACE").is_ok() {
                    let s1 = st.stats();
                    let b = st.flash().stats();
                    std::println!(
                        "  fill {i} idle={idle_between}: {ms:.0} ms: erases {} prog {} B read {} B \
                         ({} calls), gc_runs {} marks {} copies {} free {}; {} idle steps before",
                        b.erases_total() - before.erases_total(),
                        b.program_bytes - before.program_bytes,
                        b.read_bytes - before.read_bytes,
                        b.read_calls - before.read_calls,
                        s1.gc_runs - s0.gc_runs,
                        s1.marks - s0.marks,
                        s1.gc_copies - s0.gc_copies,
                        st.free_sectors(),
                        out.idle_steps - steps_before
                    );
                }
                out.costs.push(ms)
            }
            Err(StoreError::NoSpace) => break,
            Err(e) => panic!("fill {i}: {e:?}"),
        }
    }
    out.erases = st.flash().stats().erases_total() - erases0;
    out.mount_read = mount_reads(st.into_flash(), c).0;
    out
}

/// The near-full fill's store (`fill`'s content), filled with 4 KiB files
/// until fewer than `reserve + idle_pack_free` sectors are free.
fn near_full(c: &StoreConfig, files: u32) -> Store {
    let mut st = mount(formatted(NorGeometry::c6(176), c), c);
    st.begin().unwrap();
    for m in 0..44u64 {
        st.put(
            &format!("/projects/demo/modules/m{m:02}/shader.glsl"),
            &text(m, 2900),
        )
        .unwrap();
        st.put(
            &format!("/projects/demo/modules/m{m:02}/node.json"),
            &text(m + 100, 1800),
        )
        .unwrap();
    }
    st.commit().unwrap();
    for i in 0..files {
        st.put(
            &format!("/fill/f{i:03}.bin"),
            &noise(u64::from(i) + 9000, 4096),
        )
        .unwrap();
    }
    st
}

/// About c40's content, written as a push would (`rev` changes every file).
fn demo_project(st: &mut Store, rev: u64) {
    st.begin().unwrap();
    for m in 0..44u64 {
        st.put(
            &format!("/projects/demo/modules/m{m:02}/shader.glsl"),
            &text(m + 1000 * rev, 2900),
        )
        .unwrap();
        st.put(
            &format!("/projects/demo/modules/m{m:02}/node.json"),
            &text(m + 100 + 1000 * rev, 1800),
        )
        .unwrap();
    }
    st.put("/projects/demo/.lp/panel.json", &text(7 + rev, 450))
        .unwrap();
    st.commit().unwrap();
}

/// A silicon estimate of the flash work between two simulator counters.
fn cost_ms(a: &NorStats, b: &NorStats) -> f64 {
    let erases = (b.erases_total() - a.erases_total()) as f64;
    let programmed = (b.program_bytes - a.program_bytes) as f64;
    let read = (b.read_bytes - a.read_bytes) as f64;
    erases * 19.0 + programmed / 256.0 * 0.5 + read * 0.000_42
}
