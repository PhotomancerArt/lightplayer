//! Runtime/status project-read helpers.

use lpc_model::Revision;
use lpc_wire::{ProjectRuntimeStatus, RuntimeReadQuery, RuntimeReadResult, ServerRuntimeStatus};

use super::Engine;

impl Engine {
    /// Build the runtime/status read result.
    ///
    /// `overlay_changed_at` is the revision at which the project's pending-edit
    /// overlay last changed. The overlay is registry state, not engine state,
    /// so callers (the project-read stream) pass the plain revision in.
    pub fn read_project_runtime(
        &self,
        _query: RuntimeReadQuery,
        overlay_changed_at: Revision,
        server: Option<ServerRuntimeStatus>,
    ) -> RuntimeReadResult {
        RuntimeReadResult {
            project: ProjectRuntimeStatus {
                revision: self.revision(),
                overlay_changed_at,
                frame_num: self.frame_num().raw(),
                frame_delta_ms: self.frame_time().delta_ms,
                frame_total_ms: self.frame_time().total_ms,
                demand_root_count: self.demand_roots().len() as u32,
                runtime_buffer_count: self.runtime_buffers().len() as u32,
            },
            server,
        }
    }
}
