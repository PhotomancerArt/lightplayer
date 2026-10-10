//! RESEARCH (research/ram-e03; never shipped): squeeze the heap outside LP
//! SRAM down to a set slack, once, so a later project load shows whether the
//! LP SRAM region is what lets it fit.
//!
//! On the [`AT_HEARTBEAT`]th heartbeat, allocate and leak chunks (4 KiB, then
//! 1 KiB, 256 B, 64 B, so holes fill too) until the free bytes outside LP SRAM
//! are within one chunk of `LP_E3_BALLAST_LEAVE` (build-time env, bytes,
//! default 16 KiB). A chunk that lands in LP SRAM is given back at once and
//! ends that chunk size: the ballast never takes LP SRAM's own room. The same
//! build with and without `e03_lp_heap` therefore leaves the same slack in HP
//! SRAM, and differs only in the 15 KiB LP SRAM region — the counterfactual.

extern crate alloc;

use core::sync::atomic::{AtomicUsize, Ordering};

/// The heartbeat that places the ballast (heartbeats are 5 s apart, so the
/// 2nd is ~10 s after boot: after an upload's first compile has settled).
const AT_HEARTBEAT: usize = 2;
/// Chunk sizes, largest first.
const CHUNKS: [usize; 4] = [4096, 1024, 256, 64];

static HEARTBEATS: AtomicUsize = AtomicUsize::new(0);

/// Count a heartbeat; place the ballast on the [`AT_HEARTBEAT`]th.
#[cfg_attr(
    fw_harness,
    allow(dead_code, reason = "the heartbeat that calls it is the product's")
)]
pub fn heartbeat() {
    if HEARTBEATS.fetch_add(1, Ordering::Relaxed) + 1 != AT_HEARTBEAT {
        return;
    }
    let leave: usize = option_env!("LP_E3_BALLAST_LEAVE")
        .and_then(|text| text.parse().ok())
        .unwrap_or(16 * 1024);
    let mut held = 0usize;
    let mut chunks = 0usize;
    for size in CHUNKS {
        loop {
            if free_outside_lp() < leave + size {
                break;
            }
            let layout = core::alloc::Layout::from_size_align(size, 8).unwrap();
            // SAFETY: non-zero layout; the block is leaked or freed below with
            // the same layout.
            let ptr = unsafe { alloc::alloc::alloc(layout) };
            if ptr.is_null() {
                break;
            }
            if in_lp_sram(ptr as usize) {
                // SAFETY: just allocated with this layout.
                unsafe { alloc::alloc::dealloc(ptr, layout) };
                break;
            }
            held += size;
            chunks += 1;
        }
    }
    log::info!(
        "[e03-ballast] held {held} B in {chunks} chunks; {} B free outside LP SRAM (asked to leave {leave} B), {} B free in LP SRAM",
        free_outside_lp(),
        lp_free(),
    );
}

fn in_lp_sram(addr: usize) -> bool {
    (0x5000_0000..0x5000_4000).contains(&addr)
}

fn free_outside_lp() -> usize {
    esp_alloc::HEAP.free().saturating_sub(lp_free())
}

#[cfg(feature = "e03_lp_heap")]
fn lp_free() -> usize {
    super::lp_sram_heap::free_bytes()
}

#[cfg(not(feature = "e03_lp_heap"))]
fn lp_free() -> usize {
    0
}
