//! Counter wrap (M3 P2): the store started with its sector sequence or its
//! root sequence a few steps below the maximum, then walked past the wrap
//! (with cuts: `test_support::sweep_from`). The counters are set on a
//! mounted store; mount derives both from what is on flash, so every later
//! mount carries them on.

use alloc::boxed::Box;
use alloc::format;
use alloc::vec::Vec;

use lp_nor_sim::{NorFlashSim, NorGeometry};

extern crate std;

use crate::StoreConfig;
use crate::sector_header::{SECTOR_HEADER_LEN, SectorHeader, SectorRead};
use crate::test_support::{Step, formatted, mount, noise, snapshot, sweep_from, text};

/// The sector sequence (u32) wraps safely: after the wrap the newest
/// sectors carry seqs below older ones (and below the format's own), so
/// "newest sector first" picks among identical copies of a record, the head
/// a mount resumes may be an older sector (only over an all-`0xFF` tail), and
/// cost-benefit ages read young — none of which the closure, the old-or-new
/// rule or a re-run notices. (Unreachable in practice: 2^32 sector opens is
/// ~330× a 128-sector flash's 100k-cycle endurance.)
#[test]
fn the_sector_sequence_wraps_under_cuts() {
    let c = StoreConfig::default();
    let flash = store_near(u32::MAX - 8, 0, &c);
    let r = sweep_from(flash.clone(), &c, &churn(12), 8);
    std::println!("{}: {r:?}", line!());
    assert!(r.gc_runs > 0 && r.cuts > 200, "{r:?}");
    // The sweep's steps did carry the counter past the wrap.
    let mut st = mount(flash, &c);
    for s in churn(12) {
        s(&mut st).unwrap();
    }
    let seqs = sector_seqs(st.flash());
    assert!(
        seqs.iter().any(|&q| q > u32::MAX - 16) && seqs.iter().any(|&q| (3..64).contains(&q)),
        "{seqs:?}"
    );
}

/// **Pins a known limit**
/// (`docs/defects/2026-10-08-tree-store-root-sequence-does-not-wrap.md`):
/// the root sequence (u64) does not wrap. A root written past `u64::MAX`
/// carries seq 0, mount keeps the highest seq, and the store comes back at
/// the last commit before the wrap — a committed write lost. Unreachable in
/// practice (2^64 commits) and a format question for G2; this test fails
/// once the format orders roots across a wrap — replace it then.
#[test]
fn the_root_sequence_does_not_wrap() {
    let c = StoreConfig::default();
    let mut st = mount(store_near(1000, u64::MAX - 8, &c), &c);
    st.put("/before.json", &text(1, 200)).unwrap();
    assert_eq!(st.max_root_seq, u64::MAX);
    let before = snapshot(&mut st);
    st.put("/after.json", &text(2, 200)).unwrap();
    assert_eq!(st.max_root_seq, 0);
    let committed = snapshot(&mut st);
    let mut st = mount(st.into_flash(), &c);
    let got = snapshot(&mut st);
    assert_ne!(got, committed, "the format now wraps: replace this pin");
    assert_eq!(got, before);
}

/// A 10-sector store whose next sector seq is `sector_seq` and whose last
/// root seq is `root_seq` plus the seven commits of its churn (enough that
/// most sectors were opened again under the new sector counter).
fn store_near(sector_seq: u32, root_seq: u64, cfg: &StoreConfig) -> NorFlashSim {
    let mut st = mount(formatted(NorGeometry::c6(10), cfg), cfg);
    st.log.next_sector_seq = sector_seq;
    st.max_root_seq = root_seq;
    st.put("/keep.json", &text(1, 2500)).unwrap();
    for i in 0..6u64 {
        st.put("/churn.bin", &noise(i, 3000)).unwrap();
    }
    st.into_flash()
}

/// Churn that opens sectors and runs GC on 10 sectors.
fn churn(n: u64) -> Vec<Step> {
    (0..n)
        .map(|i| {
            Box::new(move |st: &mut crate::test_support::Store| {
                st.begin()?;
                st.put(&format!("/log/{}.json", i % 4), &text(60 + i, 300))?;
                st.put("/churn-a.bin", &noise(30 + i, 3000))?;
                st.put("/projects/a/.lp/panel.json", &text(50 + i, 110))?;
                st.commit()
            }) as Step
        })
        .collect()
}

/// Each trusted sector's seq.
fn sector_seqs(f: &NorFlashSim) -> Vec<u32> {
    let size = f.geometry().sector_size;
    (0..f.geometry().sector_count)
        .filter_map(|s| {
            let mut h = [0u8; SECTOR_HEADER_LEN as usize];
            f.peek(s * size, &mut h);
            match SectorHeader::decode(&h, size) {
                SectorRead::Trusted { header, .. } => Some(header.seq),
                _ => None,
            }
        })
        .collect()
}
