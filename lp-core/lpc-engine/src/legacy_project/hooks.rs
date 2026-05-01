//! Pluggable project hooks (implemented by `lpl-runtime`) to avoid a Cargo cycle
//! (`lpc-runtime` ↔ `lpl-runtime`).

use crate::error::Error;
use crate::legacy_project::project_runtime::LegacyProjectRuntime;
use alloc::string::String;
use alloc::sync::Arc;
use lpc_model::FrameId;
use lpc_wire::WireNodeSpecifier;
use lpfs::FsChange;
use lpl_model::ProjectResponse;
use spin::Mutex;

/// Snapshot of the legacy node integration (init, tick, filesystem sync, client protocol).
pub trait LegacyProjectHooks: Send + Sync {
    fn init_nodes(&self, rt: &mut LegacyProjectRuntime) -> Result<(), Error>;

    fn tick(&self, rt: &mut LegacyProjectRuntime, delta_ms: u32) -> Result<(), Error>;

    fn handle_fs_changes(&self, rt: &mut LegacyProjectRuntime, changes: &[FsChange])
                         -> Result<(), Error>;

    fn get_changes(
        &self,
        rt: &LegacyProjectRuntime,
        since_frame: FrameId,
        detail_specifier: &WireNodeSpecifier,
        theoretical_fps: Option<f32>,
    ) -> Result<ProjectResponse, Error>;
}

static HOOKS: Mutex<Option<Arc<dyn LegacyProjectHooks>>> = Mutex::new(None);

/// Install legacy project hooks (idempotent). Normally invoked from `lpl_runtime::project_hooks::install`.
pub fn set_project_hooks(hooks: Arc<dyn LegacyProjectHooks>) {
    let mut guard = HOOKS.lock();
    if guard.is_none() {
        *guard = Some(hooks);
    }
}

pub(crate) fn with_hooks<T>(
    f: impl FnOnce(&dyn LegacyProjectHooks) -> Result<T, Error>,
) -> Result<T, Error> {
    let guard = HOOKS.lock();
    let hooks = guard.as_ref().ok_or_else(|| Error::Other {
        message: String::from(
            "project hooks not installed; call lpl_runtime::project_hooks::install() before using ProjectRuntime node operations",
        ),
    })?;
    f(hooks.as_ref())
}
