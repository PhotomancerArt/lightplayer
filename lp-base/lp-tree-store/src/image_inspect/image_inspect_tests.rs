//! The inspector against the real store: images the writer made (empty,
//! c40-shaped, after GC, with a torn root, with a retired sector) must give
//! the same root, the same live bytes and the same files as the real mount,
//! and the images no writer makes (a sector at a newer version, a directory
//! with bad names, a chunk that does not hash to its id) must be named.

extern crate std;

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use lp_crc32::Crc32;
use lp_nor_sim::{NorFlashSim, NorGeometry, WearMode, WearOut};

use super::*;
use crate::dir_node::{DirEntry, EntryKind, encode_dir};
use crate::object_id::{IdTag, ObjectId};
use crate::ram_budget_tests::c40_like;
use crate::record_header::encode_header;
use crate::record_kind::{ChunkCodec, RecordKind};
use crate::root_record::RootRecord;
use crate::test_support::{Store, formatted, mount, noise, snapshot, text};
use crate::{SoftSha256, StoreConfig, StoreError, TreeStore};

fn cfg() -> StoreConfig {
    StoreConfig::default()
}

#[test]
fn an_empty_store_has_a_root_and_no_files() {
    let c = cfg();
    let f = formatted(NorGeometry::c6(16), &c);
    let image = image_of(&f);
    let img = StoreImage::open(&image, None).unwrap();
    let r = img.report();
    assert_eq!(r.sector_size, 4096);
    assert_eq!(r.sector_size_from, SectorSizeFrom::Headers);
    assert_eq!(r.sector_count, 16);
    assert_eq!(r.mount, MountVerdict::Mounts);
    assert_eq!(r.roots.len(), 1);
    assert_eq!(r.chosen_root().unwrap().seq, 1);
    assert!(r.tree.is_empty());
    // Format puts the empty directory in a cold sector and the root in a
    // hot one.
    let valid: Vec<_> = r
        .sectors
        .iter()
        .filter(|s| s.state == SectorState::Valid)
        .collect();
    assert_eq!(valid.len(), 2);
    assert_eq!(valid[0].state.label(), "valid");
    assert!(
        r.sectors
            .iter()
            .filter(|s| s.state == SectorState::Blank)
            .count()
            == 14
    );
    let check = img.check(&mut SoftSha256, None);
    assert!(check.is_consistent(), "{:#?}", check.findings);
    assert_eq!(img.extract().unwrap().files, vec![]);
    agrees_with_the_real_mount(&image, &c, NorGeometry::c6(16));
}

#[test]
fn a_blank_flash_is_no_store() {
    let image = vec![0xFFu8; 8 * 4096];
    let img = StoreImage::open(&image, None).unwrap();
    assert_eq!(img.report().sector_size_from, SectorSizeFrom::Assumed);
    assert_eq!(
        img.report().mount,
        MountVerdict::NoStore {
            why: "every sector is erased"
        }
    );
    let check = img.check(&mut SoftSha256, None);
    assert!(!check.is_consistent());
    assert!(check.findings.iter().any(|f| f.code == "no-store"));
    assert!(img.extract().is_err());
}

#[test]
fn a_c40_store_after_gc_matches_the_real_mount_exactly() {
    let c = cfg();
    let geom = NorGeometry::c6(128);
    let mut st = mount(formatted(geom, &c), &c);
    st.begin().unwrap();
    for (p, b) in c40_like() {
        st.put(&p, &b).unwrap();
    }
    st.commit().unwrap();
    // Churn until the writer has collected garbage at least a few times.
    for round in 0..80u64 {
        st.put("/churn.bin", &noise(round, 6000)).unwrap();
        st.put("/projects/a/.lp/panel.json", &text(round, 300))
            .unwrap();
        // Live records land between the garbage, so GC has to copy some.
        st.put(&format!("/keep/k{round:02}.json"), &text(round, 200))
            .unwrap();
    }
    assert!(st.stats().gc_copies > 3, "{:?}", st.stats());
    let want = snapshot(&mut st);
    let image = image_of(st.flash());
    let img = StoreImage::open(&image, None).unwrap();

    let r = img.report();
    assert_eq!(r.mount, MountVerdict::Mounts);
    assert!(r.garbage_bytes > 0, "a churned store holds garbage");
    // The files, exactly.
    let ex = img.extract().unwrap();
    assert!(ex.skipped.is_empty() && ex.warnings.is_empty(), "{ex:?}");
    let got: Vec<(String, Vec<u8>)> = ex.files.into_iter().map(|f| (f.path, f.bytes)).collect();
    let want: Vec<(String, Vec<u8>)> = want.into_iter().collect();
    assert_eq!(got.len(), want.len());
    let mut got = got;
    got.sort();
    assert_eq!(got, want);
    // The tree lists every file with its size.
    let files = r
        .tree
        .iter()
        .filter(|e| e.kind == EntryKindReport::File)
        .count();
    assert_eq!(files, want.len());
    let big = r
        .tree
        .iter()
        .find(|e| e.path == "/projects/a/maps/map0.json")
        .unwrap();
    assert_eq!(big.size, 18_000);
    assert!(!big.hot);
    assert!(
        r.tree
            .iter()
            .any(|e| e.path == "/projects/a/.lp/panel.json" && e.hot)
    );

    agrees_with_the_real_mount(&image, &c, geom);
    let check = img.check(&mut SoftSha256, None);
    assert!(check.is_consistent(), "{:#?}", check.findings);
    assert_eq!(check.files_verified as usize, want.len());
    assert!(check.orphan_records > 0);
    assert!(check.findings.iter().any(|f| f.code == "orphans"));
}

#[test]
fn a_torn_root_falls_back_to_the_root_before_it() {
    let c = cfg();
    let geom = NorGeometry::c6(16);
    let mut st = mount(formatted(geom, &c), &c);
    st.put("/a.json", b"{\"v\": 1}").unwrap();
    let before = snapshot(&mut st);
    st.put("/b.json", b"{\"v\": 2}").unwrap();
    let after = snapshot(&mut st);
    assert_ne!(before, after);
    let mut image = image_of(st.flash());

    let img = StoreImage::open(&image, None).unwrap();
    let newest = img.report().roots[0].clone();
    assert_eq!(img.report().chosen, Some(0));
    drop(img);
    // Tear the newest root: its CRC no longer matches.
    let at = newest.sector as usize * 4096 + newest.offset as usize;
    image[at + 12] ^= 0x5A;

    let img = StoreImage::open(&image, None).unwrap();
    let r = img.report();
    assert_eq!(r.mount, MountVerdict::Mounts);
    assert_eq!(r.chosen_root().unwrap().seq, newest.seq - 1);
    let s = &r.sectors[newest.sector as usize];
    assert!(s.closed.unwrap().contains("CRC"), "{:?}", s.closed);
    assert_eq!(s.records.last().unwrap().status, RecordStatus::Untrusted);
    let ex = img.extract().unwrap();
    let got: Vec<_> = ex.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(got, vec!["/a.json"]);
    // A torn tail is a handled crash: a warning, not an inconsistency.
    let check = img.check(&mut SoftSha256, None);
    assert!(check.is_consistent(), "{:#?}", check.findings);
    assert_eq!(check.warnings, 1);
    assert!(check.findings.iter().any(|f| f.code == "closed-sector"));
    agrees_with_the_real_mount(&image, &c, geom);
}

#[test]
fn a_retired_sector_is_marked_and_survives_in_the_report() {
    let c = cfg();
    let mut f = formatted(NorGeometry::c6(16), &c);
    f.add_wear_out(WearOut {
        sector: 9,
        after_erases: 0,
        mode: WearMode::EraseFails,
        seed: 3,
    });
    let mut st = mount(f, &c);
    for round in 0..80u64 {
        st.put("/churn.bin", &noise(round, 7000)).unwrap();
        st.put("/keep.json", &text(round, 900)).unwrap();
        if st.stats().retired_sectors > 0 && round > 20 {
            break;
        }
    }
    assert_eq!(st.stats().retired_sectors, 1);
    let image = image_of(st.flash());
    let img = StoreImage::open(&image, None).unwrap();
    let r = img.report();
    assert_eq!(r.chosen_root().unwrap().retired, vec![9]);
    assert!(r.sectors[9].retired);
    assert_eq!(r.sectors.iter().filter(|s| s.retired).count(), 1);
    let check = img.check(&mut SoftSha256, None);
    assert!(check.is_consistent(), "{:#?}", check.findings);
    assert!(
        check
            .findings
            .iter()
            .any(|f| f.code == "retired-sector" && f.sector == Some(9))
    );
    agrees_with_the_real_mount(&image, &c, NorGeometry::c6(16));
}

#[test]
fn a_sector_at_a_newer_version_refuses_the_mount_but_the_rest_extracts() {
    let c = cfg();
    let geom = NorGeometry::c6(16);
    let mut st = mount(formatted(geom, &c), &c);
    st.put("/projects/a/project.json", b"{\"name\": \"a\"}")
        .unwrap();
    st.put("/hardware.json", &text(1, 500)).unwrap();
    let want = snapshot(&mut st);
    let mut image = image_of(st.flash());
    let blank = StoreImage::open(&image, None)
        .unwrap()
        .report()
        .sectors
        .iter()
        .find(|s| s.state == SectorState::Blank)
        .unwrap()
        .index;
    // A newer writer's header: the magic, version 4, and anything after.
    let at = blank as usize * 4096;
    image[at..at + 4].copy_from_slice(&0x3153_544Cu32.to_le_bytes());
    image[at + 4..at + 6].copy_from_slice(&4u16.to_le_bytes());
    image[at + 6..at + 24].fill(0xA5);

    // The real mount refuses, and says it is not blank.
    let flash = flash_of(&image, geom);
    let Err((e, _, _)) = TreeStore::mount(flash, SoftSha256, c.clone()) else {
        panic!("the store mounted over a newer sector");
    };
    assert_eq!(e, StoreError::Unsupported("newer format"));

    let img = StoreImage::open(&image, None).unwrap();
    let r = img.report();
    assert_eq!(
        r.sectors[blank as usize].state,
        SectorState::Newer { version: 4 }
    );
    assert_eq!(r.sectors[blank as usize].state.label(), "NEWER");
    assert_eq!(
        r.mount,
        MountVerdict::Refused {
            sectors: vec![blank],
            rest_is_complete_store: true
        }
    );
    let check = img.check(&mut SoftSha256, None);
    assert!(!check.is_consistent());
    let codes: Vec<_> = check.findings.iter().map(|f| f.code).collect();
    assert!(codes.contains(&"newer-sector"), "{codes:?}");
    assert!(codes.contains(&"rest-complete"), "{codes:?}");
    // Extract reads the version-3 sectors anyway.
    let ex = img.extract().unwrap();
    let got: Vec<(String, Vec<u8>)> = ex.files.into_iter().map(|f| (f.path, f.bytes)).collect();
    assert_eq!(got, want.into_iter().collect::<Vec<_>>());
}

#[test]
fn a_newer_sector_that_held_the_newest_root_leaves_the_rest_incomplete() {
    let c = cfg();
    let geom = NorGeometry::c6(16);
    let mut st = mount(formatted(geom, &c), &c);
    st.put("/a.json", b"{}").unwrap();
    let mut image = image_of(st.flash());
    let root_sector = StoreImage::open(&image, None).unwrap().report().roots[0].sector;
    // A leaked bit: the version in a header that held every root.
    image[root_sector as usize * 4096 + 4] = 4;
    let img = StoreImage::open(&image, None).unwrap();
    assert_eq!(
        img.report().mount,
        MountVerdict::Refused {
            sectors: vec![root_sector],
            rest_is_complete_store: false
        }
    );
    let check = img.check(&mut SoftSha256, None);
    assert!(check.findings.iter().any(|f| f.code == "rest-incomplete"));
    assert!(img.extract().is_err());
}

#[test]
fn directory_names_that_break_the_writers_rules_are_flagged_everywhere() {
    let c = cfg();
    let geom = NorGeometry::c6(16);
    let mut st = mount(formatted(geom, &c), &c);
    st.put("/ok.json", b"{\"fine\": true}").unwrap();
    st.put("/p/.lp/panel.json", b"{}").unwrap();
    let mut image = image_of(st.flash());
    let (file_id, file_size, hot_id, hot_size) = {
        let img = StoreImage::open(&image, None).unwrap();
        let find = |path: &str| {
            let e = img.report().tree.iter().find(|e| e.path == path).unwrap();
            (ObjectId(e.id), e.size)
        };
        let (f, fs) = find("/ok.json");
        let (h, hs) = find("/p/.lp/panel.json");
        (f, fs, h, hs)
    };
    // format's empty directory is still on flash until GC takes it.
    let empty_dir = ObjectId::of(&mut SoftSha256, IdTag::Dir, &[&[0, 0]]);
    let entry = |name: &[u8], kind, id, size| DirEntry {
        name: name.to_vec(),
        kind,
        size,
        id,
    };
    forge_root(
        &mut image,
        vec![
            entry(b"", EntryKind::File, file_id, file_size),
            entry(b"a/b", EntryKind::File, file_id, file_size),
            entry(&[0xFF, b'x'], EntryKind::File, file_id, file_size),
            entry(b"ok.json", EntryKind::File, file_id, file_size),
        ],
        vec![
            // Good.
            entry(b"/p/.lp/panel.json", EntryKind::File, hot_id, hot_size),
            // Not a hot path, and a directory.
            entry(b"/not-hot.json", EntryKind::File, hot_id, hot_size),
            entry(b"/zz/.lp/panel.json", EntryKind::Dir, empty_dir, 0),
        ],
    );

    // Mount accepts the CRC-good tree; `list` calls it corrupt (P11).
    let mut real = mount(flash_of(&image, geom), &c);
    assert!(matches!(
        real.list("/"),
        Err(StoreError::Corrupt("dir entry name"))
    ));

    let img = StoreImage::open(&image, None).unwrap();
    assert_eq!(img.report().mount, MountVerdict::Mounts);
    let problems: Vec<_> = img
        .report()
        .tree
        .iter()
        .filter_map(|e| e.name_problem)
        .collect();
    assert_eq!(
        problems,
        vec!["empty name", "name contains '/'", "name is not UTF-8"]
    );
    let check = img.check(&mut SoftSha256, None);
    assert!(!check.is_consistent());
    let count = |code: &str| check.findings.iter().filter(|f| f.code == code).count();
    assert_eq!(count("dir-name"), 3, "{:#?}", check.findings);
    assert_eq!(count("hot-entry"), 2, "{:#?}", check.findings);
    // Extract writes the good file and names the rest.
    let ex = img.extract().unwrap();
    let paths: Vec<_> = ex.files.iter().map(|f| f.path.as_str()).collect();
    assert!(paths.contains(&"/ok.json") && paths.contains(&"/p/.lp/panel.json"));
    assert_eq!(ex.skipped.len(), 3, "{:?}", ex.skipped);
}

#[test]
fn a_chunk_that_does_not_hash_to_its_id_is_an_error_though_mount_accepts_it() {
    let c = cfg();
    let geom = NorGeometry::c6(16);
    let mut st = mount(formatted(geom, &c), &c);
    st.put("/a.json", &text(1, 300)).unwrap();
    let mut image = image_of(st.flash());
    let (sector, rec) = {
        let img = StoreImage::open(&image, None).unwrap();
        let (s, r) = img
            .report()
            .sectors
            .iter()
            .flat_map(|s| s.records.iter().map(move |r| (s.index, r)))
            .find(|(_, r)| r.kind == RecordKindReport::Blob && r.status == RecordStatus::Live)
            .unwrap();
        (s, r.clone())
    };
    // Change a payload byte and re-seal the record's CRC: a record that
    // checks, holding bytes its id does not hash.
    let at = sector as usize * 4096 + rec.offset as usize;
    image[at + 16] ^= 0x01;
    let mut crc = Crc32::new();
    crc.update(&image[at..at + 12]);
    crc.update(&image[at + 16..at + 16 + usize::from(rec.len)]);
    image[at + 12..at + 16].copy_from_slice(&crc.finish().to_le_bytes());

    let mut real = mount(flash_of(&image, geom), &c);
    assert!(real.get("/a.json").is_ok(), "mount does not hash chunks");
    let img = StoreImage::open(&image, None).unwrap();
    let check = img.check(&mut SoftSha256, None);
    assert!(!check.is_consistent());
    let codes: Vec<_> = check.findings.iter().map(|f| f.code).collect();
    assert!(
        codes.contains(&"id-mismatch") && codes.contains(&"file-node"),
        "{codes:?}"
    );
}

#[test]
fn two_reads_that_differ_name_the_weak_sector() {
    let c = cfg();
    let f = formatted(NorGeometry::c6(8), &c);
    let image = image_of(&f);
    let mut second = image.clone();
    second[3 * 4096 + 100] ^= 0x08;
    let img = StoreImage::open(&image, None).unwrap();
    let check = img.check(&mut SoftSha256, Some(&second));
    let weak: Vec<_> = check
        .findings
        .iter()
        .filter(|f| f.code == "weak-sector")
        .collect();
    assert_eq!(weak.len(), 1);
    assert_eq!(weak[0].sector, Some(3));
    assert!(!check.is_consistent());
    assert!(img.check(&mut SoftSha256, Some(&image)).is_consistent());
}

#[test]
fn the_sector_size_comes_from_the_headers_or_the_caller() {
    let c = cfg();
    let geom = NorGeometry::new(32, 2048, 256);
    let mut st = mount(formatted(geom, &c), &c);
    st.put("/a.json", b"{}").unwrap();
    let image = image_of(st.flash());
    assert_eq!(detect_sector_size(&image), Some(2048));
    let img = StoreImage::open(&image, None).unwrap();
    assert_eq!(img.report().sector_size, 2048);
    assert_eq!(img.report().sector_count, 32);
    assert_eq!(img.report().mount, MountVerdict::Mounts);
    assert_eq!(
        StoreImage::open(&image, Some(3000)).err(),
        Some(ImageError::BadSectorSize(3000))
    );
    assert_eq!(
        StoreImage::open(&[0; 100], None).err(),
        Some(ImageError::TooSmall)
    );
    // A wrong size given: every header names another sector size.
    let wrong = StoreImage::open(&image, Some(4096)).unwrap();
    assert!(wrong.report().refusing_sectors().len() >= 1);
}

#[test]
fn extract_will_not_write_outside_its_directory() {
    let c = cfg();
    let geom = NorGeometry::c6(16);
    let mut st = mount(formatted(geom, &c), &c);
    st.put("/ok.json", b"{}").unwrap();
    let mut image = image_of(st.flash());
    let (id, size) = {
        let img = StoreImage::open(&image, None).unwrap();
        let e = &img.report().tree[0];
        (ObjectId(e.id), e.size)
    };
    let entry = |name: &[u8]| DirEntry {
        name: name.to_vec(),
        kind: EntryKind::File,
        size,
        id,
    };
    forge_root(&mut image, vec![entry(b".."), entry(b"fine")], vec![]);
    let img = StoreImage::open(&image, None).unwrap();
    let ex = img.extract().unwrap();
    assert_eq!(ex.files.len(), 1);
    assert_eq!(ex.files[0].path, "/fine");
    assert_eq!(ex.skipped.len(), 1);
}

#[test]
fn damaged_images_never_panic_the_inspector() {
    let c = cfg();
    let mut st = mount(formatted(NorGeometry::c6(16), &c), &c);
    st.put("/a/b.json", &text(1, 3000)).unwrap();
    st.put("/big.bin", &noise(2, 9000)).unwrap();
    st.put("/p/.lp/panel.json", b"{}").unwrap();
    let image = image_of(st.flash());
    let mut x = 0x2545_F491_4F6C_DD1Du64;
    let mut next = move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    for round in 0..400 {
        let mut damaged = image.clone();
        // Bytes anywhere, and now and then a burst inside the first sector
        // of records, where the directories and the root live.
        for _ in 0..1 + round % 5 {
            let at = if round % 3 == 0 {
                (next() % 3000) as usize
            } else {
                (next() % damaged.len() as u64) as usize
            };
            damaged[at] = next() as u8;
        }
        let img = StoreImage::open(&damaged, None).unwrap();
        let _ = img.check(&mut SoftSha256, None);
        let _ = img.extract();
        let _ = img.report().mount.clone();
    }
    // Pure noise, and every length near a sector boundary.
    for len in [512usize, 513, 4096, 4097, 8191, 16384] {
        let noise: Vec<u8> = (0..len).map(|_| next() as u8).collect();
        if let Ok(img) = StoreImage::open(&noise, None) {
            let _ = img.check(&mut SoftSha256, None);
            let _ = img.extract();
        }
    }
}

// ---- helpers -----------------------------------------------------------------

fn image_of(f: &NorFlashSim) -> Vec<u8> {
    let g = f.geometry();
    let mut out = vec![0u8; g.capacity() as usize];
    f.peek(0, &mut out);
    out
}

fn flash_of(image: &[u8], geom: NorGeometry) -> NorFlashSim {
    let mut f = NorFlashSim::new(geom);
    for (s, chunk) in image.chunks(geom.sector_size as usize).enumerate() {
        f.program(s as u32 * geom.sector_size, chunk).unwrap();
    }
    f
}

/// The real mount, given the same bytes: the same root, the same live bytes
/// in every sector, the same files.
fn agrees_with_the_real_mount(image: &[u8], c: &StoreConfig, geom: NorGeometry) {
    let img = StoreImage::open(image, None).unwrap();
    let r = img.report();
    let mut real: Store = mount(flash_of(image, geom), c);
    let committed = real.committed.clone().unwrap();
    let chosen = r.chosen_root().expect("a root");
    assert_eq!(chosen.seq, committed.root.seq);
    assert_eq!(chosen.id, committed.id.0);
    assert_eq!(chosen.retired, committed.root.retired);
    let live: Vec<u32> = r.sectors.iter().map(|s| s.live_bytes).collect();
    let want: Vec<u32> = real
        .log
        .sectors
        .live
        .iter()
        .map(|&l| u32::from(l))
        .collect();
    assert_eq!(live, want, "live bytes per sector");
    let files: Vec<String> = r
        .tree
        .iter()
        .filter(|e| e.kind == EntryKindReport::File)
        .map(|e| e.path.clone())
        .collect();
    let mut files = files;
    files.sort();
    if let Ok(listed) = real.list("/") {
        assert_eq!(files, listed);
    }
    for (s, sr) in r.sectors.iter().enumerate() {
        assert_eq!(
            sr.retired,
            real.log.sectors.is_retired(s as u32),
            "retired flag of sector {s}"
        );
    }
    let _ = format!("{:?}", r.mount);
}

/// Append a cold directory, a hot directory and a root naming them (seq
/// one past the newest) where the newest root's sector is still erased.
fn forge_root(image: &mut Vec<u8>, mut cold: Vec<DirEntry>, mut hot: Vec<DirEntry>) {
    let (sector, end, seq, retired) = {
        let img = StoreImage::open(image, None).unwrap();
        let r = &img.report().roots[0];
        let end = img.report().sectors[r.sector as usize].records_end;
        (r.sector as usize, end as usize, r.seq, r.retired.clone())
    };
    let h = &mut SoftSha256;
    let put = |image: &mut Vec<u8>, at: &mut usize, kind, id: ObjectId, payload: &[u8]| {
        let header = encode_header(kind, ChunkCodec::Stored, id, &[payload]);
        image[*at..*at + 16].copy_from_slice(&header);
        image[*at + 16..*at + 16 + payload.len()].copy_from_slice(payload);
        *at += 16 + payload.len();
    };
    let mut at = sector * 4096 + end;
    let cold_bytes = encode_dir(&mut cold);
    let cold_id = ObjectId::of(h, IdTag::Dir, &[&cold_bytes]);
    put(image, &mut at, RecordKind::Dir, cold_id, &cold_bytes);
    let hot_bytes = encode_dir(&mut hot);
    let hot_id = ObjectId::of(h, IdTag::Dir, &[&hot_bytes]);
    put(image, &mut at, RecordKind::Dir, hot_id, &hot_bytes);
    let root = RootRecord {
        seq: seq + 1,
        cold_dir: cold_id,
        hot_dir: hot_id,
        retired,
    }
    .encode();
    let root_id = ObjectId::of(h, IdTag::Root, &[&root]);
    put(image, &mut at, RecordKind::Root, root_id, &root);
    assert!(
        at <= (sector + 1) * 4096,
        "the forged records fit the sector"
    );
}
