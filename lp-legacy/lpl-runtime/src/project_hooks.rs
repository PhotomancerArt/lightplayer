//! Register legacy project behavior with `lpc-runtime` (breaks the `lpc-runtime` ↔ `lpl-runtime` cycle).

use alloc::sync::Arc;
use lpc_model::FrameId;
use lpc_model::project::api::ApiNodeSpecifier;
use lpc_runtime::error::Error;
use lpc_runtime::project::ProjectRuntime;
use lpc_runtime::project::hooks::{ProjectHooks, set_project_hooks};
use lpfs::FsChange;
use lpl_model::ProjectResponse;

/// Wire legacy node runtimes into `ProjectRuntime` (idempotent).
pub fn install() {
    set_project_hooks(Arc::new(DefaultProjectHooks));
}

struct DefaultProjectHooks;

impl ProjectHooks for DefaultProjectHooks {
    fn init_nodes(&self, rt: &mut ProjectRuntime) -> Result<(), Error> {
        crate::legacy_hooks::init_nodes(rt)
    }

    fn tick(&self, rt: &mut ProjectRuntime, delta_ms: u32) -> Result<(), Error> {
        crate::legacy_hooks::tick(rt, delta_ms)
    }

    fn handle_fs_changes(
        &self,
        rt: &mut ProjectRuntime,
        changes: &[FsChange],
    ) -> Result<(), Error> {
        crate::legacy_hooks::handle_fs_changes(rt, changes)
    }

    fn get_changes(
        &self,
        rt: &ProjectRuntime,
        since_frame: FrameId,
        detail_specifier: &ApiNodeSpecifier,
        theoretical_fps: Option<f32>,
    ) -> Result<ProjectResponse, Error> {
        crate::legacy_hooks::get_changes(rt, since_frame, detail_specifier, theoretical_fps)
    }
}
