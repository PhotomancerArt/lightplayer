//! Runtime/status project-read domain.

use lpc_model::Revision;

use crate::server::MemoryStats;

/// Request for runtime status data.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct RuntimeReadQuery;

/// Runtime/status result for a single project read.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct RuntimeReadResult {
    /// Project-engine status.
    pub project: ProjectRuntimeStatus,
    /// Optional server-loop status. Engine-only callers leave this absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<ServerRuntimeStatus>,
}

/// Project-engine runtime counters.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ProjectRuntimeStatus {
    pub revision: Revision,
    pub frame_num: u64,
    pub frame_delta_ms: u32,
    pub frame_total_ms: u32,
    pub demand_root_count: u32,
    pub runtime_buffer_count: u32,
}

/// Server-loop runtime counters.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ServerRuntimeStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theoretical_fps: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_frame_time_us: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<MemoryStats>,
}
