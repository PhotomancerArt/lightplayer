//! Registry for browser firmware runtimes.
//!
//! The wasm boundary uses numeric runtime ids so one page can create multiple
//! browser firmware instances without exposing Rust references to JavaScript.

use std::cell::RefCell;

use crate::runtime::BrowserFirmwareRuntime;

thread_local! {
    static RUNTIMES: RefCell<Vec<BrowserFirmwareRuntime>> = const { RefCell::new(Vec::new()) };
}

/// Create a runtime and return its stable id for later wasm calls.
pub(crate) fn create_runtime(label: &str) -> Result<u32, String> {
    RUNTIMES.with(|runtimes| {
        let mut runtimes = runtimes.borrow_mut();
        let id = runtimes.len() as u32 + 1;
        runtimes.push(BrowserFirmwareRuntime::new(id, label)?);
        Ok(id)
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
