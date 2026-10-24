//! A second core for render-samples calls, through a routine the firmware
//! injects.
//!
//! A render-samples call's per-point work is independent, so a batch of `n`
//! points splits into two contiguous halves. The helper half runs the same
//! JIT'd entry against its **own vmctx** (a copy of the instance's, taken
//! after the uniforms are bound and the globals reset): the entry writes the
//! fuel tank, the invocation index and the shader's globals into the vmctx
//! per point, so two cores never share one. Both halves produce exactly the
//! bytes one core would.
//!
//! This crate is sans-IO and names no chip: the firmware hands in a
//! [`HelperCore`] (start a job on the other core, wait for it), and the core
//! that runs a job calls [`run_helper_job`]. A firmware that installs nothing
//! gets the single-core path, one atomic load per call. A helper that does not
//! finish (its join gives up) costs a re-run of its half on the calling core:
//! the batch still renders, slower.

use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

/// One half of a render-samples call, as either core runs it.
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct HelperJob {
    /// The JIT'd render-samples entry (an exec address).
    pub entry: usize,
    /// This half's vmctx (guest address), armed.
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
    /// returns. `false`: the helper is unavailable, and the caller runs the
    /// half itself.
    pub start: fn(job: *const HelperJob) -> bool,
    /// Wait for the started job. `false`: it did not finish (the helper
    /// stopped, or the wait timed out), and the caller runs the half itself.
    pub join: fn() -> bool,
}

static START: AtomicUsize = AtomicUsize::new(0);
static JOIN: AtomicUsize = AtomicUsize::new(0);
/// The smallest call worth splitting (points). Below it the hand-off costs
/// more than the half it saves.
static MIN_SPLIT: AtomicU32 = AtomicU32::new(16);

/// Install the helper core. Once, at boot, before any shader renders.
pub fn set_helper_core(helper: HelperCore) {
    JOIN.store(helper.join as usize, Ordering::Relaxed);
    START.store(helper.start as usize, Ordering::Release);
}

/// Stop splitting: every later call runs on one core. For a firmware whose
/// helper failed.
pub fn clear_helper_core() {
    START.store(0, Ordering::Release);
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
            join: core::mem::transmute::<usize, fn() -> bool>(join),
        })
    }
}

/// Run `job` on the calling core: the helper core's side of the split.
///
/// It touches nothing but `job`'s entry, vmctx and buffers: no allocation,
/// no lock, no log. A firmware may rely on that to stall the helper core at
/// any instruction (a flash write), and to forbid it the allocator.
///
/// # Safety
///
/// `job` must come from this crate's split: a live entry, a vmctx of its
/// module, and buffers that outlive the call.
pub unsafe fn run_helper_job(job: &HelperJob) {
    unsafe { crate::rt_jit::call_render_samples_raw(job) }
}
