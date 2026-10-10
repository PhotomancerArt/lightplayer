//! A Bluetooth central connecting mid-project, stood in for.
//!
//! The emulated C6 has no central (AGENTS.md: BLE's air is not modelled),
//! so a phone's link cannot open on it. E8 measured what one costs on
//! silicon: 17,908 B, of which 7,112 B is the radio lp-link and 6,904 B is
//! one JSON Pack learned table, the rest small pieces. This task, on the
//! main executor like the Bluetooth host's own tasks, makes allocations of
//! that shape when [`trigger`] is called and holds them for the boot's
//! life. It runs whenever the executor next polls it — often while the
//! server's project read is awaiting a send with its loan open, which is
//! the case the borrower check must get right: these bytes are not the
//! borrower's and must not land in the block.

extern crate alloc;

use alloc::vec::Vec;

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;

static GO: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// Wake the stand-in.
pub fn trigger() {
    GO.signal(());
}

/// In an image without the lender: count project reads off lp-perf's
/// markers and trigger after the 30th, as the lender's edge does.
#[cfg(not(feature = "e11_lender"))]
pub fn install_counting_hook() {
    lp_perf::set_hook(|name, kind| {
        use core::sync::atomic::{AtomicU32, Ordering};
        static READS: AtomicU32 = AtomicU32::new(0);
        if name == lp_perf::EVENT_PROJECT_READ
            && matches!(kind, lp_perf::PerfEventKind::End)
            && READS.fetch_add(1, Ordering::Relaxed) + 1 == 30
        {
            trigger();
        }
    });
}

/// The stand-in's task. Spawn once on the main executor.
#[embassy_executor::task]
pub async fn link_standin_task() {
    GO.wait().await;
    let (start, size) = super::block_region();
    let mut held: Vec<Vec<u8>> = Vec::with_capacity(42);
    held.push(alloc::vec![0x11; 7_112]);
    held.push(alloc::vec![0x22; 6_904]);
    for _ in 0..40 {
        held.push(alloc::vec![0x33; 96]);
    }
    let in_block: usize = held
        .iter()
        .filter(|piece| {
            let at = piece.as_ptr() as usize;
            at >= start && at < start + size
        })
        .map(Vec::len)
        .sum();
    let total: usize = held.iter().map(Vec::len).sum();
    log::info!("[e11] link stand-in: {total} B held, {in_block} B of it in the block");
    core::mem::forget(held);
}
