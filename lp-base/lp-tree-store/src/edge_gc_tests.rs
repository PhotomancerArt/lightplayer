//! A nearly full store: a write GC can make room for is not refused, and a
//! re-run after a power cut fits where the step fitted
//! (`docs/defects/2026-10-09-tree-store-rerun-after-a-cut-is-refused-at-the-edge.md`).
//! Small geometries on lp-nor-sim; each test fails on the store before
//! that defect's fix.

extern crate std;

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use lp_nor_sim::{FaultPlan, NorError, NorGeometry, TearModel};

use crate::test_support::{Store, formatted, mount, noise, snapshot, text};
use crate::{StoreConfig, StoreError};

/// Mechanism 1. Big and small files side by side, then every small one
/// deleted at once: garbage spread thin over every sector, the free count
/// at the reserve. No single collection frees a sector, so GC used to stop
/// after `reserve + 2` of them and refuse a write that a second attempt
/// (after the first one's collections) accepted. A refusal is now final:
/// the same write after a remount is refused too.
#[test]
fn spread_garbage_never_refuses_then_accepts() {
    let cfg = StoreConfig::default();
    let mut cells = Vec::new();
    for files in [15, 16, 17, u64::MAX] {
        for small in [150, 250, 350] {
            for want in [2000, 4000] {
                // (A store filled to the brim may refuse even the deletes'
                // directories: no such cell.)
                let Some(mut st) = spread_garbage(&cfg, 2000, small, files) else {
                    continue;
                };
                let first = st.put("/new.bin", &noise(99, want)).is_ok();
                let mut again = mount(st.into_flash(), &cfg);
                let second = again.put("/new.bin", &noise(99, want)).is_ok();
                assert!(
                    first || !second,
                    "files {files} small {small} want {want}: refused, then accepted after a remount"
                );
                cells.push((files, small, want, first));
            }
        }
    }
    // The grid reaches both sides of the edge.
    let accepted = cells.iter().filter(|c| c.3).count();
    assert!(accepted > 0 && accepted < cells.len(), "{cells:?}");
}

/// Mechanism 2. Copies of a project until one is refused, then ever
/// smaller files until even the smallest is: the free count at the
/// reserve, the cold head full. Each panel write that fits is cut at every
/// flash op it makes (clean, and torn mid-program); after the remount, the
/// state is the old one or the new one, and the write run again must fit.
/// A cut inside a record of the hot head closes it (nothing after a record
/// that fails its CRC is trusted), so the re-run needs a hot sector the
/// step did not; GC used to copy the closed head's live records to the full
/// cold head (opening the sector it freed), the bound refused live bytes
/// the layout had held, and a hot head full of old roots was never
/// collected.
#[test]
fn a_rerun_after_a_cut_fits_where_the_step_fitted() {
    let cfg = StoreConfig::default();
    let mut cases = 0;
    let mut reruns = 0;
    for seed in 0..2u64 {
        let (mut flash, copies) = full_store(&cfg, seed);
        for i in 0..3u64 {
            let panel = vec![(
                format!("/projects/f{}/.lp/panel.json", i % copies),
                text(seed * 31 + i, 150 + i as usize * 100),
            )];
            let pre = flash.clone();
            let mut st = mount(pre.clone(), &cfg);
            let old = snapshot(&mut st);
            st.flash_mut().set_plan(FaultPlan::none());
            if push(&mut st, &panel).is_err() {
                continue;
            }
            let ops = st.flash().ops_since_plan();
            let new = snapshot(&mut st);
            flash = st.into_flash();
            for cut in 0..ops {
                for tear in [TearModel::Clean, TearModel::BytePrefix] {
                    let ctx = || format!("seed {seed} panel {i} cut {cut}/{ops} {tear:?}");
                    cases += 1;
                    let mut st = mount(pre.clone(), &cfg);
                    st.flash_mut().set_plan(FaultPlan::cut(cut, tear, cut));
                    let _ = push(&mut st, &panel);
                    let mut f = st.into_flash();
                    f.power_cycle(FaultPlan::none());
                    let mut st = mount(f, &cfg);
                    let got = snapshot(&mut st);
                    if got == new {
                        continue;
                    }
                    assert!(got == old, "{}: neither old nor new", ctx());
                    reruns += 1;
                    push(&mut st, &panel).unwrap_or_else(|e| panic!("{}: re-run: {e:?}", ctx()));
                    assert!(snapshot(&mut st) == new, "{}: re-run state", ctx());
                }
            }
        }
    }
    assert!(cases > 50 && reruns > 50, "cases {cases} re-runs {reruns}");
}

/// The head's own garbage. On the full store, the same panel file written
/// over and over: the live set stays put, but every write leaves an old
/// panel, hot directory and root in the hot head, and once that head is
/// full a write needs a hot sector the reserve will not give. No victim
/// holds garbage then — it is all in the head, which GC never collected —
/// so the store refused a write the packing bound admitted. GC now renews
/// the head (its live records to a new hot head, the old one erased).
#[test]
fn a_hot_head_full_of_old_roots_is_renewed() {
    let cfg = StoreConfig::default();
    for seed in 0..2u64 {
        let (flash, _) = full_store(&cfg, seed);
        let mut st = mount(flash, &cfg);
        for i in 0..60u64 {
            st.put("/projects/f0/.lp/panel.json", &text(seed * 7 + i, 200))
                .unwrap_or_else(|e| panic!("seed {seed} write {i}: {e:?}"));
        }
        let mut st = mount(st.into_flash(), &cfg);
        let got = st.get("/projects/f0/.lp/panel.json").unwrap().unwrap();
        assert_eq!(got, text(seed * 7 + 59, 200));
    }
}

/// Compaction near full. Copies of a project until one is refused, then
/// edits of the kind a user makes there (a re-push, one or two shaders
/// saved). With every sector's garbage collected, GC compacts sectors whose
/// only waste is their tail; copying their records in sector order, a
/// record that did not fit the head's tail opened a head at once and left
/// that tail unused, so compaction cycled over the same sectors and every
/// one of these edits was refused. Filling the head's tail with the
/// records that fit first wins those tails back.
#[test]
fn compaction_near_full_wins_the_tails_back() {
    let cfg = StoreConfig::default();
    let mut fitted = 0;
    for seed in 0..2u64 {
        let mut st = mount(formatted(NorGeometry::c6(16), &cfg), &cfg);
        let mut copies = 0;
        while push(
            &mut st,
            &project(&format!("f{copies}"), seed * 100 + copies),
        )
        .is_ok()
        {
            copies += 1;
        }
        for i in 0..6u64 {
            let slot = format!("f{}", (i * 7 + seed) % copies);
            let edit: Files = match (i + seed) % 3 {
                0 => project(&slot, seed * 7777 + i),
                1 => vec![(
                    format!("/projects/{slot}/m1/shader.glsl"),
                    text(seed * 91 + i, 900),
                )],
                _ => vec![
                    (
                        format!("/projects/{slot}/m0/shader.glsl"),
                        text(seed * 93 + i, 900),
                    ),
                    (
                        format!("/projects/{slot}/m2/shader.glsl"),
                        text(seed * 97 + i, 900),
                    ),
                ],
            };
            fitted += push(&mut st, &edit).is_ok() as u32;
        }
        let mut st = mount(st.into_flash(), &cfg);
        assert_eq!(snapshot(&mut st).len(), 12 * copies as usize);
    }
    assert!(fitted >= 6, "{fitted} of 12 edits fitted");
}

type Files = Vec<(String, Vec<u8>)>;

/// [`spread_garbage_never_refuses_then_accepts`]'s store, 16 sectors: `big`
/// and `small` noise files in turn (at most `files` of each, or until a
/// write is refused), then every small one deleted in one transaction
/// (`None` if that transaction is refused).
fn spread_garbage(cfg: &StoreConfig, big: usize, small: usize, files: u64) -> Option<Store> {
    let mut st = mount(formatted(NorGeometry::c6(16), cfg), cfg);
    let mut smalls = Vec::new();
    for i in 0..files {
        if st
            .put(&format!("/d{}/b{i}.bin", i % 6), &noise(i, big))
            .is_err()
        {
            break;
        }
        let p = format!("/d{}/s{i}.bin", i % 6);
        if st.put(&p, &noise(i + 7777, small)).is_err() {
            break;
        }
        smalls.push(p);
    }
    st.begin().unwrap();
    for p in &smalls {
        st.delete(p).ok()?;
    }
    st.commit().ok()?;
    Some(mount(st.into_flash(), cfg))
}

/// [`a_rerun_after_a_cut_fits_where_the_step_fitted`]'s store, 16 sectors,
/// and the number of project copies in it.
fn full_store(cfg: &StoreConfig, seed: u64) -> (lp_nor_sim::NorFlashSim, u64) {
    let mut st = mount(formatted(NorGeometry::c6(16), cfg), cfg);
    let mut copies = 0;
    while push(
        &mut st,
        &project(&format!("f{copies}"), seed * 100 + copies),
    )
    .is_ok()
    {
        copies += 1;
    }
    let mut k = 0u64;
    for size in [2000, 1000, 500, 250, 120, 60] {
        while st
            .put(&format!("/fill/x{k}.bin"), &noise(seed * 1000 + k, size))
            .is_ok()
        {
            k += 1;
        }
    }
    assert!(copies > 0);
    (st.into_flash(), copies)
}

/// A small project (a dozen documents, one 900-byte shader per module),
/// every document prefixed with its slot so no two copies share a chunk.
fn project(slot: &str, seed: u64) -> Files {
    let mut out = Vec::new();
    let mut doc = |rel: &str, len: usize, s: u64| {
        let mut b = format!("// copy {slot}\n").into_bytes();
        b.extend_from_slice(&text(seed ^ s, len));
        out.push((format!("/projects/{slot}/{rel}"), b));
    };
    doc("project.json", 80, 1);
    doc("playlist.json", 60, 2);
    doc("output.json", 50, 3);
    for m in 0..3u64 {
        doc(&format!("m{m}/node.json"), 120, 10 + m);
        doc(&format!("m{m}/shader.glsl"), 900, 20 + m);
        doc(&format!("m{m}/params.json"), 200, 30 + m);
    }
    out
}

/// `files` as one transaction (aborted on a refusal).
fn push(st: &mut Store, files: &Files) -> Result<(), StoreError<NorError>> {
    st.begin()?;
    for (p, b) in files {
        if let Err(e) = st.put(p, b) {
            let _ = st.abort();
            return Err(e);
        }
    }
    st.commit()
}
