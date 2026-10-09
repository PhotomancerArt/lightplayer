//! Behaviour tests of `TreeStore` on the NOR model, and the exhaustive cut
//! sweeps (blob mode, every codec).

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use lp_nor_sim::{FaultPlan, NorFlashSim, NorGeometry, TearModel};

use crate::record_header::encode_record;
use crate::record_kind::{ChunkCodec, RecordKind};
use crate::root_record::RootRecord;
use crate::test_support::{Step, formatted, mount, noise, snapshot, sweep, text};
use crate::{Codec, GcPolicy, ObjectId, StoreConfig, StoreError, TreeStore};

fn cfg(codec: Codec) -> StoreConfig {
    StoreConfig {
        codec,
        dict_size: 1024,
        dict_train_min: 2048,
        ..StoreConfig::default()
    }
}

const CODECS: [Codec; 3] = [Codec::Stored, Codec::Deflate, Codec::DeflateDict];

#[test]
fn round_trip_list_delete_and_remount() {
    for codec in CODECS {
        let c = cfg(codec);
        let mut st = mount(formatted(NorGeometry::c6(32), &c), &c);
        let shader = text(1, 9000);
        st.put("/projects/a/project.json", b"{\"name\": \"a\"}")
            .unwrap();
        st.put("/projects/a/modules/m/shader.glsl", &shader)
            .unwrap();
        st.put("/projects/a/.lp/panel.json", b"{}").unwrap();
        st.put("/hardware.json", &text(2, 300)).unwrap();
        // Read-your-writes before commit.
        assert_eq!(
            st.get("/projects/a/modules/m/shader.glsl")
                .unwrap()
                .unwrap(),
            shader
        );
        st.commit().unwrap();
        assert_eq!(
            st.get("/projects/a/modules/m/shader.glsl")
                .unwrap()
                .unwrap(),
            shader
        );
        assert_eq!(
            st.list("/projects/a/").unwrap(),
            vec![
                String::from("/projects/a/.lp/panel.json"),
                String::from("/projects/a/modules/m/shader.glsl"),
                String::from("/projects/a/project.json"),
            ]
        );
        st.delete_prefix("/projects/a/modules/").unwrap();
        assert_eq!(st.get("/projects/a/modules/m/shader.glsl").unwrap(), None);
        st.commit().unwrap();
        let before = snapshot(&mut st);
        let mut st = mount(st.into_flash(), &c);
        assert_eq!(snapshot(&mut st), before, "{codec:?}");
        assert_eq!(st.get("/nope").unwrap(), None);
    }
}

#[test]
fn dedup_by_id_writes_once() {
    let c = cfg(Codec::Deflate);
    let mut st = mount(formatted(NorGeometry::c6(32), &c), &c);
    let body = text(3, 5000);
    st.put("/a/x.json", &body).unwrap();
    st.commit().unwrap();
    let written = st.stats().record_bytes_written;
    let hits = st.stats().dedup_hits;
    st.put("/b/x.json", &body).unwrap();
    st.commit().unwrap();
    let s = st.stats();
    assert!(s.dedup_hits > hits);
    // Only the changed directories and a root: far less than the file.
    assert!(
        s.record_bytes_written - written < 400,
        "{}",
        s.record_bytes_written - written
    );
    // A commit that changes nothing writes nothing.
    st.put("/b/x.json", &body).unwrap();
    st.commit().unwrap();
    assert_eq!(st.stats().record_bytes_written, s.record_bytes_written);
}

#[test]
fn multi_part_nodes_over_record_max() {
    for codec in CODECS {
        let c = StoreConfig {
            record_max: 256,
            ..cfg(codec)
        };
        let mut st = mount(formatted(NorGeometry::c6(64), &c), &c);
        let big = noise(4, 40_000);
        let mut many_files = Vec::new();
        for i in 0..40 {
            many_files.push((
                alloc::format!("/d/file-with-a-long-name-{i:03}.json"),
                text(i, 50),
            ));
        }
        st.put("/big.bin", &big).unwrap();
        for (p, b) in &many_files {
            st.put(p, b).unwrap();
        }
        st.commit().unwrap();
        let mut st = mount(st.into_flash(), &c);
        assert_eq!(st.get("/big.bin").unwrap().unwrap(), big, "{codec:?}");
        for (p, b) in &many_files {
            assert_eq!(&st.get(p).unwrap().unwrap(), b);
        }
    }
}

#[test]
fn torn_root_falls_back_and_torn_sector_is_closed() {
    let c = cfg(Codec::Stored);
    let mut st = mount(formatted(NorGeometry::c6(16), &c), &c);
    st.put("/a.json", b"one").unwrap();
    st.commit().unwrap();
    let pre = st.into_flash();
    // Count the commit's ops, then tear its last one (the root's page).
    let mut st = mount(pre.clone(), &c);
    st.put("/a.json", b"two").unwrap();
    st.flash_mut().set_plan(FaultPlan::none());
    st.commit().unwrap();
    let n = st.flash().ops_since_plan();
    for tear in [TearModel::BytePrefix, TearModel::RandomBits] {
        let mut st = mount(pre.clone(), &c);
        st.put("/a.json", b"two").unwrap();
        st.flash_mut().set_plan(FaultPlan::cut(n - 1, tear, 7));
        assert!(matches!(st.commit(), Err(StoreError::Flash(_))));
        let mut f = st.into_flash();
        f.power_cycle(FaultPlan::none());
        let mut st = mount(f, &c);
        assert_eq!(st.get("/a.json").unwrap().unwrap(), b"one");
        // Appending again must not land on the torn bytes: a fresh write
        // and a remount read back exactly.
        st.put("/a.json", b"three").unwrap();
        st.commit().unwrap();
        let mut st = mount(st.into_flash(), &c);
        assert_eq!(st.get("/a.json").unwrap().unwrap(), b"three");
    }
}

#[test]
fn a_sector_without_a_header_is_never_trusted() {
    let c = cfg(Codec::Stored);
    let mut st = mount(formatted(NorGeometry::c6(16), &c), &c);
    st.put("/a.json", b"one").unwrap();
    st.commit().unwrap();
    let mut f = st.into_flash();
    // A perfectly valid root record with a huge seq, in a sector whose
    // header was never written (format left the rest erased, unheaded).
    let root = RootRecord {
        seq: 1_000_000,
        cold_dir: ObjectId(5),
        hot_dir: ObjectId(6),
        dict: ObjectId::NONE,
        next_key_id: 0,
    };
    let payload = root.encode();
    let raw = encode_record(
        RecordKind::Root,
        ChunkCodec::Stored,
        RootRecord::id_of(&payload),
        &payload,
    );
    let s = 15 * 4096;
    f.program(s + 20, &raw).unwrap();
    let mut st = mount(f, &c);
    assert_eq!(st.get("/a.json").unwrap().unwrap(), b"one");
}

#[test]
fn garbage_flash_never_panics_and_formats() {
    let c = cfg(Codec::Deflate);
    for seed in 0..8 {
        let f = NorFlashSim::garbage(NorGeometry::c6(16), seed);
        let mut f = match TreeStore::mount(f, c.clone()) {
            Ok(_) => panic!("garbage mounted"),
            Err((_, f)) => f,
        };
        TreeStore::format(&mut f, &c).unwrap();
        let mut st = mount(f, &c);
        assert!(st.list("/").unwrap().is_empty());
    }
}

#[test]
fn no_space_before_writing_anything() {
    let c = cfg(Codec::Stored);
    let mut st = mount(formatted(NorGeometry::c6(8), &c), &c);
    st.put("/small.json", b"keep me").unwrap();
    st.commit().unwrap();
    let programs = st.flash().stats().program_calls;
    let erases = st.flash().stats().erases_total();
    st.put("/huge.bin", &noise(9, 40_000)).unwrap();
    assert_eq!(st.commit(), Err(StoreError::NoSpace));
    assert_eq!(
        st.flash().stats().program_calls,
        programs,
        "programmed before NoSpace"
    );
    assert_eq!(
        st.flash().stats().erases_total(),
        erases,
        "erased before NoSpace"
    );
    // The staged change is still there; drop it and keep going.
    st.discard_uncommitted().unwrap();
    st.put("/small.json", b"still works").unwrap();
    st.commit().unwrap();
    let mut st = mount(st.into_flash(), &c);
    assert_eq!(st.get("/small.json").unwrap().unwrap(), b"still works");
    assert_eq!(st.get("/huge.bin").unwrap(), None);
}

#[test]
fn gc_keeps_live_and_reclaims_garbage() {
    for policy in [GcPolicy::Greedy, GcPolicy::CostBenefit] {
        let c = StoreConfig {
            gc_policy: policy,
            ..cfg(Codec::Stored)
        };
        let mut st = mount(formatted(NorGeometry::c6(16), &c), &c);
        st.put("/keep.bin", &noise(100, 6000)).unwrap();
        st.commit().unwrap();
        // Rewrite ~10× the flash: only GC makes that possible.
        for round in 0..60u64 {
            st.put("/churn.bin", &noise(round, 9000)).unwrap();
            st.put("/projects/a/.lp/panel.json", &text(round, 200))
                .unwrap();
            st.commit().unwrap();
            if round % 13 == 0 {
                st = mount(st.into_flash(), &c);
            }
            assert_eq!(st.get("/keep.bin").unwrap().unwrap(), noise(100, 6000));
            assert_eq!(st.get("/churn.bin").unwrap().unwrap(), noise(round, 9000));
        }
        assert!(
            st.stats().gc_runs > 0 || st.stats().erases > 16,
            "{policy:?}"
        );
        assert!(st.free_sectors() >= c.reserve);
    }
}

#[test]
fn json_tree_is_refused() {
    let c = StoreConfig {
        json_tree: true,
        ..StoreConfig::default()
    };
    let mut f = NorFlashSim::new(NorGeometry::c6(16));
    assert!(matches!(
        TreeStore::format(&mut f, &c),
        Err(StoreError::Unsupported(_))
    ));
}

// ---- the exhaustive cut sweeps -------------------------------------------

fn basic_workload() -> Vec<Step> {
    vec![
        Box::new(|st| {
            st.put("/hardware.json", &text(10, 220))?;
            st.put("/projects/a/project.json", &text(11, 700))?;
            st.put("/projects/a/modules/m/shader.glsl", &text(12, 3500))?;
            st.put("/projects/a/.lp/panel.json", &text(13, 120))?;
            st.commit()
        }),
        Box::new(|st| {
            st.put("/projects/a/.lp/panel.json", &text(14, 130))?;
            st.commit()
        }),
        Box::new(|st| {
            st.put("/projects/a/modules/m/shader.glsl", &text(15, 3600))?;
            st.put("/projects/a/modules/n/shader.glsl", &text(12, 3500))?;
            st.commit()
        }),
        Box::new(|st| {
            st.delete_prefix("/projects/a/modules/")?;
            st.put("/projects/b/project.json", &text(16, 900))?;
            st.put("/projects/b/.lp/panel.json", &text(17, 90))?;
            st.commit()
        }),
    ]
}

#[test]
fn cut_sweep_basic_every_codec() {
    for codec in CODECS {
        let c = cfg(codec);
        let r = sweep(NorGeometry::c6(16), &c, &basic_workload(), 32);
        assert!(
            r.cuts > 100 && r.landed_old > 0 && r.landed_new > 0,
            "{codec:?}: {r:?}"
        );
    }
}

fn gc_workload() -> Vec<Step> {
    let mut steps: Vec<Step> = Vec::new();
    steps.push(Box::new(|st| {
        st.put("/keep.json", &text(20, 2500))?;
        st.put("/projects/a/.lp/panel.json", &text(21, 100))?;
        st.commit()
    }));
    for i in 0..crate::test_support::dial("LP_TREE_STORE_SWEEP_STEPS", 6) {
        steps.push(Box::new(move |st| {
            // A small file kept forever, written beside the churn, so cold
            // sectors mix live and garbage and GC has to copy.
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
    for (codec, record_max) in [
        (Codec::Stored, 1024),
        (Codec::DeflateDict, 256),
        (Codec::Deflate, 512),
    ] {
        let c = StoreConfig {
            record_max,
            ..cfg(codec)
        };
        // 10 sectors: the churn outgrows the flash within a few steps.
        let r = sweep(NorGeometry::c6(10), &c, &gc_workload(), 8);
        assert!(r.gc_runs > 0, "{codec:?}: no GC ran: {r:?}");
        assert!(r.landed_old > 0 && r.landed_new > 0, "{codec:?}: {r:?}");
    }
}
