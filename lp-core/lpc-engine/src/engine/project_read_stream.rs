//! Streaming project-read response writer for [`Engine`].

use lpc_model::SlotAccess;
use lpc_wire::json::json_write::JsonWrite;
use lpc_wire::json::json_writer::{JsonWriter, JsonWriterError};
use lpc_wire::{
    NodeReadQuery, ProjectProbeRequest, ProjectProbeResult, ProjectReadQuery, ProjectReadRequest,
    ProjectReadResult, RuntimeReadQuery, ServerRuntimeStatus, ShapeReadQuery,
    WireSlotMutationRequest, WireSlotMutationResponse, write_project_read_result_json,
    write_slot_data_json, write_slot_shape_registry_snapshot_json,
};

use crate::node::{NodeEntryState, tree_deltas_since};

use super::Engine;
use super::project_read_nodes::{node_def_root_name, node_state_root_name};

impl Engine {
    /// Write one stateless project read response directly to a JSON sink.
    ///
    /// This preserves the same JSON shape as [`Self::read_project`], but writes
    /// each query/probe result as soon as it is produced. The current
    /// implementation may still allocate individual result objects; it avoids
    /// allocating the whole response envelope and uses streaming base64 for
    /// runtime-buffer payload fields.
    pub fn write_project_read_json<W>(
        &mut self,
        request: ProjectReadRequest,
        out: W,
    ) -> Result<W, JsonWriterError<W::Error>>
    where
        W: JsonWrite,
    {
        lpc_shared::transport::ProjectReadJsonSource::write_project_read_json(self, request, out)
    }
}

impl lpc_shared::transport::ProjectReadJsonSource for Engine {
    fn project_read_revision(&self) -> lpc_model::Revision {
        self.revision()
    }

    fn apply_project_mutations(
        &mut self,
        mutations: alloc::vec::Vec<WireSlotMutationRequest>,
    ) -> alloc::vec::Vec<WireSlotMutationResponse> {
        self.mutate_project_slots(mutations)
    }

    fn write_project_read_result_json<W>(
        &mut self,
        since: Option<lpc_model::Revision>,
        query: ProjectReadQuery,
        out: W,
    ) -> Result<W, JsonWriterError<W::Error>>
    where
        W: JsonWrite,
    {
        match query {
            ProjectReadQuery::Shapes(query) => {
                return self.write_project_shape_read_result_json(query, out);
            }
            ProjectReadQuery::Nodes(query) => {
                return self.write_project_node_read_result_json(since, query, out);
            }
            ProjectReadQuery::Runtime(query) => {
                return self.write_project_runtime_read_result_json(query, None, out);
            }
            ProjectReadQuery::Resources(_) => {}
        }

        let result = match query {
            ProjectReadQuery::Resources(query) => {
                ProjectReadResult::Resources(self.read_project_resources(query))
            }
            ProjectReadQuery::Shapes(_)
            | ProjectReadQuery::Nodes(_)
            | ProjectReadQuery::Runtime(_) => {
                unreachable!("handled above")
            }
        };
        let mut writer = JsonWriter::new(out);
        write_project_read_result_json(&mut writer, &result)?;
        Ok(writer.into_inner())
    }

    fn write_project_probe_result_json<W>(
        &mut self,
        probe: ProjectProbeRequest,
        out: W,
    ) -> Result<W, JsonWriterError<W::Error>>
    where
        W: JsonWrite,
    {
        let result = match probe {
            ProjectProbeRequest::RenderProduct(request) => {
                ProjectProbeResult::RenderProduct(self.read_project_render_product_probe(request))
            }
            ProjectProbeRequest::ExplainSlot(request) => {
                ProjectProbeResult::ExplainSlot(self.read_project_explain_slot_probe(request))
            }
        };
        let mut writer = JsonWriter::new(out);
        writer.serde(&result)?;
        Ok(writer.into_inner())
    }
}

impl Engine {
    fn write_project_shape_read_result_json<W>(
        &mut self,
        query: ShapeReadQuery,
        out: W,
    ) -> Result<W, JsonWriterError<W::Error>>
    where
        W: JsonWrite,
    {
        let mut writer = JsonWriter::new(out);
        let mut result = writer.object()?;
        let mut shapes = result.prop("shapes")?.object()?;
        shapes.prop("level")?.serde(&query.level)?;
        write_slot_shape_registry_snapshot_json(shapes.prop("registry")?, self.slot_shapes())?;
        shapes.finish()?;
        result.finish()?;
        Ok(writer.into_inner())
    }

    pub fn write_project_runtime_read_result_json<W>(
        &mut self,
        query: RuntimeReadQuery,
        server: Option<ServerRuntimeStatus>,
        out: W,
    ) -> Result<W, JsonWriterError<W::Error>>
    where
        W: JsonWrite,
    {
        let result = ProjectReadResult::Runtime(self.read_project_runtime(query, server));
        let mut writer = JsonWriter::new(out);
        write_project_read_result_json(&mut writer, &result)?;
        Ok(writer.into_inner())
    }

    fn write_project_node_read_result_json<W>(
        &mut self,
        since: Option<lpc_model::Revision>,
        query: NodeReadQuery,
        out: W,
    ) -> Result<W, JsonWriterError<W::Error>>
    where
        W: JsonWrite,
    {
        let since = since.unwrap_or_default();
        let mut writer = JsonWriter::new(out);
        let mut result = writer.object()?;
        let mut nodes = result.prop("nodes")?.object()?;
        nodes.prop("level")?.serde(&query.level)?;

        let tree_deltas = tree_deltas_since(self.tree(), since);
        if !tree_deltas.is_empty() {
            nodes.prop("tree_deltas")?.serde(&tree_deltas)?;
        }

        if query.include_slots && query.level == lpc_wire::ReadLevel::Detail {
            let mut slots = nodes.prop("slots")?.object()?;
            let mut roots = slots.prop("roots")?.array()?;
            for entry in self.tree().entries() {
                if let Some(def) = self.loaded_node_def(entry.artifact()) {
                    let mut root = roots.item()?.object()?;
                    root.prop("name")?.string(&node_def_root_name(entry.id))?;
                    root.prop("shape")?.serde(&def.shape_id())?;
                    write_slot_data_json(
                        root.prop("data")?,
                        &def.shape_id(),
                        def.data(),
                        self.slot_shapes(),
                    )?;
                    root.finish()?;
                }

                if let NodeEntryState::Alive(node) = entry.state.value()
                    && let Some(state) = node.runtime_state_slots()
                {
                    let mut root = roots.item()?.object()?;
                    root.prop("name")?.string(&node_state_root_name(entry.id))?;
                    root.prop("shape")?.serde(&state.shape_id())?;
                    write_slot_data_json(
                        root.prop("data")?,
                        &state.shape_id(),
                        state.data(),
                        self.slot_shapes(),
                    )?;
                    root.finish()?;
                }
            }
            roots.finish()?;
            slots.finish()?;
        } else {
            nodes.prop("slots")?.null()?;
        }

        nodes.finish()?;
        result.finish()?;
        Ok(writer.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;
    use lpc_model::{Revision, TreePath, WithRevision};
    use lpc_wire::json::json_write::ChunkCountingWrite;
    use lpc_wire::{ProjectReadResponse, ResourcePayloadRead, ResourceReadQuery};

    use crate::engine::test_support::EngineTestBuilder;
    use crate::resource::RuntimeBuffer;

    #[test]
    fn streaming_project_read_matches_full_debug_response() {
        let mut h = EngineTestBuilder::new().output_node("output").build();
        let request = ProjectReadRequest::default_debug(None);

        assert_streams_to_full_response(&mut h.engine, request);
    }

    #[test]
    fn streaming_project_read_matches_resource_payload_response() {
        let mut engine = Engine::new(TreePath::parse("/basic.project").unwrap());
        engine.runtime_buffers_mut().insert(WithRevision::new(
            Revision::new(1),
            RuntimeBuffer::raw(vec![1, 2, 3, 253, 254, 255]),
        ));
        let mut request = ProjectReadRequest::default_debug(None);
        request.queries[2] = ProjectReadQuery::Resources(ResourceReadQuery {
            level: lpc_wire::ReadLevel::Detail,
            payloads: ResourcePayloadRead::All,
        });

        assert_streams_to_full_response(&mut engine, request);
    }

    #[test]
    fn streaming_project_read_writes_multiple_chunks() {
        let mut engine = Engine::new(TreePath::parse("/basic.project").unwrap());
        engine.runtime_buffers_mut().insert(WithRevision::new(
            Revision::new(1),
            RuntimeBuffer::raw(vec![1, 2, 3, 253, 254, 255]),
        ));

        let out = engine
            .write_project_read_json(
                ProjectReadRequest::default_debug(None),
                ChunkCountingWrite::new(16),
            )
            .unwrap();
        let decoded: ProjectReadResponse = lpc_wire::json::from_slice(out.bytes()).unwrap();

        assert_eq!(decoded.results.len(), 4);
        assert!(out.chunk_count() > 1);
    }

    fn assert_streams_to_full_response(engine: &mut Engine, request: ProjectReadRequest) {
        let full = engine.read_project(request.clone());
        let streamed = engine
            .write_project_read_json(request, Vec::new())
            .expect("stream project read");
        let decoded: ProjectReadResponse =
            lpc_wire::json::from_slice(&streamed).expect("decode streamed project read");

        assert_eq!(decoded, full);

        let resources = decoded
            .results
            .iter()
            .find_map(|result| match result {
                ProjectReadResult::Resources(resources) => Some(resources),
                _ => None,
            })
            .expect("resources result");
        for payload in &resources.runtime_buffer_payloads {
            assert!(!payload.bytes.is_empty());
        }
    }
}
