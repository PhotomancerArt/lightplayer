//! Mount's verdicts on whole flashes (`mount_verdict.rs`): what is
//! `NoStore` (format it), what is `Damaged` or `Unsupported` (keep it), the
//! proof that a cut anywhere in a first `format` leaves `NoStore`, the
//! firmware's mount-then-format flow on one store type, and the summary.

extern crate std;

use alloc::vec::Vec;

use lp_nor_sim::{FaultPlan, NorError, NorFlashSim, NorGeometry, TearModel};

use crate::sector_header::{HeadKind, SECTOR_HEADER_LEN, SectorHeader, SectorRead};
use crate::test_support::{Store, formatted, mount, snapshot, text};
use crate::{MountSummary, SoftSha256, StoreConfig, StoreError, TreeStore};

const SECTORS: u32 = 16;

#[test]
fn blank_flash_is_no_store() {
    assert_eq!(verdict(NorFlashSim::new(geom())), Err(StoreError::NoStore));
}

#[test]
fn garbage_is_no_store() {
    for seed in 0..8 {
        let v = verdict(NorFlashSim::garbage(geom(), seed));
        assert_eq!(v, Err(StoreError::NoStore), "seed {seed}");
    }
}

#[test]
fn a_littlefs_partition_is_no_store() {
    assert_eq!(verdict(littlefs_shaped()), Err(StoreError::NoStore));
}

#[test]
fn a_valid_store_mounts() {
    let (c, f) = files_store();
    assert!(TreeStore::mount(f, SoftSha256, c).is_ok());
}

#[test]
fn file_records_without_a_root_are_damaged() {
    // Every hot sector (the roots, the panel file) erased: the cold
    // sectors' files and directories stay, and no root names them.
    let (_, mut f) = files_store();
    for s in sectors_of(&f, HeadKind::Hot) {
        f.erase_sector(s).unwrap();
    }
    assert_eq!(verdict(f), Err(StoreError::Damaged("no complete root")));
}

#[test]
fn a_root_whose_tree_is_gone_is_damaged() {
    // Every cold sector erased: the roots stay and their closure fails.
    let (_, mut f) = files_store();
    for s in sectors_of(&f, HeadKind::Cold) {
        f.erase_sector(s).unwrap();
    }
    assert_eq!(verdict(f), Err(StoreError::Damaged("no complete root")));
}

#[test]
fn a_newer_header_is_unsupported() {
    let (c, mut f) = files_store();
    let s = sectors_of(&f, HeadKind::Cold)[0];
    let mut cells = alloc::vec![0u8; 4096];
    f.peek(s * 4096, &mut cells);
    // A newer writer's header: the magic and version 4, the rest unread.
    cells[4..6].copy_from_slice(&4u16.to_le_bytes());
    f.erase_sector(s).unwrap();
    f.program(s * 4096, &cells).unwrap();
    let Err((e, _, _)) = TreeStore::mount(f, SoftSha256, c) else {
        panic!("mounted over a newer header");
    };
    assert_eq!(e, StoreError::Unsupported("newer format"));
}

/// D2 (b): a power cut anywhere in a first `format` — over blank flash, a
/// littlefs partition, garbage — under every named tear model, leaves a
/// flash the next mount calls `NoStore` (or, after the root landed, a
/// mounted empty store), never `Damaged` or `Unsupported`; a second format
/// then gives a working store.
#[test]
fn a_cut_anywhere_in_a_first_format_is_no_store() {
    let c = cfg();
    let starts = [
        NorFlashSim::new(geom()),
        littlefs_shaped(),
        NorFlashSim::garbage(geom(), 7),
    ];
    let mut runs = 0u32;
    let mut no_store = 0u32;
    for (si, start) in starts.iter().enumerate() {
        let mut probe = start.clone();
        probe.set_plan(FaultPlan::none());
        let ops = {
            let st = TreeStore::format(&mut probe, SoftSha256, c.clone())
                .map_err(|(e, ..)| e)
                .unwrap();
            st.flash().ops_since_plan()
        };
        // Lazy: two erases, whatever the flash held.
        assert_eq!(erases(&probe), 2, "start {si}");
        for k in 0..ops {
            for tear in TearModel::NAMED {
                for seed in 0..3u64 {
                    let mut g = start.clone();
                    g.set_plan(FaultPlan::cut(k, tear, seed << 16 | k));
                    assert!(TreeStore::format(&mut g, SoftSha256, c.clone()).is_err());
                    g.power_cycle(FaultPlan::none());
                    let ctx = || std::format!("start {si} cut {k}/{ops} {tear:?} seed {seed}");
                    let g = match TreeStore::mount(g, SoftSha256, c.clone()) {
                        Ok(mut st) => {
                            assert!(snapshot(&mut st).is_empty(), "{}", ctx());
                            st.into_flash()
                        }
                        Err((StoreError::NoStore, g, _)) => {
                            no_store += 1;
                            g
                        }
                        Err((e, ..)) => panic!("{}: {e:?}", ctx()),
                    };
                    let mut st = match TreeStore::format(g, SoftSha256, c.clone()) {
                        Ok(st) => st,
                        Err((e, ..)) => panic!("{}: reformat: {e:?}", ctx()),
                    };
                    st.put("/x.json", b"{}").unwrap();
                    let mut st = mount(st.into_flash(), &c);
                    assert_eq!(st.get("/x.json").unwrap().unwrap(), b"{}", "{}", ctx());
                    runs += 1;
                }
            }
        }
    }
    std::println!("{runs} cut formats, {no_store} NoStore");
    assert!(
        runs > 1000 && no_store > runs / 2,
        "{runs} runs, {no_store} NoStore"
    );
}

/// A format over a store with files and many roots (each with a higher
/// sequence than the root `format` writes): two sectors erased, every other
/// header killed (its erase count left readable), so the mount sees the empty store and nothing of the old
/// one, the old roots included; writing goes on over the killed sectors.
#[test]
fn a_format_over_a_store_kills_every_old_header() {
    let (c, f) = files_store();
    let old_seq = mount(f.clone(), &c).summary().root_seq;
    assert!(old_seq > 1);
    let mut f = f;
    f.reset_stats();
    let st = TreeStore::format(&mut f, SoftSha256, c.clone())
        .map_err(|(e, ..)| e)
        .unwrap();
    assert_eq!(st.summary().root_seq, 1);
    drop(st);
    assert_eq!(erases(&f), 2);
    for s in 2..SECTORS {
        let mut h = [0u8; SECTOR_HEADER_LEN as usize];
        f.peek(s * 4096, &mut h);
        let killed = crate::sector_header::reads_killed(&h);
        assert!(killed || h == [0xFF; 24], "sector {s} header {h:02x?}");
        // A kill leaves bytes 4..20: the erase count survives the format.
        if killed {
            assert!(u32::from_le_bytes(h[16..20].try_into().unwrap()) >= 1);
        }
    }
    let mut st = mount(f, &c);
    assert!(snapshot(&mut st).is_empty());
    assert_eq!(st.summary().root_seq, 1);
    for i in 0..40u64 {
        st.put("/after.json", &text(i, 3000)).unwrap();
    }
    let mut st = mount(st.into_flash(), &c);
    assert_eq!(st.list("/").unwrap(), ["/after.json"]);
    assert_eq!(st.get("/after.json").unwrap().unwrap(), text(39, 3000));
}

/// A format erases littlefs's superblock pair (blocks 0 and 1), so neither a
/// littlefs core nor the host's probe can find the old files.
#[test]
fn a_format_erases_the_littlefs_superblocks() {
    let mut f = littlefs_shaped();
    TreeStore::format(&mut f, SoftSha256, cfg())
        .map_err(|(e, ..)| e)
        .unwrap();
    for block in 0..2u32 {
        let mut b = [0u8; 4096];
        f.peek(block * 4096, &mut b);
        assert!(
            !b.windows(8).any(|w| w == b"littlefs"),
            "block {block} still names littlefs"
        );
    }
}

/// A cut anywhere in a format over a store with files, then a second
/// format: the store is empty and takes writes. (What the cut leaves before
/// that is the old store, `Damaged` or `NoStore` — a firmware formats only
/// on `NoStore`, so it never runs this — but never a newer format:
/// `format_extension_tests`.)
#[test]
fn a_cut_format_over_a_store_formats_again() {
    let (c, f) = files_store();
    let mut probe = f.clone();
    probe.set_plan(FaultPlan::none());
    TreeStore::format(&mut probe, SoftSha256, c.clone())
        .map_err(|(e, ..)| e)
        .unwrap();
    let ops = probe.ops_since_plan();
    for k in 0..ops {
        for tear in TearModel::NAMED {
            for seed in 0..3u64 {
                let mut g = f.clone();
                g.set_plan(FaultPlan::cut(k, tear, seed << 16 | k));
                assert!(TreeStore::format(&mut g, SoftSha256, c.clone()).is_err());
                g.power_cycle(FaultPlan::none());
                let ctx = || std::format!("cut {k}/{ops} {tear:?} seed {seed}");
                let mut st = match TreeStore::format(g, SoftSha256, c.clone()) {
                    Ok(st) => st,
                    Err((e, ..)) => panic!("{}: reformat: {e:?}", ctx()),
                };
                st.put("/x.json", b"{}").unwrap();
                let mut st = mount(st.into_flash(), &c);
                assert_eq!(st.list("/").unwrap(), ["/x.json"], "{}", ctx());
            }
        }
    }
}

/// The firmware's boot: mount by value, and on `NoStore` format by value
/// what the mount handed back — one `TreeStore<F, H>` type throughout.
#[test]
fn mount_then_format_is_one_store_type() {
    let c = cfg();
    let boot = |f: NorFlashSim| -> Store {
        match TreeStore::mount(f, SoftSha256, c.clone()) {
            Ok(st) => st,
            Err((StoreError::NoStore, f, h)) => match TreeStore::format(f, h, c.clone()) {
                Ok(st) => st,
                Err((e, ..)) => panic!("format: {e:?}"),
            },
            Err((e, ..)) => panic!("mount: {e:?}"),
        }
    };
    let mut st = boot(NorFlashSim::new(geom()));
    st.put("/a.json", &text(1, 300)).unwrap();
    let mut st = boot(st.into_flash());
    assert_eq!(st.get("/a.json").unwrap().unwrap(), text(1, 300));
}

#[test]
fn the_summary_after_format_commit_and_reopen() {
    let c = cfg();
    let st = TreeStore::format(NorFlashSim::new(geom()), SoftSha256, c.clone())
        .map_err(|(e, ..)| e)
        .unwrap();
    let after_format = st.summary();
    assert_eq!(after_format.sectors, SECTORS);
    assert_eq!(after_format.root_seq, 1);
    // Two heads: the empty directory's cold sector and the root's hot one.
    assert_eq!(after_format.free_sectors, SECTORS - 2);
    let mut st = st;
    st.put("/a.json", &text(1, 300)).unwrap();
    let after_commit = st.summary();
    assert_eq!(after_commit.root_seq, 2);
    let st = mount(st.into_flash(), &c);
    assert_eq!(
        st.summary(),
        MountSummary {
            sectors: SECTORS,
            free_sectors: after_commit.free_sectors,
            root_seq: 2,
        }
    );
}

// ---- helpers ---------------------------------------------------------------

fn geom() -> NorGeometry {
    NorGeometry::c6(SECTORS)
}

fn cfg() -> StoreConfig {
    StoreConfig::default()
}

/// Erases the simulator counted.
fn erases(f: &NorFlashSim) -> u64 {
    f.stats().erases_total()
}

/// The mount's error, or `Ok` when it mounted.
fn verdict(f: NorFlashSim) -> Result<(), StoreError<NorError>> {
    TreeStore::mount(f, SoftSha256, cfg())
        .map(drop)
        .map_err(|(e, ..)| e)
}

/// A store holding cold files, a hot panel file and several roots.
fn files_store() -> (StoreConfig, NorFlashSim) {
    let c = cfg();
    let mut st = mount(formatted(geom(), &c), &c);
    st.put("/projects/a/project.json", &text(1, 400)).unwrap();
    st.put("/projects/a/big.bin", &text(2, 6000)).unwrap();
    st.put("/projects/a/.lp/panel.json", &text(3, 120)).unwrap();
    (c, st.into_flash())
}

/// The sectors whose trusted header names `kind`.
fn sectors_of(f: &NorFlashSim, kind: HeadKind) -> Vec<u32> {
    let out: Vec<u32> = (0..SECTORS)
        .filter(|&s| {
            let mut h = [0u8; SECTOR_HEADER_LEN as usize];
            f.peek(s * 4096, &mut h);
            matches!(
                SectorHeader::decode(&h, 4096),
                SectorRead::Trusted { header, .. } if header.kind == kind
            )
        })
        .collect();
    assert!(!out.is_empty(), "no {kind:?} sector");
    out
}

/// Two littlefs metadata blocks as a fresh littlefs format leaves them: a
/// revision count, a tag, the `littlefs` magic, a superblock's fields, the
/// rest erased.
fn littlefs_shaped() -> NorFlashSim {
    let mut f = NorFlashSim::new(geom());
    for block in 0..2u32 {
        let mut b = Vec::new();
        b.extend_from_slice(&(block + 1).to_le_bytes());
        b.extend_from_slice(&[0x0f, 0xff, 0xf0, 0x08]);
        b.extend_from_slice(b"littlefs");
        b.extend_from_slice(&[0x2f, 0xe0, 0x00, 0x10]);
        b.extend_from_slice(&0x0002_0001u32.to_le_bytes());
        b.extend_from_slice(&4096u32.to_le_bytes());
        b.extend_from_slice(&SECTORS.to_le_bytes());
        b.extend_from_slice(&[0x5a; 20]);
        f.program(block * 4096, &b).unwrap();
    }
    f
}
