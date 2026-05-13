//! Streaming JSON writer for `ProjectReadResponse`.

use lpc_model::Revision;

use crate::json::json_write::JsonWrite;
use crate::json::json_writer::{JsonWriter, JsonWriterError};
use crate::project::write_runtime_buffer_payload_json;
use crate::slot::WireSlotMutationResponse;

use super::{ProjectProbeResult, ProjectReadResponse, ProjectReadResult, ResourceReadResult};

/// Writes a `ProjectReadResponse` JSON envelope incrementally.
///
/// The emitted JSON has the same semantic shape as [`ProjectReadResponse`], but
/// callers can append results and probes one at a time instead of allocating the
/// whole response.
pub struct ProjectReadResponseWriter<W>
where
    W: JsonWrite,
{
    writer: JsonWriter<W>,
    result_count: usize,
    probe_count: usize,
    mutation_count: usize,
    in_probes: bool,
    in_mutations: bool,
    finished: bool,
}

impl<W> ProjectReadResponseWriter<W>
where
    W: JsonWrite,
{
    pub fn begin(
        mut writer: JsonWriter<W>,
        revision: Revision,
    ) -> Result<Self, JsonWriterError<W::Error>> {
        writer.write_raw(b"{\"revision\":")?;
        writer.serde(&revision)?;
        writer.write_raw(b",\"results\":[")?;
        Ok(Self {
            writer,
            result_count: 0,
            probe_count: 0,
            mutation_count: 0,
            in_probes: false,
            in_mutations: false,
            finished: false,
        })
    }

    pub fn write_result(
        &mut self,
        result: &ProjectReadResult,
    ) -> Result<(), JsonWriterError<W::Error>> {
        if self.in_probes {
            return Err(JsonWriterError::Serialize);
        }
        if self.result_count > 0 {
            self.writer.write_raw(b",")?;
        }
        write_project_read_result_json(&mut self.writer, result)?;
        self.result_count += 1;
        Ok(())
    }

    pub fn write_probe(
        &mut self,
        probe: &ProjectProbeResult,
    ) -> Result<(), JsonWriterError<W::Error>> {
        self.begin_probes()?;
        if self.probe_count > 0 {
            self.writer.write_raw(b",")?;
        }
        self.writer.serde(probe)?;
        self.probe_count += 1;
        Ok(())
    }

    pub fn write_mutation(
        &mut self,
        mutation: &WireSlotMutationResponse,
    ) -> Result<(), JsonWriterError<W::Error>> {
        self.begin_mutations()?;
        if self.mutation_count > 0 {
            self.writer.write_raw(b",")?;
        }
        self.writer.serde(mutation)?;
        self.mutation_count += 1;
        Ok(())
    }

    pub fn finish(mut self) -> Result<W, JsonWriterError<W::Error>> {
        self.begin_mutations()?;
        self.writer.write_raw(b"]}")?;
        self.finished = true;
        Ok(self.writer.into_inner())
    }

    fn begin_probes(&mut self) -> Result<(), JsonWriterError<W::Error>> {
        if !self.in_probes {
            self.writer.write_raw(b"],\"probes\":[")?;
            self.in_probes = true;
        }
        Ok(())
    }

    fn begin_mutations(&mut self) -> Result<(), JsonWriterError<W::Error>> {
        if !self.in_mutations {
            self.begin_probes()?;
            self.writer.write_raw(b"],\"mutations\":[")?;
            self.in_mutations = true;
        }
        Ok(())
    }
}

/// Write one [`ProjectReadResult`] in its externally tagged JSON form.
pub fn write_project_read_result_json<W>(
    writer: &mut JsonWriter<W>,
    result: &ProjectReadResult,
) -> Result<(), JsonWriterError<W::Error>>
where
    W: JsonWrite,
{
    match result {
        ProjectReadResult::Resources(resources) => {
            let mut object = writer.object()?;
            write_resource_read_result_json(object.prop("resources")?, resources)?;
            object.finish()
        }
        _ => writer.serde(result),
    }
}

fn write_resource_read_result_json<W>(
    value: crate::json::json_writer::JsonValue<'_, W>,
    resources: &ResourceReadResult,
) -> Result<(), JsonWriterError<W::Error>>
where
    W: JsonWrite,
{
    let mut object = value.object()?;
    object.prop("level")?.serde(&resources.level)?;

    let mut summaries = object.prop("summaries")?.array()?;
    for summary in &resources.summaries {
        summaries.item()?.serde(summary)?;
    }
    summaries.finish()?;

    let mut payloads = object.prop("runtime_buffer_payloads")?.array()?;
    for payload in &resources.runtime_buffer_payloads {
        write_runtime_buffer_payload_json(payloads.item()?, payload)?;
    }
    payloads.finish()?;

    object.finish()
}

/// Serialize a project-read response with the streaming envelope writer.
pub fn write_project_read_response<W>(
    writer: JsonWriter<W>,
    response: &ProjectReadResponse,
) -> Result<W, JsonWriterError<W::Error>>
where
    W: JsonWrite,
{
    let mut streamed = ProjectReadResponseWriter::begin(writer, response.revision)?;
    for result in &response.results {
        streamed.write_result(result)?;
    }
    for probe in &response.probes {
        streamed.write_probe(probe)?;
    }
    for mutation in &response.mutations {
        streamed.write_mutation(mutation)?;
    }
    streamed.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::json_write::ChunkCountingWrite;
    use crate::messages::{ReadLevel, ShapeReadResult};
    use crate::project::{WireRuntimeBufferMetadataPayload, WireRuntimeBufferPayload};
    use alloc::vec;
    use alloc::vec::Vec;
    use lpc_model::{ResourceRef, RuntimeBufferId};

    #[test]
    fn streamed_empty_project_read_response_deserializes() {
        let response = ProjectReadResponse {
            revision: Revision::new(12),
            results: Vec::new(),
            probes: Vec::new(),
            mutations: Vec::new(),
        };

        assert_streams_to_same_response(&response);
    }

    #[test]
    fn streamed_project_read_response_with_results_deserializes() {
        let response = ProjectReadResponse {
            revision: Revision::new(12),
            results: vec![ProjectReadResult::Shapes(ShapeReadResult {
                level: ReadLevel::Ids,
                registry: None,
            })],
            probes: Vec::new(),
            mutations: Vec::new(),
        };

        assert_streams_to_same_response(&response);
    }

    #[test]
    fn streamed_project_read_response_writes_chunks() {
        let response = ProjectReadResponse {
            revision: Revision::new(12),
            results: vec![ProjectReadResult::Shapes(ShapeReadResult {
                level: ReadLevel::Ids,
                registry: None,
            })],
            probes: Vec::new(),
            mutations: Vec::new(),
        };
        let out =
            write_project_read_response(JsonWriter::new(ChunkCountingWrite::new(8)), &response)
                .unwrap();
        let decoded: ProjectReadResponse = serde_json::from_slice(out.bytes()).unwrap();

        assert_eq!(decoded, response);
        assert!(out.chunk_count() > 1);
    }

    #[test]
    fn streamed_project_read_response_streams_resource_payload_bytes() {
        let response = ProjectReadResponse {
            revision: Revision::new(12),
            results: vec![ProjectReadResult::Resources(ResourceReadResult {
                level: ReadLevel::Detail,
                summaries: Vec::new(),
                runtime_buffer_payloads: vec![WireRuntimeBufferPayload {
                    resource_ref: ResourceRef::runtime_buffer(RuntimeBufferId::new(7)),
                    revision: Revision::new(11),
                    metadata: WireRuntimeBufferMetadataPayload::Raw,
                    bytes: vec![0, 1, 2, 253, 254, 255],
                }],
            })],
            probes: Vec::new(),
            mutations: Vec::new(),
        };

        assert_streams_to_same_response(&response);
    }

    fn assert_streams_to_same_response(response: &ProjectReadResponse) {
        let out = write_project_read_response(JsonWriter::new(Vec::new()), response).unwrap();
        let streamed: ProjectReadResponse = serde_json::from_slice(&out).unwrap();
        let normal: ProjectReadResponse =
            serde_json::from_str(&serde_json::to_string(response).unwrap()).unwrap();

        assert_eq!(streamed, normal);
    }
}
