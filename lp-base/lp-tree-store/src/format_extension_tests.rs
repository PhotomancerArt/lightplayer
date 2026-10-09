//! The format's extension room (FORMAT.md "Sector", "Root tail", "Unknown
//! records"): what this version does with flash a newer writer could have
//! produced — compat and incompat flags, another sector size, a root TLV
//! tag, a record kind it does not define.

use alloc::vec::Vec;

use lp_crc32::crc32;
use lp_nor_sim::{FaultPlan, NorFlashSim, NorGeometry, TearModel};

use crate::object_id::{IdTag, ObjectId};
use crate::record_header::{RECORD_HEADER_LEN, encode_header};
use crate::record_kind::{ChunkCodec, RecordKind};
use crate::root_record::{ROOT_FIXED_LEN, RootRecord};
use crate::sector_header::{HeadKind, SECTOR_HEADER_LEN, SectorHeader, SectorRead};
use crate::test_support::{Store, formatted, mount, snapshot, text};
use crate::{SoftSha256, StoreConfig, StoreError, TreeStore};

const SECTORS: u32 = 16;

/// An unknown compat flag: the sector is read (its files are there) and
/// never appended to; writing goes on in other sectors.
#[test]
fn an_unknown_compat_flag_mounts_and_its_sector_takes_no_appends() {
    let (c, f, want) = small_store();
    let st = mount(f, &c);
    let cold = st.log.heads[HeadKind::Cold.index()].expect("a cold head");
    let mut f = st.into_flash();
    reseal_header(&mut f, cold, |h| h[9] |= 0x80);

    let mut st = mount(f, &c);
    assert_eq!(snapshot(&mut st), want);
    assert_ne!(st.log.heads[HeadKind::Cold.index()], Some(cold));
    assert_eq!(
        u32::from(st.log.sectors.end[cold as usize]),
        st.log.sector_size
    );
    st.put("/after.json", &text(99, 300)).unwrap();
    let mut st = mount(st.into_flash(), &c);
    assert_eq!(st.get("/after.json").unwrap().unwrap(), text(99, 300));
    assert_eq!(st.get("/a/one.json").unwrap().unwrap(), want["/a/one.json"]);
}

/// An unknown incompat flag on any good header refuses the mount — before
/// a single record is read — and a format afterwards gives a working store.
#[test]
fn an_unknown_incompat_flag_refuses_the_mount_and_format_recovers() {
    let (c, mut f, _) = small_store();
    let victim = (0..SECTORS).find(|&s| !f.sector_is_blank(s)).unwrap();
    reseal_header(&mut f, victim, |h| h[11] |= 0x01);
    let mut f = refused(f, &c, "unknown incompat flag");

    TreeStore::format(&mut f, &mut SoftSha256, &c).unwrap();
    let mut st = mount(f, &c);
    assert!(snapshot(&mut st).is_empty());
    st.put("/x.json", b"{}").unwrap();
    let mut st = mount(st.into_flash(), &c);
    assert_eq!(st.get("/x.json").unwrap().unwrap(), b"{}");
}

/// A header written for another sector size (or a head kind this version
/// does not define) refuses the mount the same way.
#[test]
fn another_sector_size_or_head_kind_refuses_the_mount() {
    let (c, f, _) = small_store();
    let victim = (0..SECTORS).find(|&s| !f.sector_is_blank(s)).unwrap();
    let mut size = f.clone();
    reseal_header(&mut size, victim, |h| h[7] = 13);
    refused(size, &c, "another sector size");
    let mut kind = f;
    reseal_header(&mut kind, victim, |h| h[6] = 2);
    refused(kind, &c, "unknown head kind");
}

/// A sector at a newer format version refuses the whole mount (a rolled-
/// back core must never see a newer core's store as blank and format it),
/// whether or not its CRC is where this version keeps it.
#[test]
fn a_sector_at_a_newer_version_refuses_the_mount() {
    let (c, f, want) = small_store();
    let victim = (0..SECTORS).find(|&s| !f.sector_is_blank(s)).unwrap();
    let mut sealed = f.clone();
    reseal_header(&mut sealed, victim, |h| h[4] = 4);
    refused(sealed, &c, "newer format");
    // A newer layout (here: everything after the version reshuffled).
    let mut moved = f.clone();
    rewrite_sector(&mut moved, victim, |h| {
        h[4] = 9;
        h[6..24].fill(0x5A);
    });
    refused(moved, &c, "newer format");
    // A blank sector a newer core opened, the rest of the store untouched.
    let blank = (0..SECTORS).find(|&s| f.sector_is_blank(s)).unwrap();
    let mut extra = f;
    let mut h = header_bytes(&extra, victim);
    h[4] = 4;
    extra.program(blank * 4096, &h).unwrap();
    let before = image(&extra);
    let back = refused(extra, &c, "newer format");
    // Refusing wrote nothing; with the newer sector gone the store is whole.
    assert_eq!(image(&back), before);
    let mut back = back;
    back.erase_sector(blank).unwrap();
    let mut st = mount(back, &c);
    assert_eq!(snapshot(&mut st), want);
}

/// A sector at an older (or never assigned lower) version is only
/// untrusted: the rest of the store mounts and writing goes on.
#[test]
fn a_sector_at_an_older_version_is_untrusted_and_the_rest_mounts() {
    let (c, mut f, want) = small_store();
    let blank = (0..SECTORS).find(|&s| f.sector_is_blank(s)).unwrap();
    let live = (0..SECTORS).find(|&s| !f.sector_is_blank(s)).unwrap();
    let mut h = header_bytes(&f, live);
    h[4] = 2;
    let crc = crc32(&h[..20]);
    h[20..24].copy_from_slice(&crc.to_le_bytes());
    f.program(blank * 4096, &h).unwrap();
    f.program(blank * 4096 + 24, b"records a version-2 writer left")
        .unwrap();

    let mut st = mount(f, &c);
    assert_eq!(snapshot(&mut st), want);
    for i in 0..40u64 {
        st.put("/after.json", &text(i, 3000)).unwrap();
    }
    let mut st = mount(st.into_flash(), &c);
    assert_eq!(st.get("/after.json").unwrap().unwrap(), text(39, 3000));
    assert_eq!(st.get("/a/one.json").unwrap().unwrap(), want["/a/one.json"]);
}

/// Power cut anywhere in a format over a live store — a torn kill of a good
/// header, a torn erase of a killed one, a torn header program — under
/// every tear model and many seeds: no sector ever reads as a newer format
/// (on any of several reads, so weak bits get their chances), and the
/// mount never refuses.
#[test]
fn a_torn_kill_erase_or_header_program_never_reads_as_newer() {
    let (c, f, _) = small_store();
    let mut probe = f.clone();
    probe.set_plan(FaultPlan::none());
    TreeStore::format(&mut probe, &mut SoftSha256, &c).unwrap();
    let ops = probe.ops_since_plan();
    let mut runs = 0u32;
    for k in 0..ops {
        for tear in TearModel::ALL {
            for seed in 0..24u64 {
                let mut g = f.clone();
                g.set_plan(FaultPlan::cut(k, tear, seed << 8 | k));
                assert!(TreeStore::format(&mut g, &mut SoftSha256, &c).is_err());
                g.power_cycle(FaultPlan::none());
                for s in 0..SECTORS {
                    for _ in 0..4 {
                        let mut h = [0u8; SECTOR_HEADER_LEN as usize];
                        g.read(s * 4096, &mut h).unwrap();
                        assert!(
                            !matches!(SectorHeader::decode(&h, 4096), SectorRead::Unsupported(_)),
                            "cut {k} {tear:?} seed {seed}: sector {s} reads {h:02x?}"
                        );
                    }
                }
                if let Err((e, _, _)) = TreeStore::mount(g, SoftSha256, c.clone()) {
                    assert!(!matches!(e, StoreError::Unsupported(_)), "cut {k}: {e:?}");
                }
                runs += 1;
            }
        }
    }
    assert!(runs > 1000, "{runs} runs");
}

/// A root whose tail carries a tag this version does not know is the
/// committed root; the next root this writer writes drops the tag.
#[test]
fn an_unknown_root_tag_is_skipped_then_dropped() {
    let (c, f, want) = small_store();
    let mut st = mount(f, &c);
    let committed = st.committed.clone().unwrap().root;
    let mut payload = RootRecord {
        seq: committed.seq + 1,
        ..committed
    }
    .encode();
    payload.extend_from_slice(&[0x7F, 2, 0, 0xAB, 0xCD]);
    let id = ObjectId::of(&mut st.hasher, IdTag::Root, &[&payload]);
    let hot = st.log.heads[HeadKind::Hot.index()].expect("a hot head");
    let at = u32::from(st.log.sectors.end[hot as usize]);
    let mut f = st.into_flash();
    program_record(&mut f, hot, at, RecordKind::Root.to_u8(), id, &payload);

    let mut st = mount(f, &c);
    let got = st.committed.clone().unwrap();
    assert_eq!((got.id, got.root.seq), (id, committed.seq + 1));
    assert_eq!(snapshot(&mut st), want);
    st.put("/b.json", b"[]").unwrap();
    let next = st.committed.clone().unwrap();
    let (_, p) = st.log.read_record(next.id).unwrap();
    assert_eq!(p.len(), ROOT_FIXED_LEN + 2 * next.root.retired.len());
    assert_eq!(next.root.seq, committed.seq + 2);
    let mut st = mount(st.into_flash(), &c);
    assert_eq!(st.get("/b.json").unwrap().unwrap(), b"[]");
}

/// A CRC-good record of an unknown kind is skipped as garbage: the sector
/// is not closed (writing resumes after it), the record is never indexed,
/// and GC can collect its sector.
#[test]
fn an_unknown_record_kind_is_skipped() {
    let (c, f, want) = small_store();
    let st = mount(f, &c);
    let cold = st.log.heads[HeadKind::Cold.index()].expect("a cold head");
    let at = u32::from(st.log.sectors.end[cold as usize]);
    let mut f = st.into_flash();
    let future = ObjectId(0x5EED);
    program_record(&mut f, cold, at, 9, future, b"a record from later");
    let past = at + RECORD_HEADER_LEN + 19;

    let mut st = mount(f, &c);
    assert_eq!(st.log.heads[HeadKind::Cold.index()], Some(cold));
    assert_eq!(u32::from(st.log.sectors.end[cold as usize]), past);
    assert!(!st.log.index.contains(future));
    assert_eq!(snapshot(&mut st), want);
    st.put("/c.json", &text(5, 200)).unwrap();
    let blob = st.lookup("/c.json").unwrap().unwrap().id;
    let loc = st.log.index.get(blob).unwrap();
    assert_eq!((loc.sector, loc.offset), (cold, past));

    let mut st = mount(st.into_flash(), &c);
    assert!(!st.log.index.contains(future));
    assert_eq!(st.get("/c.json").unwrap().unwrap(), text(5, 200));
    // Make the sector garbage, then churn until GC has erased it.
    st.delete_prefix("/a/").unwrap();
    st.delete("/c.json").unwrap();
    let erases = st.log.sectors.erase_count[cold as usize];
    for round in 0..400u64 {
        st.put("/churn.json", &text(round, 3000)).unwrap();
        if st.log.sectors.erase_count[cold as usize] > erases {
            break;
        }
    }
    assert!(st.log.sectors.erase_count[cold as usize] > erases);
    let f = mount(st.into_flash(), &c).into_flash();
    let mut cells = alloc::vec![0u8; 4096];
    f.peek(cold * 4096, &mut cells);
    assert!(!cells.windows(19).any(|w| w == b"a record from later"));
    let mut st = mount(f, &c);
    assert_eq!(st.list("/").unwrap(), ["/churn.json"]);
}

/// A formatted store with a few files (cold, multi-part, hot).
fn small_store() -> (StoreConfig, NorFlashSim, crate::test_support::State) {
    let c = StoreConfig::default();
    let mut st: Store = mount(formatted(NorGeometry::c6(SECTORS), &c), &c);
    st.put("/a/one.json", &text(1, 400)).unwrap();
    st.put("/a/big.bin", &text(2, 5000)).unwrap();
    st.put("/a/.lp/panel.json", &text(3, 120)).unwrap();
    let want = snapshot(&mut st);
    (c, st.into_flash(), want)
}

/// The mount must fail with `Unsupported(why)`; the flash comes back.
fn refused(f: NorFlashSim, c: &StoreConfig, why: &'static str) -> NorFlashSim {
    match TreeStore::mount(f, SoftSha256, c.clone()) {
        Ok(_) => panic!("mounted flash it must refuse ({why})"),
        Err((e, f, _)) => {
            assert_eq!(e, StoreError::Unsupported(why));
            f
        }
    }
}

/// Rewrite sector `s`'s header with `edit` applied and its CRC recomputed
/// (as a newer writer would have programmed it), keeping its records.
fn reseal_header(f: &mut NorFlashSim, s: u32, edit: impl FnOnce(&mut [u8])) {
    let size = f.geometry().sector_size;
    let mut cells = alloc::vec![0u8; size as usize];
    f.peek(s * size, &mut cells);
    let h = &mut cells[..SECTOR_HEADER_LEN as usize];
    edit(h);
    let crc = crc32(&h[..20]);
    h[20..24].copy_from_slice(&crc.to_le_bytes());
    f.erase_sector(s).unwrap();
    let end = cells.iter().rposition(|&b| b != 0xFF).map_or(0, |p| p + 1);
    f.program(s * size, &cells[..end]).unwrap();
}

/// Sector `s`'s 24 header bytes.
fn header_bytes(f: &NorFlashSim, s: u32) -> [u8; SECTOR_HEADER_LEN as usize] {
    let mut h = [0u8; SECTOR_HEADER_LEN as usize];
    f.peek(s * f.geometry().sector_size, &mut h);
    h
}

/// Every cell of the flash.
fn image(f: &NorFlashSim) -> Vec<u8> {
    let mut v = alloc::vec![0u8; (SECTORS * f.geometry().sector_size) as usize];
    f.peek(0, &mut v);
    v
}

/// Rewrite sector `s`'s header with `edit` applied and nothing resealed.
fn rewrite_sector(f: &mut NorFlashSim, s: u32, edit: impl FnOnce(&mut [u8])) {
    let size = f.geometry().sector_size;
    let mut cells = alloc::vec![0u8; size as usize];
    f.peek(s * size, &mut cells);
    edit(&mut cells[..SECTOR_HEADER_LEN as usize]);
    f.erase_sector(s).unwrap();
    let end = cells.iter().rposition(|&b| b != 0xFF).map_or(0, |p| p + 1);
    f.program(s * size, &cells[..end]).unwrap();
}

/// Program one CRC-good record (any kind byte) at `(s, at)`.
fn program_record(f: &mut NorFlashSim, s: u32, at: u32, kind: u8, id: ObjectId, payload: &[u8]) {
    let mut h = encode_header(RecordKind::Blob, ChunkCodec::Stored, id, &[payload]);
    h[0] = kind;
    let mut crc_in: Vec<u8> = h[..12].to_vec();
    crc_in.extend_from_slice(payload);
    h[12..16].copy_from_slice(&crc32(&crc_in).to_le_bytes());
    let size = f.geometry().sector_size;
    f.program(s * size + at, &h).unwrap();
    f.program(s * size + at + RECORD_HEADER_LEN, payload)
        .unwrap();
}
