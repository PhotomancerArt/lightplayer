//! PROTOTYPE (`checkpoint`): a mount by checkpoint adopts exactly what the
//! readings without one adopt — the same index, live bytes, summary and
//! files — on a store with churn, GC copies, multi-part files and a big
//! directory, at several checkpoint intervals.

extern crate std;

use alloc::format;
use alloc::vec::Vec;
use std::cell::Cell;

use lp_nor_sim::NorGeometry;

use crate::StoreConfig;
use crate::test_support::{Store, formatted, mount, noise, snapshot, text};

std::thread_local! {
    /// Mount on this thread reads as if no root carried a checkpoint.
    pub static IGNORE_CHECKPOINT: Cell<bool> = const { Cell::new(false) };
}

/// Mounts that adopted their root by checkpoint, process-wide (the sweeps
/// run on several threads): what says the cut sweeps exercised the path.
pub static MOUNTS_BY_CHECKPOINT: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// The power-cut sweep of transactions, with a checkpoint
/// every few records: old or new after every cut, and most of its mounts
/// are by checkpoint (the sweep suite runs every workload this way under
/// `--features checkpoint`; this asserts the path is taken).
#[test]
fn cuts_mount_by_checkpoint() {
    use crate::test_support::sweep;
    use alloc::boxed::Box;
    let before = MOUNTS_BY_CHECKPOINT.load(core::sync::atomic::Ordering::SeqCst);
    let c = StoreConfig {
        record_max: 256,
        checkpoint_every: 4,
        ..StoreConfig::default()
    };
    let steps: Vec<crate::test_support::Step> = (0..24u64)
        .map(|i| {
            Box::new(move |st: &mut Store| {
                st.begin()?;
                st.put(&format!("/f{}.bin", i % 3), &noise(i, 4000))?;
                st.put("/p/.lp/panel.json", &text(i, 300))?;
                st.commit()
            }) as crate::test_support::Step
        })
        .collect();
    let r = sweep(NorGeometry::c6(14), &c, &steps, 24);
    std::println!("{r:?}");
    let by = MOUNTS_BY_CHECKPOINT.load(core::sync::atomic::Ordering::SeqCst) - before;
    assert!(by > r.cuts, "{by} mounts by checkpoint, {r:?}");
}

fn index_and_live(st: &Store) -> (Vec<(u64, u32, u32)>, Vec<u16>) {
    let ix = &st.log.index;
    let entries = (0..ix.len())
        .map(|i| (ix.id_at(i).0, ix.loc_at(i).sector, ix.loc_at(i).offset))
        .collect();
    (entries, st.log.sectors.live.clone())
}

#[test]
fn a_mount_by_checkpoint_adopts_what_the_whole_reading_adopts() {
    for every in [1u32, 16, 64, 1000] {
        let c = StoreConfig {
            record_max: 256,
            checkpoint_every: every,
            ..StoreConfig::default()
        };
        let mut st = mount(formatted(NorGeometry::c6(32), &c), &c);
        st.put("/big.bin", &noise(1, 30_000)).unwrap();
        st.begin().unwrap();
        for i in 0..160u64 {
            st.put(
                &format!("/d/a-file-with-a-long-name-{i:03}.json"),
                &text(i, 40),
            )
            .unwrap();
        }
        st.commit().unwrap();
        for round in 0..60u64 {
            st.put("/churn.bin", &noise(round, 6000)).unwrap();
            st.put(&format!("/keep/k{round:02}.json"), &text(round, 200))
                .unwrap();
            st.put("/p/.lp/panel.json", &text(round, 300)).unwrap();
        }
        assert!(st.stats().gc_copies > 5, "{:?}", st.stats());
        let want = snapshot(&mut st);
        let f = st.into_flash();

        let mut a = mount(f.clone(), &c);
        assert!(
            a.stats().mount_by_checkpoint,
            "every {every}: {:?}",
            a.stats()
        );
        IGNORE_CHECKPOINT.with(|i| i.set(true));
        let mut b = mount(f, &c);
        IGNORE_CHECKPOINT.with(|i| i.set(false));
        assert!(!b.stats().mount_by_checkpoint);
        assert_eq!(index_and_live(&a), index_and_live(&b), "every {every}");
        assert_eq!(a.summary(), b.summary());
        assert_eq!(snapshot(&mut a), want);
        assert_eq!(snapshot(&mut b), want);
        // And it keeps writing and mounting by checkpoint.
        a.put("/after.json", &text(9, 500)).unwrap();
        let mut a = mount(a.into_flash(), &c);
        assert!(a.stats().mount_by_checkpoint);
        assert_eq!(a.get("/after.json").unwrap().unwrap(), text(9, 500));
    }
}
