//! Pluggable project hooks (implemented by `lpl-runtime`) to avoid a Cargo cycle
//! (`lpc-runtime` ↔ `lpl-runtime`).

use crate::error::Error;
use crate::project::project_runtime::ProjectRuntime;
use alloc::string::String;
use alloc::sync::Arc;
use lpc_model::{FrameId, project::api::ApiNodeSpecifier};
use lpfs::FsChange;
use lpl_model::ProjectResponse;
use spin::Mutex;

/// Snapshot of the legacy node integration (init, tick, filesystem sync, client protocol).
pub trait ProjectHooks: Send + Sync {
    fn init_nodes(&self, rt: &mut ProjectRuntime) -> Result<(), Error>;

    fn tick(&self, rt: &mut ProjectRuntime, delta_ms: u32) -> Result<(), Error>;

    fn handle_fs_changes(&self, rt: &mut ProjectRuntime, changes: &[FsChange])
    -> Result<(), Error>;

    fn get_changes(
        &self,
        rt: &ProjectRuntime,
        since_frame: FrameId,
        detail_specifier: &ApiNodeSpecifier,
        theoretical_fps: Option<f32>,
    ) -> Result<ProjectResponse, Error>;
}

static HOOKS: Mutex<Option<Arc<dyn ProjectHooks>>> = Mutex::new(None);

/// Install legacy project hooks (idempotent). Normally invoked from `lpl_runtime::project_hooks::install`.
pub fn set_project_hooks(hooks: Arc<dyn ProjectHooks>) {
    let mut guard = HOOKS.lock();
    if guard.is_none() {
        *guard = Some(hooks);
    }
}

pub(crate) fn with_hooks<T>(
    f: impl FnOnce(&dyn ProjectHooks) -> Result<T, Error>,
) -> Result<T, Error> {
    let guard = HOOKS.lock();
    let hooks = guard.as_ref().ok_or_else(|| Error::Other {
        message: String::from(
            "project hooks not installed; call lpl_runtime::project_hooks::install() before using ProjectRuntime node operations",
        ),
    })?;
    f(hooks.as_ref())
}
