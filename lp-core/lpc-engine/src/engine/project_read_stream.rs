//! Streaming project-read response writer for [`Engine`].

use lpc_wire::json::json_write::JsonWrite;
use lpc_wire::json::json_writer::{JsonWriter, JsonWriterError};
use lpc_wire::{
    ProjectProbeRequest, ProjectProbeResult, ProjectReadQuery, ProjectReadRequest,
    ProjectReadResult, ShapeReadQuery, write_project_read_result_json,
    write_slot_shape_registry_snapshot_json,
};

use super::Engine;

impl Engine {
    /// Write one stateless project read response directly to a JSON sink.
    ///
    /// This preserves the same JSON shape as [`Self::read_project`], but writes
    /// each query/probe result as soon as it is produced. The current
    /// implementation may still allocate individual result objects; it avoids
    /// allocating the whole response envelope and uses streaming base64 for
    /// runtime-buffer payload fields.
    pub fn write_project_read_json<W>(
        &self,
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

    fn write_project_read_result_json<W>(
        &self,
        since: Option<lpc_model::Revision>,
        query: ProjectReadQuery,
        out: W,
    ) -> Result<W, JsonWriterError<W::Error>>
    where
        W: JsonWrite,
    {
        if let ProjectReadQuery::Shapes(query) = query {
            return self.write_project_shape_read_result_json(query, out);
        }

        let result = match query {
            ProjectReadQuery::Nodes(query) => {
                ProjectReadResult::Nodes(self.read_project_nodes(since, query))
            }
            ProjectReadQuery::Resources(query) => {
                ProjectReadResult::Resources(self.read_project_resources(query))
            }
            ProjectReadQuery::Shapes(_) => unreachable!("handled above"),
        };
        let mut writer = JsonWriter::new(out);
        write_project_read_result_json(&mut writer, &result)?;
        Ok(writer.into_inner())
    }

    fn write_project_probe_result_json<W>(
        &self,
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
        &self,
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;
    use lpc_model::{Revision, TreePath, WithRevision};
    use lpc_wire::json::json_write::ChunkCountingWrite;
    use lpc_wire::{
        ProjectReadResponse, ResourcePayloadRead, ResourceReadQuery, ResourceReadResult,
    };

    use crate::engine::test_support::EngineTestBuilder;
    use crate::resource::RuntimeBuffer;

    #[test]
    fn streaming_project_read_matches_full_debug_response() {
        let h = EngineTestBuilder::new().output_node("output").build();
        let request = ProjectReadRequest::default_debug(None);

        assert_streams_to_full_response(&h.engine, request);
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

        assert_streams_to_full_response(&engine, request);
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

        assert_eq!(decoded.results.len(), 3);
        assert!(out.chunk_count() > 1);
    }

    fn assert_streams_to_full_response(engine: &Engine, request: ProjectReadRequest) {
        let full = engine.read_project(request.clone());
        let streamed = engine
            .write_project_read_json(request, Vec::new())
            .expect("stream project read");
        let decoded: ProjectReadResponse =
            lpc_wire::json::from_slice(&streamed).expect("decode streamed project read");

        assert_eq!(decoded, full);

        let ProjectReadResult::Resources(ResourceReadResult {
            runtime_buffer_payloads,
            ..
        }) = decoded.results.last().expect("resources result")
        else {
            panic!("last result should be resources");
        };
        for payload in runtime_buffer_payloads {
            assert!(!payload.bytes.is_empty());
        }
    }
}
