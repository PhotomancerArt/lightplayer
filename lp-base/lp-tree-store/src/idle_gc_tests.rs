//! Idle steps (`store_idle.rs`): they change no file, settle, never run in
//! a transaction, forget garbage-only sectors (so a mount reads less),
//! keep the low water, survive a cut anywhere (and every remount reads one
//! state), and — the point — keep a near-full store's writes from stalling
//! in GC.

extern crate std;

use alloc::boxed::Box;
use alloc::vec::Vec;
use alloc::{format, vec};

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

/// A step is one piece, about one erase's time: a mark (reads only), one
/// erase, one forget (a header kill), or copies of at most
/// `idle_copy_bytes` (the record that crosses it finishes) — never a mark
/// and work together, never two erases. Near full and churned alike, until
/// settled.
#[test]
fn each_step_is_one_piece() {
    let c = StoreConfig::default();
    let mut stores = [near_full(&c, 93), churned(&c)];
    let mut kinds = [0u32; 4];
    for st in &mut stores {
        for _ in 0..4000 {
            let before = st.flash().stats().clone();
            let marks = st.stats().marks;
            if !st.collect_step().unwrap() {
                break;
            }
            let after = st.flash().stats();
            let erases = after.erases_total() - before.erases_total();
            let programmed = after.program_bytes - before.program_bytes;
            let marked = st.stats().marks - marks;
            assert!(marked + erases <= 1, "a mark and an erase in one step");
            let kind = if marked == 1 {
                assert_eq!(programmed, 0, "a mark step programmed");
                0
            } else if erases == 1 {
                // The kill (8 B), and an opened head's header, at most.
                assert!(programmed <= 64, "an erase step programmed {programmed} B");
                1
            } else if programmed <= 8 {
                2
            } else {
                // Copies, each with its header, and a sector opened for them.
                let most = u64::from(c.idle_copy_bytes + c.record_max) + 64;
                assert!(programmed <= most, "a copy step programmed {programmed} B");
                3
            };
            kinds[kind] += 1;
        }
    }
    std::println!(
        "steps: {} marks, {} erases, {} forgets, {} copies",
        kinds[0],
        kinds[1],
        kinds[2],
        kinds[3]
    );
    assert!(kinds.iter().all(|&k| k > 0), "{kinds:?}");
}

/// A write between steps: the victim under way is dropped (its copies so
/// far stand; the rest of it stays live where it is) and every file reads
/// back, now and after a mount; the next steps settle from a new mark.
#[test]
fn a_write_between_steps_drops_the_victim_under_way() {
    let c = StoreConfig {
        idle_copy_bytes: 512,
        ..StoreConfig::default()
    };
    let mut st = near_full(&c, 93);
    let mut steps = 0;
    while st.idle_victim.is_none() {
        assert!(st.collect_step().unwrap(), "settled before a victim");
        steps += 1;
    }
    st.collect_step().unwrap();
    assert!(st.idle_victim.is_some(), "a victim in {} steps", steps + 1);
    st.put("/projects/demo/.lp/panel.json", &text(77, 450))
        .unwrap();
    assert!(st.idle_victim.is_none() && st.idle_lens.is_none());
    let want = snapshot(&mut st);
    idle_until_settled(&mut st);
    assert_eq!(snapshot(&mut st), want);
    assert_eq!(snapshot(&mut mount(st.into_flash(), &c)), want);
}

/// Cuts through steps that a write interrupts — part of a victim copied, a
/// write, more steps; an emptied victim, a write, more steps — under the
/// sweep's tear models: the state is the one before the write or after it.
#[test]
fn cut_sweep_through_steps_a_write_interrupts() {
    for rm in [256, 1024] {
        let c = StoreConfig {
            record_max: rm,
            idle_copy_bytes: 512,
            ..cfg(100)
        };
        let r = sweep(NorGeometry::c6(12), &c, &interrupted_workload(), 12);
        std::println!("{}: {r:?}", line!());
        assert!(r.landed_old > 0 && r.landed_new > 0, "{rm}: {r:?}");
    }
}

/// The soak's near-full fill, on the C6's 176 sectors: c40-sized content,
/// then incompressible 4 KiB files until the store says no space — once
/// with no idle time, once with CX1's 20 s of a playing show (44 ms frames)
/// between writes, the steps paced by the server's `lpfs::IdlePacer`.
/// Each write's and step's flash work is priced by a silicon model (an
/// erase 19 ms, a program 0.5 ms per 256 B, a read 0.42 µs per byte: CX1's
/// c40 mount), a host estimate — silicon numbers come from CX1, never from
/// this.
#[test]
fn idle_steps_keep_near_full_writes_from_stalling() {
    let c = StoreConfig::default();
    let without = fill(&c, false);
    let with = fill(&c, true);
    for (label, f) in [("no idle", &without), ("idle steps", &with)] {
        let over = f.costs.iter().filter(|&&ms| ms > 1000.0).count();
        let worst = f.costs.iter().cloned().fold(0.0, f64::max);
        let but_last = f.costs[..f.costs.len() - 1]
            .iter()
            .cloned()
            .fold(0.0, f64::max);
        let last20: Vec<_> = f.costs.iter().rev().take(20).collect();
        let over20 = last20.iter().filter(|&&&ms| ms > 1000.0).count();
        std::println!(
            "{label}: {} files, {over} writes over 1 s (last 20: {over20}), worst {worst:.0} ms, \
             worst but the last {but_last:.0} ms (model); {} erases in all; idle steps {} \
             ({:.0} ms modelled, worst {:.0} ms); then a mount reads {} B",
            f.costs.len(),
            f.erases,
            f.idle_steps,
            f.idle_ms,
            f.idle_worst_ms,
            f.mount_read
        );
    }
    std::println!(
        "the show beside the steps: {} frames, p50 {:.0} p99 {:.0} worst {:.0} ms, {} over \
         100 ms; the steps took {:.1} % of its time",
        with.frames.len(),
        with.frame_percentile(50.0),
        with.frame_percentile(99.0),
        with.frame_percentile(100.0),
        with.frames_over(100.0),
        with.fps_cost_percent()
    );
    // The frame-rate budget's idle row, as a radio feature's: p99 frame
    // ≤ 100 ms and ≤ 10 % of the show's fps.
    assert!(with.frame_percentile(99.0) <= 100.0);
    assert!(with.fps_cost_percent() <= 10.0);
    // A step fits beside a 44 ms frame under the ceiling.
    assert!(with.idle_worst_ms <= 50.0, "{}", with.idle_worst_ms);
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
        let quiet = |st: &mut Store| {
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

/// Churn, then steps a write cuts short: `k` steps (a mark and some
/// copies, or an emptied victim), a write, and steps until settled.
fn interrupted_workload() -> Vec<Step> {
    // Small files packed into shared sectors, then every other one
    // rewritten: sectors with live records and garbage side by side.
    let small = |st: &mut Store, rev: u64, parity: u64| {
        st.begin()?;
        for j in (0..24u64).filter(|j| j % 2 == parity || rev == 0) {
            st.put(&format!("/s/{j:02}.json"), &noise(rev * 100 + j, 300))?;
        }
        st.commit()
    };
    let mut steps: Vec<Step> = vec![Box::new(move |st| small(st, 0, 0))];
    for k in [2u32, 3, 5, 8] {
        steps.push(Box::new(move |st| {
            small(st, u64::from(k), u64::from(k % 2))
        }));
        steps.push(Box::new(move |st| {
            for _ in 0..k {
                st.collect_step()?;
            }
            st.put("/projects/a/.lp/panel.json", &text(u64::from(k) + 90, 110))?;
            idle(st).map(drop)
        }));
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
    /// Every frame of the show in the quiet between writes (ms): the
    /// render's own [`SHOW_FRAME_MS`], plus the step it hosted.
    frames: Vec<f64>,
}

impl Fill {
    /// Frames over `ms`.
    fn frames_over(&self, ms: f64) -> usize {
        self.frames.iter().filter(|&&f| f > ms).count()
    }

    /// The `p`th percentile frame (ms).
    fn frame_percentile(&self, p: f64) -> f64 {
        let mut f = self.frames.clone();
        f.sort_by(|a, b| a.partial_cmp(b).unwrap());
        f.get(((f.len() as f64 - 1.0) * p / 100.0).round() as usize)
            .copied()
            .unwrap_or(0.0)
    }

    /// The share of the show's time idle steps took (%): its fps cost.
    fn fps_cost_percent(&self) -> f64 {
        self.idle_ms * 100.0 / self.frames.iter().sum::<f64>().max(1.0)
    }
}

/// CX1's c40 show: a frame's render alone, quiet and settled (ms).
const SHOW_FRAME_MS: f64 = 44.0;

/// The quiet between two fill writes (ms): CX1's fill driver's 20 s.
const FILL_QUIET_MS: f64 = 20_000.0;

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
        frames: Vec::new(),
    };
    // The server's pacing, over the show's frames (`lpfs::IdlePacer`).
    let mut pacer = lpfs::IdlePacer::new();
    let mut delta = SHOW_FRAME_MS as u32;
    for i in 0..400u32 {
        let steps_before = out.idle_steps;
        if idle_between {
            // The write's tick ended the quiet; then 20 s of a playing show,
            // a step after a frame whenever the pacer finds the time.
            pacer.tick(lpfs::IdleTick {
                delta_ms: delta,
                busy: true,
                playing: true,
            });
            delta = SHOW_FRAME_MS as u32;
            let mut t = 0.0;
            while t < FILL_QUIET_MS {
                let mut frame = SHOW_FRAME_MS;
                if pacer.tick(lpfs::IdleTick {
                    delta_ms: delta,
                    busy: false,
                    playing: true,
                }) {
                    let before = st.flash().stats().clone();
                    let did = st.collect_step().unwrap();
                    pacer.ran(did);
                    let ms = cost_ms(&before, st.flash().stats());
                    if did {
                        out.idle_steps += 1;
                        out.idle_ms += ms;
                        out.idle_worst_ms = out.idle_worst_ms.max(ms);
                        frame += ms;
                    }
                }
                out.frames.push(frame);
                t += frame;
                delta = frame.ceil() as u32;
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
