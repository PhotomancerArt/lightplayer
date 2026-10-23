//! SPIKE (core-1 render split): run half of a render-samples call on a
//! second core, through a firmware-injected routine.
//!
//! The point loop's per-point work is independent, so a batch of `n` points
//! splits into two contiguous halves. The helper half runs the same JIT'd
//! entry against its **own vmctx** (a copy of the instance's, taken after the
//! uniforms are bound): the wrapper writes the fuel tank and the invocation
//! index into the vmctx per point, so two cores must never share one.
//!
//! This crate is sans-IO and names no chip: the firmware hands in a
//! [`HelperCore`] (start a job on the other core, wait for it), and the core
//! that runs the job calls [`run_helper_job`]. A firmware that installs
//! nothing gets today's single-core path, one atomic load per call.

use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

/// One half of a render-samples call, as the helper core runs it.
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct HelperJob {
    /// The JIT'd `__render_samples_rgba16` entry (an exec address).
    pub entry: usize,
    /// The helper's own vmctx (guest address).
    pub vmctx: i32,
    /// First point of this half (guest address).
    pub points: i32,
    /// First output sample of this half (guest address).
    pub out: i32,
    /// Points in this half.
    pub count: i32,
}

/// The firmware's two routines.
#[derive(Clone, Copy)]
pub struct HelperCore {
    /// Start `job` on the helper core. The pointee stays valid until `join`
    /// returns. `false` = the helper is unavailable (run it here instead).
    pub start: fn(job: *const HelperJob) -> bool,
    /// Wait until the started job has finished.
    pub join: fn(),
}

static START: AtomicUsize = AtomicUsize::new(0);
static JOIN: AtomicUsize = AtomicUsize::new(0);
/// The smallest call worth splitting (points). Below it the hand-off costs
/// more than the half it saves.
static MIN_SPLIT: AtomicU32 = AtomicU32::new(16);

/// SPIKE instrumentation: a firmware cycle counter, and what render-samples
/// calls have cost since boot (both modes, split or not). Wrapping u32s; the
/// reader takes deltas.
static CLOCK: AtomicUsize = AtomicUsize::new(0);
static CALLS: AtomicU32 = AtomicU32::new(0);
static POINTS: AtomicU32 = AtomicU32::new(0);
static CYCLES: AtomicU32 = AtomicU32::new(0);

/// Hand in a cycle counter (CCOUNT) to time render-samples calls with.
pub fn set_render_clock(clock: fn() -> u32) {
    CLOCK.store(clock as usize, Ordering::Release);
}

/// `(calls, points, cycles)` spent in render-samples calls since boot.
pub fn render_stats() -> (u32, u32, u32) {
    (
        CALLS.load(Ordering::Relaxed),
        POINTS.load(Ordering::Relaxed),
        CYCLES.load(Ordering::Relaxed),
    )
}

pub(crate) fn clock_now() -> Option<u32> {
    let c = CLOCK.load(Ordering::Acquire);
    if c == 0 {
        return None;
    }
    // SAFETY: only `set_render_clock` stores here, and only a `fn() -> u32`.
    Some(unsafe { core::mem::transmute::<usize, fn() -> u32>(c) }())
}

pub(crate) fn record_call(points: u32, started: Option<u32>) {
    let Some(t0) = started else { return };
    let Some(t1) = clock_now() else { return };
    CALLS.fetch_add(1, Ordering::Relaxed);
    POINTS.fetch_add(points, Ordering::Relaxed);
    CYCLES.fetch_add(t1.wrapping_sub(t0), Ordering::Relaxed);
}

/// Install the helper core. Call once, at boot, before any shader renders.
pub fn set_helper_core(helper: HelperCore) {
    JOIN.store(helper.join as usize, Ordering::Relaxed);
    START.store(helper.start as usize, Ordering::Release);
}

/// Change the smallest call worth splitting; `u32::MAX` turns splitting off.
pub fn set_helper_min_split(points: u32) {
    MIN_SPLIT.store(points, Ordering::Relaxed);
}

/// The installed helper, if any, when a call of `count` points is worth it.
pub(crate) fn helper_for(count: u32) -> Option<HelperCore> {
    let start = START.load(Ordering::Acquire);
    if start == 0 || count < MIN_SPLIT.load(Ordering::Relaxed) {
        return None;
    }
    let join = JOIN.load(Ordering::Relaxed);
    // SAFETY: only `set_helper_core` stores these, and only these fn types.
    unsafe {
        Some(HelperCore {
            start: core::mem::transmute::<usize, fn(*const HelperJob) -> bool>(start),
            join: core::mem::transmute::<usize, fn()>(join),
        })
    }
}

/// Run `job` on the calling core: the helper core's side of the split.
///
/// # Safety
///
/// `job` must come from this crate's split (a live entry, vmctx and buffers).
#[cfg(any(target_arch = "riscv32", target_arch = "xtensa"))]
pub unsafe fn run_helper_job(job: &HelperJob) {
    unsafe { crate::rt_jit::call_render_samples_raw(job) }
}
