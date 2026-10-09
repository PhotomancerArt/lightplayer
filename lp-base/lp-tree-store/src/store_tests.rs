//! Behaviour tests of `TreeStore` on the NOR model: round trips, dedup,
//! multi-part nodes, big directories under GC, torn roots, untrusted
//! sectors, garbage flash, `NoSpace`, GC, and verify-after-write retiring a
//! worn sector.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use lp_nor_sim::{FaultPlan, NorFlashSim, NorGeometry, TearModel, WearMode, WearOut};

use crate::object_id::IdTag;
use crate::record_header::encode_header;
use crate::record_kind::{ChunkCodec, RecordKind};
use crate::root_record::RootRecord;
use crate::test_support::{formatted, mount, noise, snapshot, text};
use crate::{GcPolicy, ObjectId, SoftSha256, StoreConfig, StoreError, TreeStore};

fn cfg() -> StoreConfig {
    StoreConfig::default()
}

#[test]
fn round_trip_list_delete_and_remount() {
    let c = cfg();
    let mut st = mount(formatted(NorGeometry::c6(32), &c), &c);
    let shader = text(1, 9000);
    st.put("/projects/a/project.json", b"{\"name\": \"a\"}")
        .unwrap();
    st.put("/projects/a/modules/m/shader.glsl", &shader)
        .unwrap();
    st.put("/projects/a/.lp/panel.json", b"{}").unwrap();
    st.put("/hardware.json", &text(2, 300)).unwrap();
    assert_eq!(
        st.get("/projects/a/modules/m/shader.glsl")
            .unwrap()
            .unwrap(),
        shader
    );
    assert_eq!(
        st.file_size("/projects/a/modules/m/shader.glsl").unwrap(),
        Some(9000)
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
    assert!(st.delete("/hardware.json").unwrap());
    assert!(!st.delete("/hardware.json").unwrap());
    let before = snapshot(&mut st);
    assert_eq!(before.len(), 2);
    let mut st = mount(st.into_flash(), &c);
    assert_eq!(snapshot(&mut st), before);
    assert_eq!(st.get("/nope").unwrap(), None);
    assert_eq!(st.get("/").unwrap(), None);
    assert_eq!(st.put("/a/", b"x"), Err(StoreError::InvalidPath));
}

#[test]
fn valid_paths() {
    use crate::{MAX_DEPTH, valid_path};
    for ok in ["/a", "/a/b.json", "/.lp/panel.json", "/é/ü"] {
        assert!(valid_path(ok), "{ok:?}");
    }
    for bad in ["", "/", "a", "a/b", "/a/", "//a", "/a//b", "/a/b/"] {
        assert!(!valid_path(bad), "{bad:?}");
    }
    let deep = |n: usize| "/d".repeat(n);
    assert!(valid_path(&deep(MAX_DEPTH)));
    assert!(!valid_path(&deep(MAX_DEPTH + 1)));
    assert!(!valid_path(&alloc::format!(
        "/{}",
        "x".repeat(usize::from(u16::MAX))
    )));
}

/// `delete_prefix` takes a whole directory (`"<dir>/"`) and nothing else; a
/// directory that is not there is a no-op that writes nothing.
#[test]
fn delete_prefix_takes_whole_directories_only() {
    let c = cfg();
    let mut st = mount(formatted(NorGeometry::c6(32), &c), &c);
    st.put("/a/x.json", b"x").unwrap();
    st.put("/ab.json", b"ab").unwrap();
    st.put("/a/.lp/panel.json", b"{}").unwrap();
    for bad in ["/a", "/", "", "a/", "/a//"] {
        assert_eq!(
            st.delete_prefix(bad),
            Err(StoreError::InvalidPath),
            "{bad:?}"
        );
    }
    let written = st.stats().record_bytes_written;
    st.delete_prefix("/nope/").unwrap();
    st.delete_file_and_tree("/nope").unwrap();
    assert_eq!(st.stats().record_bytes_written, written, "a no-op wrote");
    st.delete_prefix("/a/").unwrap();
    assert_eq!(st.list("/").unwrap(), vec![String::from("/ab.json")]);
    let mut st = mount(st.into_flash(), &c);
    assert_eq!(st.list("/").unwrap(), vec![String::from("/ab.json")]);
}

#[test]
fn dedup_by_id_writes_once_and_an_unchanged_write_writes_nothing() {
    let c = cfg();
    let mut st = mount(formatted(NorGeometry::c6(32), &c), &c);
    let body = text(3, 5000);
    st.put("/a/x.json", &body).unwrap();
    let written = st.stats().record_bytes_written;
    let hits = st.stats().dedup_hits;
    st.put("/b/x.json", &body).unwrap();
    let s = st.stats();
    assert!(s.dedup_hits > hits);
    // Only the changed directories and a root: far less than the file.
    assert!(
        s.record_bytes_written - written < 200,
        "{}",
        s.record_bytes_written - written
    );
    st.put("/b/x.json", &body).unwrap();
    assert_eq!(st.stats().record_bytes_written, s.record_bytes_written);
    assert_eq!(st.stats().commits, s.commits, "no root for no change");
}

#[test]
fn multi_part_nodes_and_big_directories_survive_gc() {
    let c = StoreConfig {
        record_max: 256,
        ..cfg()
    };
    let mut st = mount(formatted(NorGeometry::c6(24), &c), &c);
    let big = noise(4, 40_000);
    st.put("/big.bin", &big).unwrap();
    let names: Vec<String> = (0..40)
        .map(|i| alloc::format!("/d/file-with-a-long-name-{i:03}.json"))
        .collect();
    for (i, p) in names.iter().enumerate() {
        st.put(p, &text(i as u64, 50)).unwrap();
    }
    // Churn until GC has collected every sector a few times over: the big
    // directory's files must be marked live through its multi.
    for round in 0..40u64 {
        st.put("/churn.bin", &noise(round, 6000)).unwrap();
    }
    assert!(st.stats().gc_runs > 10, "{:?}", st.stats());
    let mut st = mount(st.into_flash(), &c);
    assert_eq!(st.get("/big.bin").unwrap().unwrap(), big);
    for (i, p) in names.iter().enumerate() {
        assert_eq!(st.get(p).unwrap().unwrap(), text(i as u64, 50), "{p}");
    }
}

#[test]
fn torn_root_falls_back_and_torn_sector_is_closed() {
    let c = cfg();
    let mut st = mount(formatted(NorGeometry::c6(16), &c), &c);
    st.put("/a.json", b"one").unwrap();
    let pre = st.into_flash();
    let mut st = mount(pre.clone(), &c);
    st.flash_mut().set_plan(FaultPlan::none());
    st.put("/a.json", b"two").unwrap();
    let n = st.flash().ops_since_plan();
    for tear in [TearModel::BytePrefix, TearModel::RandomBits] {
        let mut st = mount(pre.clone(), &c);
        st.flash_mut().set_plan(FaultPlan::cut(n - 1, tear, 7));
        assert!(matches!(
            st.put("/a.json", b"two"),
            Err(StoreError::Flash(_))
        ));
        let mut f = st.into_flash();
        f.power_cycle(FaultPlan::none());
        let mut st = mount(f, &c);
        assert_eq!(st.get("/a.json").unwrap().unwrap(), b"one");
        st.put("/a.json", b"three").unwrap();
        let mut st = mount(st.into_flash(), &c);
        assert_eq!(st.get("/a.json").unwrap().unwrap(), b"three");
    }
}

#[test]
fn a_sector_without_a_header_is_never_trusted() {
    let c = cfg();
    let mut st = mount(formatted(NorGeometry::c6(16), &c), &c);
    st.put("/a.json", b"one").unwrap();
    let mut f = st.into_flash();
    let root = RootRecord {
        seq: 1_000_000,
        cold_dir: ObjectId(5),
        hot_dir: ObjectId(6),
        retired: vec![],
    };
    let payload = root.encode();
    let id = ObjectId::of(&mut SoftSha256, IdTag::Root, &[&payload]);
    let h = encode_header(RecordKind::Root, ChunkCodec::Stored, id, &[&payload]);
    let s = 15 * 4096;
    f.program(s + 20, &h).unwrap();
    f.program(s + 36, &payload).unwrap();
    let mut st = mount(f, &c);
    assert_eq!(st.get("/a.json").unwrap().unwrap(), b"one");
}

#[test]
fn garbage_flash_never_panics_and_formats() {
    let c = cfg();
    for seed in 0..8 {
        let f = NorFlashSim::garbage(NorGeometry::c6(16), seed);
        let mut f = match TreeStore::mount(f, SoftSha256, c.clone()) {
            Ok(_) => panic!("garbage mounted"),
            Err((_, f, _)) => f,
        };
        TreeStore::format(&mut f, &mut SoftSha256, &c).unwrap();
        let mut st = mount(f, &c);
        assert!(st.list("/").unwrap().is_empty());
    }
}

#[test]
fn no_space_before_writing_anything() {
    let c = cfg();
    let mut st = mount(formatted(NorGeometry::c6(8), &c), &c);
    st.put("/small.json", b"keep me").unwrap();
    let programs = st.flash().stats().program_calls;
    let erases = st.flash().stats().erases_total();
    assert_eq!(
        st.put("/huge.bin", &noise(9, 40_000)),
        Err(StoreError::NoSpace)
    );
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
    st.put("/small.json", b"still works").unwrap();
    let mut st = mount(st.into_flash(), &c);
    assert_eq!(st.get("/small.json").unwrap().unwrap(), b"still works");
    assert_eq!(st.get("/huge.bin").unwrap(), None);
}

#[test]
fn gc_keeps_live_and_reclaims_garbage() {
    for policy in [GcPolicy::Greedy, GcPolicy::CostBenefit] {
        let c = StoreConfig {
            gc_policy: policy,
            ..cfg()
        };
        let mut st = mount(formatted(NorGeometry::c6(16), &c), &c);
        st.put("/keep.bin", &noise(100, 6000)).unwrap();
        for round in 0..60u64 {
            st.begin().unwrap();
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
        // Wholly-garbage sectors are freed by erasing alone; either way the
        // flash was reused many times over.
        assert!(
            st.stats().gc_runs > 0 || st.stats().erases > 16,
            "{policy:?}: {:?}",
            st.stats()
        );
        assert!(st.free_sectors() >= c.reserve);
    }
}

#[test]
fn verify_after_write_retires_a_worn_sector_and_the_retirement_survives_remount() {
    for mode in [WearMode::EraseFails, WearMode::ProgramFails] {
        let c = cfg();
        let mut f = formatted(NorGeometry::c6(16), &c);
        // Every sector was erased once by format; sector 9 fails from its
        // next erase (or program) on.
        f.add_wear_out(WearOut {
            sector: 9,
            after_erases: 0,
            mode,
            seed: 3,
        });
        let mut st = mount(f, &c);
        let mut failures = 0;
        for round in 0..80u64 {
            st.put("/churn.bin", &noise(round, 7000)).unwrap();
            st.put("/keep.json", &text(round, 900)).unwrap();
            failures = failures.max(st.stats().verify_failures);
            if st.stats().retired_sectors > 0 && round % 7 == 0 {
                st = mount(st.into_flash(), &c);
                assert!(
                    st.log.sectors.is_retired(9),
                    "{mode:?}: retirement lost on remount"
                );
            }
            assert_eq!(st.get("/churn.bin").unwrap().unwrap(), noise(round, 7000));
            assert_eq!(st.get("/keep.json").unwrap().unwrap(), text(round, 900));
        }
        let s = st.stats();
        assert_eq!(s.retired_sectors, 1, "{mode:?}: {s:?}");
        assert!(failures >= 1);
        let st = mount(st.into_flash(), &c);
        assert_eq!(st.log.sectors.retired, vec![9]);
        assert!(st.log.heads.iter().all(|h| *h != Some(9)));
    }
}
