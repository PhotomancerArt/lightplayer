//! Runtime/status project-read helpers.

use lpc_wire::{ProjectRuntimeStatus, RuntimeReadQuery, RuntimeReadResult, ServerRuntimeStatus};

use super::Engine;

impl Engine {
    pub fn read_project_runtime(
        &self,
        _query: RuntimeReadQuery,
        server: Option<ServerRuntimeStatus>,
    ) -> RuntimeReadResult {
        RuntimeReadResult {
            project: ProjectRuntimeStatus {
                revision: self.revision(),
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
