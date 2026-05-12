//! Stateless project read builder for [`Engine`].

use lpc_wire::{
    ProjectProbeRequest, ProjectProbeResult, ProjectReadQuery, ProjectReadRequest,
    ProjectReadResponse, ProjectReadResult,
};

use super::Engine;

impl Engine {
    /// Answer one stateless project read request from the current engine state.
    pub fn read_project(&self, request: ProjectReadRequest) -> ProjectReadResponse {
        let revision = self.revision();
        let results = request
            .queries
            .into_iter()
            .map(|query| match query {
                ProjectReadQuery::Shapes(query) => {
                    ProjectReadResult::Shapes(self.read_project_shapes(query))
                }
                ProjectReadQuery::Nodes(query) => {
                    ProjectReadResult::Nodes(self.read_project_nodes(request.since, query))
                }
                ProjectReadQuery::Resources(query) => {
                    ProjectReadResult::Resources(self.read_project_resources(query))
                }
            })
            .collect();
        let probes = request
            .probes
            .into_iter()
            .map(|probe| match probe {
                ProjectProbeRequest::RenderProduct(request) => ProjectProbeResult::RenderProduct(
                    self.read_project_render_product_probe(request),
                ),
                ProjectProbeRequest::ExplainSlot(request) => {
                    ProjectProbeResult::ExplainSlot(self.read_project_explain_slot_probe(request))
                }
            })
            .collect();

        ProjectReadResponse {
            revision,
            results,
            probes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::{Revision, TreePath, WithRevision};
    use lpc_wire::{ProjectReadRequest, ProjectReadResult, ResourcePayloadRead};

    use crate::engine::test_support::EngineTestBuilder;
    use crate::resource::RuntimeBuffer;

    #[test]
    fn default_debug_read_returns_shapes_nodes_and_resource_summaries() {
        let mut engine = Engine::new(TreePath::parse("/basic.project").unwrap());
        engine.runtime_buffers_mut().insert(WithRevision::new(
            Revision::new(1),
            RuntimeBuffer::output_channels_u16(3, alloc::vec![0, 1, 2, 3, 4, 5]),
        ));

        let response = engine.read_project(ProjectReadRequest::default_debug(None));

        assert_eq!(response.results.len(), 3);
        assert!(matches!(response.results[0], ProjectReadResult::Shapes(_)));
        assert!(matches!(response.results[1], ProjectReadResult::Nodes(_)));
        let ProjectReadResult::Resources(resources) = &response.results[2] else {
            panic!("third result should be resources");
        };
        assert_eq!(resources.summaries.len(), 1);
        assert!(resources.runtime_buffer_payloads.is_empty());
    }

    #[test]
    fn default_debug_read_skips_nodes_without_runtime_state_roots() {
        let h = EngineTestBuilder::new().output_node("output").build();

        let response = h
            .engine
            .read_project(ProjectReadRequest::default_debug(None));

        let ProjectReadResult::Nodes(nodes) = &response.results[1] else {
            panic!("second result should be nodes");
        };
        let slots = nodes.slots.as_ref().expect("node slot snapshot");
        assert!(
            slots
                .roots
                .iter()
                .all(|root| !root.name.ends_with(".state")),
            "output node has no public runtime state root"
        );
    }

    #[test]
    fn resource_payload_read_all_includes_buffer_bytes() {
        let mut engine = Engine::new(TreePath::parse("/basic.project").unwrap());
        engine.runtime_buffers_mut().insert(WithRevision::new(
            Revision::new(1),
            RuntimeBuffer::raw(alloc::vec![1, 2, 3]),
        ));

        let mut request = ProjectReadRequest::default_debug(None);
        request.queries[2] = lpc_wire::ProjectReadQuery::Resources(lpc_wire::ResourceReadQuery {
            level: lpc_wire::ReadLevel::Detail,
            payloads: ResourcePayloadRead::All,
        });
        let response = engine.read_project(request);

        let ProjectReadResult::Resources(resources) = &response.results[2] else {
            panic!("third result should be resources");
        };
        assert_eq!(resources.runtime_buffer_payloads.len(), 1);
        assert_eq!(
            resources.runtime_buffer_payloads[0].bytes,
            alloc::vec![1, 2, 3]
        );
    }
}
