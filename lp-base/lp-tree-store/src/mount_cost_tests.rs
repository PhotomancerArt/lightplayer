//! What a mount reads, on the stores a board holds: empty, a c40 pushed
//! once, a partition written out by use (mostly garbage and GC copies), and
//! one full of live files — at 128 and 176 sectors. Counted three ways: the
//! store's own reads (calls and bytes), the reads the C6 makes of them
//! through `fw-esp32-common`'s `AlignedNorFlash` (a 256-B bounce buffer:
//! calls of at most 256 bytes, word-aligned), and the bytes mount CRCs.
//! Simulator counts (lp-nor-sim), not silicon or emulated time.
//!
//! `LP_TREE_MOUNT_IMAGES=<dir>` also mounts every raw `lpfs` image in that
//! directory (`lp-cli hardware lpfs save`'s `.bin`): the soak's written-out
//! partition is the real "used" case.

extern crate std;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use std::cell::Cell;

use lp_nor_sim::{NorError, NorFlashSim, NorGeometry};

use crate::ram_budget_tests::c40_like;
use crate::test_support::{formatted, mount, noise, text};
use crate::{Flash, SoftSha256, StoreConfig, StoreError, TreeStore};

std::thread_local! {
    /// Bytes the CRC folded in on this thread (`record_header::crc_ok`).
    pub static CRC_BYTES: Cell<u64> = const { Cell::new(0) };
    /// Store reads (calls, bytes) at the end of mount's first pass.
    pub static PASS1: Cell<(u64, u64)> = const { Cell::new((0, 0)) };
}

/// The C6 shim's bounce buffer (`fw_esp32_common::aligned_nor_flash::BOUNCE`).
const BOUNCE: u32 = 256;

/// A flash that counts what reaches it.
pub struct Counting {
    pub inner: NorFlashSim,
    pub calls: u64,
    pub bytes: u64,
    /// Calls and bytes the C6's `AlignedNorFlash` would make of them.
    pub c6_calls: u64,
    pub c6_bytes: u64,
}

impl Counting {
    fn new(inner: NorFlashSim) -> Self {
        Self {
            inner,
            calls: 0,
            bytes: 0,
            c6_calls: 0,
            c6_bytes: 0,
        }
    }
}

impl Flash for Counting {
    type Error = NorError;
    fn sector_count(&self) -> u32 {
        self.inner.geometry().sector_count
    }
    fn sector_size(&self) -> u32 {
        self.inner.geometry().sector_size
    }
    fn read(&mut self, addr: u32, buf: &mut [u8]) -> Result<(), NorError> {
        self.calls += 1;
        self.bytes += buf.len() as u64;
        if addr % 4 == 0 && buf.len() % 4 == 0 && buf.as_ptr().align_offset(4) == 0 {
            self.c6_calls += 1;
            self.c6_bytes += buf.len() as u64;
            return self.inner.read(addr, buf);
        }
        let (mut at, mut left) = (addr, buf.len() as u32);
        while left > 0 {
            let skip = at % 4;
            let want = left.min(BOUNCE - skip);
            self.c6_calls += 1;
            self.c6_bytes += u64::from((skip + want).next_multiple_of(4));
            at += want;
            left -= want;
        }
        self.inner.read(addr, buf)
    }
    fn program(&mut self, addr: u32, data: &[u8]) -> Result<(), NorError> {
        self.inner.program(addr, data)
    }
    fn erase_sector(&mut self, sector: u32) -> Result<(), NorError> {
        self.inner.erase_sector(sector)
    }
}

/// One mount's cost.
#[derive(Clone, Copy, Debug)]
pub struct MountCost {
    pub calls: u64,
    pub bytes: u64,
    pub c6_calls: u64,
    pub c6_bytes: u64,
    pub crc_bytes: u64,
    pub pass1_calls: u64,
    pub pass1_bytes: u64,
    pub scans: u32,
    pub peak: usize,
}

pub fn mount_cost(flash: NorFlashSim, cfg: &StoreConfig) -> (MountCost, NorFlashSim) {
    CRC_BYTES.with(|c| c.set(0));
    let (st, peak, _) = crate::test_support::heap_use(|| {
        match TreeStore::mount(Counting::new(flash), SoftSha256, cfg.clone()) {
            Ok(s) => s,
            Err((e, _, _)) => panic!("mount: {e:?}"),
        }
    });
    let scans = st.stats().mount_scans;
    let f = st.into_flash();
    let cost = MountCost {
        calls: f.calls,
        bytes: f.bytes,
        c6_calls: f.c6_calls,
        c6_bytes: f.c6_bytes,
        crc_bytes: CRC_BYTES.with(Cell::get),
        pass1_calls: PASS1.with(Cell::get).0,
        pass1_bytes: PASS1.with(Cell::get).1,
        scans,
        peak,
    };
    (cost, f.inner)
}

/// c40 as a push sends it: every file but the panel host-deflated.
fn push_c40(st: &mut crate::test_support::Store, c: &StoreConfig, root: &str) {
    st.begin().unwrap();
    for (p, b) in &c40_like() {
        let p = p.replacen("/projects/a", root, 1);
        if crate::is_hot(&p) {
            st.put(&p, b).unwrap();
            continue;
        }
        let mut off = 0;
        for ch in crate::host_deflate_chunks(&mut SoftSha256, b, c.record_max) {
            st.put_chunk_deflated(&p, off, ch.logical_len, Some(ch.id), &ch.deflated)
                .unwrap();
            off += ch.logical_len;
        }
    }
    st.commit().unwrap();
}

/// A small project (the soak's `basic`): eight documents.
fn push_basic(st: &mut crate::test_support::Store, root: &str, seed: u64) {
    st.begin().unwrap();
    for i in 0..8u64 {
        st.put(
            &format!("{root}/doc{i}.json"),
            &text(seed + i, 300 + 200 * i as usize),
        )
        .unwrap();
    }
    st.commit().unwrap();
}

/// The stores, by name.
pub fn scenarios(sectors: u32) -> Vec<(String, NorFlashSim)> {
    let c = cost_cfg();
    let geom = NorGeometry::c6(sectors);
    let mut out = Vec::new();
    out.push((String::from("empty"), formatted(geom, &c)));

    let mut st = mount(formatted(geom, &c), &c);
    push_c40(&mut st, &c, "/projects/a");
    let c40 = st.into_flash();
    out.push((String::from("c40"), c40.clone()));

    // Used: what the soak did — pushes of a small project and of c40 into
    // two slots, saves and panel writes, until the partition has been
    // written over three times; the live set ends small.
    let mut st = mount(formatted(geom, &c), &c);
    push_basic(&mut st, "/projects/a", 0);
    let mut round = 0u64;
    while st.stats().erases < 3 * u64::from(sectors) {
        match round % 40 {
            0 => {
                st.delete_prefix("/projects/b/").unwrap();
                push_c40(&mut st, &c, "/projects/b");
            }
            20 => {
                st.delete_prefix("/projects/b/").unwrap();
                push_basic(&mut st, "/projects/b", round);
            }
            r if r % 3 == 0 => {
                st.put(
                    &format!("/projects/a/doc{}.json", round % 8),
                    &text(round, 1700),
                )
                .unwrap();
            }
            _ => {
                st.put("/projects/a/.lp/panel.json", &text(round, 450))
                    .unwrap();
            }
        }
        round += 1;
    }
    out.push((String::from("used"), st.into_flash()));

    // Full: c40, then 4 KiB files until the store refuses.
    let mut st = mount(c40, &c);
    for i in 0.. {
        match st.put(&format!("/fill/f{i:03}.bin"), &noise(i, 4096)) {
            Ok(()) => {}
            Err(StoreError::NoSpace) => break,
            Err(e) => panic!("fill: {e:?}"),
        }
    }
    out.push((String::from("full"), st.into_flash()));
    out
}

/// Raw images from `LP_TREE_MOUNT_IMAGES`.
fn images() -> Vec<(String, NorFlashSim)> {
    let Ok(dir) = std::env::var("LP_TREE_MOUNT_IMAGES") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .expect("LP_TREE_MOUNT_IMAGES")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "bin"))
        .collect();
    paths.sort();
    for p in paths {
        let bytes = std::fs::read(&p).unwrap();
        let sectors = (bytes.len() / 4096) as u32;
        let mut f = NorFlashSim::new(NorGeometry::c6(sectors));
        f.program(0, &bytes).unwrap();
        f.reset_stats();
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        out.push((format!("image {name}"), f));
    }
    out
}

#[test]
fn mount_cost_report() {
    let c = cost_cfg();
    std::println!(
        "{:<36} {:>7} {:>9} {:>8} {:>9} {:>9} {:>7} {:>9} {:>5} {:>7}",
        "store / sectors",
        "calls",
        "bytes",
        "c6 calls",
        "c6 bytes",
        "crc B",
        "p1 calls",
        "p1 bytes",
        "scans",
        "peak"
    );
    let mut rows = Vec::new();
    for sectors in [128u32, 176] {
        for (name, f) in scenarios(sectors) {
            rows.push((format!("{name} / {sectors}"), f));
        }
    }
    rows.extend(images());
    for (name, f) in rows {
        let (m, _) = mount_cost(f, &c);
        std::println!(
            "{:<36} {:>7} {:>9} {:>8} {:>9} {:>9} {:>7} {:>9} {:>5} {:>7}",
            name,
            m.calls,
            m.bytes,
            m.c6_calls,
            m.c6_bytes,
            m.crc_bytes,
            m.pass1_calls,
            m.pass1_bytes,
            m.scans,
            m.peak
        );
    }
}

/// `LP_TREE_WRITE_IMAGES=<dir>`: write each scenario at 176 sectors (the
/// C6's `lpfs`) as a raw image, for a board to mount (`espflash write-bin
/// 0x350000`).
#[test]
fn write_scenario_images() {
    let Ok(dir) = std::env::var("LP_TREE_WRITE_IMAGES") else {
        return;
    };
    for (name, f) in scenarios(176) {
        let g = f.geometry();
        let mut bytes = alloc::vec![0u8; (g.sector_count * g.sector_size) as usize];
        f.peek(0, &mut bytes);
        std::fs::write(std::format!("{dir}/{name}-176.bin"), &bytes).unwrap();
    }
}

/// The stores' dials: the defaults (with `--features checkpoint`, the
/// prototype's checkpoint every `LP_TREE_STORE_CHECKPOINT_EVERY` records,
/// default 256).
fn cost_cfg() -> StoreConfig {
    StoreConfig {
        #[cfg(feature = "checkpoint")]
        checkpoint_every: crate::test_support::dial("LP_TREE_STORE_CHECKPOINT_EVERY", 256) as u32,
        ..StoreConfig::default()
    }
}
