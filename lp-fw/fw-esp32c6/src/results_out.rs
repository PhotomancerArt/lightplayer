//! RESEARCH (`research/ram-e15`, experiment E15 of
//! `lp2025/2026-10-09-1203-ram-research`; **never shipped**): keep the
//! per-edit results out of `dram2_seg`.
//!
//! `dram2_seg` is the C6's accidental big block (E8/E9): the main region is
//! full before the first compile, so every large transient lands there —
//! and so do the long-lived results each edit makes, which then hole it
//! (E9's mixers, E11's 2.7–6.6 KB a compile). Three lp-perf windows bracket
//! those results:
//!
//! - `registry-refresh` — the registry inventory a file write re-derives;
//! - `asset-text` — the shader text;
//! - `shader-link` — the linked JIT module.
//!
//! While one is open on the engine's thread, a capability-free allocation
//! made on that thread reaches `dram2_seg` LAST (main, then the radio
//! region, then `dram2_seg`: the esp-alloc fork's `avoid-region`), except
//! inside an `artifact-read` window (a whole-file read is a transient).
//!
//! Everything [`_esp_alloc_avoid`] reads is an atomic: it runs inside the
//! heap's lock.

use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use lp_perf::PerfEventKind;

/// Open result windows on [`OWNER`]'s thread.
static DEPTH: AtomicU32 = AtomicU32::new(0);
/// Open `artifact-read` windows inside them.
static SUPPRESS: AtomicU32 = AtomicU32::new(0);
/// The thread (its `tp`) that opened the outermost window.
static OWNER: AtomicUsize = AtomicUsize::new(0);
/// Windows opened, by kind: refresh, text, link.
static OPENED: [AtomicU32; 3] = [AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0)];

/// `dram2_seg` is the heap's second region (`board::esp32c6::init`).
const DRAM2_INDEX: usize = 1;

/// Make `dram2_seg` the avoided region and install the marker hook. Call
/// once, before the server's first tick.
pub fn install() {
    esp_alloc::HEAP.set_avoid_region(DRAM2_INDEX);
    lp_perf::set_hook(on_marker);
    log::info!(
        "[e15] results out of dram2_seg: registry-refresh, asset-text, shader-link reach it last"
    );
}

fn on_marker(name: &'static str, kind: PerfEventKind) {
    let window = match name {
        lp_perf::EVENT_REGISTRY_REFRESH => Some(0),
        lp_perf::EVENT_ASSET_TEXT => Some(1),
        lp_perf::EVENT_SHADER_LINK => Some(2),
        _ => None,
    };
    if let Some(window) = window {
        match kind {
            PerfEventKind::Begin => {
                if DEPTH.load(Ordering::Relaxed) == 0 {
                    OWNER.store(thread(), Ordering::Relaxed);
                }
                OPENED[window].fetch_add(1, Ordering::Relaxed);
                DEPTH.fetch_add(1, Ordering::Release);
            }
            PerfEventKind::End => {
                let _ =
                    DEPTH.fetch_update(Ordering::Release, Ordering::Relaxed, |d| d.checked_sub(1));
            }
            PerfEventKind::Instant => {}
        }
        return;
    }
    if name == lp_perf::EVENT_ARTIFACT_READ && thread() == OWNER.load(Ordering::Relaxed) {
        match kind {
            PerfEventKind::Begin => {
                SUPPRESS.fetch_add(1, Ordering::Release);
            }
            PerfEventKind::End => {
                let _ = SUPPRESS
                    .fetch_update(Ordering::Release, Ordering::Relaxed, |d| d.checked_sub(1));
            }
            PerfEventKind::Instant => {}
        }
    } else if name == lp_perf::EVENT_SHADER_COMPILE && matches!(kind, PerfEventKind::End) {
        log_counters();
    }
}

/// esp-alloc's question, from inside the heap lock: should this allocation
/// reach `dram2_seg` last?
#[unsafe(no_mangle)]
pub fn _esp_alloc_avoid() -> bool {
    DEPTH.load(Ordering::Acquire) != 0
        && SUPPRESS.load(Ordering::Acquire) == 0
        && thread() == OWNER.load(Ordering::Relaxed)
}

/// One `[e15]` line: windows opened and where their allocations went.
pub fn log_counters() {
    let stats = esp_alloc::HEAP.avoid_stats();
    log::info!(
        "[e15] windows refresh={} text={} link={}; avoided {} allocs {} B; fell back to dram2 {} allocs {} B",
        OPENED[0].load(Ordering::Relaxed),
        OPENED[1].load(Ordering::Relaxed),
        OPENED[2].load(Ordering::Relaxed),
        stats.avoided_count,
        stats.avoided_bytes,
        stats.fell_back_count,
        stats.fell_back_bytes,
    );
}

fn thread() -> usize {
    let tp: usize;
    // SAFETY: reads a register.
    unsafe { core::arch::asm!("mv {0}, tp", out(reg) tp) };
    tp
}
