//! Project read response envelope.

use super::{
    NodeReadResult, ProjectProbeResult, ResourceReadResult, RuntimeReadResult, ShapeReadResult,
};
use crate::slot::WireSlotMutationResponse;
use alloc::vec::Vec;
use lpc_model::Revision;

/// Stateless project read response.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ProjectReadResponse {
    pub revision: Revision,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub results: Vec<ProjectReadResult>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub probes: Vec<ProjectProbeResult>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mutations: Vec<WireSlotMutationResponse>,
}

/// One result aligned with a [`super::ProjectReadQuery`].
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ProjectReadResult {
    Shapes(ShapeReadResult),
    Nodes(NodeReadResult),
    Resources(ResourceReadResult),
    Runtime(RuntimeReadResult),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::ReadLevel;
    use alloc::vec;

    #[test]
    fn project_read_response_round_trips() {
        let response = ProjectReadResponse {
            revision: Revision::new(12),
            results: vec![ProjectReadResult::Shapes(ShapeReadResult {
                level: ReadLevel::Ids,
                registry: None,
            })],
            probes: Vec::new(),
            mutations: Vec::new(),
        };

        let json = serde_json::to_string(&response).unwrap();
        let back: ProjectReadResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(back, response);
    }
}
