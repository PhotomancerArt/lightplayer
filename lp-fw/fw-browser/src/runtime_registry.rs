//! Registry for browser firmware runtimes.
//!
//! The wasm boundary uses numeric runtime ids so one page can create multiple
//! browser firmware instances without exposing Rust references to JavaScript.

use std::cell::{Cell, RefCell};

use crate::runtime::BrowserFirmwareRuntime;
use crate::tier::{RuntimeTier, TierSelection};

thread_local! {
    static RUNTIMES: RefCell<Vec<BrowserFirmwareRuntime>> = const { RefCell::new(Vec::new()) };
    // Monotonic so a destroyed runtime's id is never reissued: `len() + 1`
    // would collide with a live runtime once destruction punches holes in
    // the id sequence.
    static NEXT_RUNTIME_ID: Cell<u32> = const { Cell::new(1) };
}

/// Create a runtime on the requested tier; returns its stable id plus the
/// recorded tier selection (which may be CPU with a reason when a GPU
/// request could not be granted — fidelity-tiers ADR).
pub(crate) fn create_runtime(
    label: &str,
    requested: RuntimeTier,
) -> Result<(u32, TierSelection), String> {
    RUNTIMES.with(|runtimes| {
        let mut runtimes = runtimes.borrow_mut();
        let id = NEXT_RUNTIME_ID.with(|next| {
            let id = next.get();
            next.set(id + 1);
            id
        });
        let runtime = BrowserFirmwareRuntime::new(id, label, requested)?;
        let selection = runtime.tier().clone();
        runtimes.push(runtime);
        Ok((id, selection))
    })
}

/// Drop the runtime with `runtime_id`, releasing everything it owns (server,
/// filesystem, and on the GPU tier the graphics backend plus any attached
/// preview surface).
///
/// Returns `false` when no such runtime exists — release is idempotent.
pub(crate) fn destroy_runtime(runtime_id: u32) -> bool {
    RUNTIMES.with(|runtimes| {
        let mut runtimes = runtimes.borrow_mut();
        let before = runtimes.len();
        runtimes.retain(|runtime| runtime.id() != runtime_id);
        runtimes.len() != before
    })
}

/// Return the number of runtimes currently held by this wasm instance.
pub(crate) fn runtime_count() -> u32 {
    RUNTIMES.with(|runtimes| runtimes.borrow().len() as u32)
}

/// Borrow a runtime by id for one wasm export call.
pub(crate) fn with_runtime_mut<T>(
    runtime_id: u32,
    f: impl FnOnce(&mut BrowserFirmwareRuntime) -> Result<T, String>,
) -> Result<T, String> {
    RUNTIMES.with(|runtimes| {
        let mut runtimes = runtimes.borrow_mut();
        let runtime = runtimes
            .iter_mut()
            .find(|runtime| runtime.id() == runtime_id)
            .ok_or_else(|| format!("runtime {runtime_id} not found"))?;
        f(runtime)
    })
}
