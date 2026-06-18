use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct StudioHeartbeat {
    pub fps_avg: f32,
    pub frame_count: u64,
    pub loaded_project_count: usize,
    pub uptime_ms: u64,
    pub free_memory_bytes: Option<u32>,
}
