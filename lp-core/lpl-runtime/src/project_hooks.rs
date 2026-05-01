//! Register legacy project behavior with `lpc-runtime` (breaks the `lpc-runtime` ↔ `lpl-runtime` cycle).

use alloc::sync::Arc;
use lpc_engine::error::Error;
use lpc_engine::legacy_project::LegacyProjectRuntime;
use lpc_engine::legacy_project::hooks::{LegacyProjectHooks, set_project_hooks};
use lpc_model::FrameId;
use lpc_wire::WireNodeSpecifier;
use lpfs::FsChange;
use lpl_model::ProjectResponse;

/// Wire legacy node runtimes into `ProjectRuntime` (idempotent).
pub fn install() {
    set_project_hooks(Arc::new(DefaultProjectHooks));
}

struct DefaultProjectHooks;

impl LegacyProjectHooks for DefaultProjectHooks {
    fn init_nodes(&self, rt: &mut LegacyProjectRuntime) -> Result<(), Error> {
        crate::legacy_hooks::init_nodes(rt)
    }

    fn tick(&self, rt: &mut LegacyProjectRuntime, delta_ms: u32) -> Result<(), Error> {
        crate::legacy_hooks::tick(rt, delta_ms)
    }

    fn handle_fs_changes(
        &self,
        rt: &mut LegacyProjectRuntime,
        changes: &[FsChange],
    ) -> Result<(), Error> {
        crate::legacy_hooks::handle_fs_changes(rt, changes)
    }

    fn get_changes(
        &self,
        rt: &LegacyProjectRuntime,
        since_frame: FrameId,
        detail_specifier: &WireNodeSpecifier,
        theoretical_fps: Option<f32>,
    ) -> Result<ProjectResponse, Error> {
        crate::legacy_hooks::get_changes(rt, since_frame, detail_specifier, theoretical_fps)
    }
}
