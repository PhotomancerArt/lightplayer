use lpc_wire::{WireProjectHandle, WireProjectInventoryReadResponse};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct ProjectSession {
    pub project_id: String,
    pub handle: WireProjectHandle,
    pub inventory: Option<WireProjectInventoryReadResponse>,
    pub selected_node_id: Option<String>,
}

impl ProjectSession {
    pub fn new(project_id: impl Into<String>, handle: WireProjectHandle) -> Self {
        Self {
            project_id: project_id.into(),
            handle,
            inventory: None,
            selected_node_id: None,
        }
    }
}
