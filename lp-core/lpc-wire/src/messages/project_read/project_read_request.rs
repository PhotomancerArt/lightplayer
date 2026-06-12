//! Project read request envelope.

use super::{
    NodeReadQuery, ProjectProbeRequest, ReadLevel, ResourcePayloadRead, ResourceReadQuery,
    RuntimeReadQuery, ShapeReadQuery,
};
use alloc::vec::Vec;
use lpc_model::Revision;

/// Stateless project read request.
///
/// `since` is the only client-side sync state the server needs. `queries`
/// select mirrorable project data; `probes` request diagnostic work outside the
/// normal client mirror.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ProjectReadRequest {
    pub since: Option<Revision>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queries: Vec<ProjectReadQuery>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub probes: Vec<ProjectProbeRequest>,
}

impl ProjectReadRequest {
    /// Build the default developer/debug read: shapes, node detail, resource
    /// summaries, and no expensive payloads or probes.
    #[must_use]
    pub fn default_debug(since: Option<Revision>) -> Self {
        Self {
            since,
            queries: ProjectReadQuery::default_debug(),
            probes: Vec::new(),
        }
    }
}

/// One mirrorable domain requested by a project read.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ProjectReadQuery {
    Shapes(ShapeReadQuery),
    Nodes(NodeReadQuery),
    Resources(ResourceReadQuery),
    Runtime(RuntimeReadQuery),
}

impl ProjectReadQuery {
    /// Standard high-signal debug query list.
    #[must_use]
    pub fn default_debug() -> Vec<Self> {
        Vec::from([
            Self::Shapes(ShapeReadQuery {
                level: ReadLevel::Detail,
                after: None,
                limit: None,
            }),
            Self::Nodes(NodeReadQuery::detail_all()),
            Self::Resources(ResourceReadQuery {
                level: ReadLevel::Summary,
                payloads: ResourcePayloadRead::None,
            }),
            Self::Runtime(RuntimeReadQuery),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn project_read_request_round_trips() {
        let request = ProjectReadRequest {
            since: Some(Revision::new(7)),
            queries: ProjectReadQuery::default_debug(),
            probes: vec![ProjectProbeRequest::unsupported_example_for_test()],
        };

        let json = serde_json::to_string(&request).unwrap();
        let back: ProjectReadRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(back, request);
    }
}
