use lpc_wire::RuntimeReadResult;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProjectRuntimeSummary {
    pub frame_num: u64,
    pub frame_delta_ms: u32,
    pub runtime_buffer_count: u32,
    pub free_bytes: Option<u64>,
    pub used_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
}

impl From<&RuntimeReadResult> for ProjectRuntimeSummary {
    fn from(runtime: &RuntimeReadResult) -> Self {
        let memory = runtime
            .server
            .as_ref()
            .and_then(|server| server.memory.as_ref());
        Self {
            frame_num: runtime.project.frame_num,
            frame_delta_ms: runtime.project.frame_delta_ms,
            runtime_buffer_count: runtime.project.runtime_buffer_count,
            free_bytes: memory.map(|memory| u64::from(memory.free_bytes)),
            used_bytes: memory.map(|memory| u64::from(memory.used_bytes)),
            total_bytes: memory.map(|memory| u64::from(memory.total_bytes)),
        }
    }
}
