//! Server-decorated project-read source.

extern crate alloc;

use alloc::vec::Vec;
use lpc_engine::Engine;
use lpc_shared::transport::ProjectReadJsonSource;
use lpc_wire::json::json_write::JsonWrite;
use lpc_wire::json::json_writer::{JsonWriter, JsonWriterError};
use lpc_wire::{
    ProjectProbeRequest, ProjectReadQuery, ProjectReadResult, RuntimeReadQuery,
    ServerRuntimeStatus, WireSlotMutationRequest, WireSlotMutationResponse, WireSlotMutationResult,
    write_project_read_result_json,
};

/// Project-read source that adds server-loop status to runtime queries.
pub(crate) struct ServerProjectReadSource<'a> {
    engine: &'a mut Engine,
    server_status: Option<ServerRuntimeStatus>,
}

impl<'a> ServerProjectReadSource<'a> {
    pub(crate) fn new(engine: &'a mut Engine, server_status: Option<ServerRuntimeStatus>) -> Self {
        Self {
            engine,
            server_status,
        }
    }
}

impl ProjectReadJsonSource for ServerProjectReadSource<'_> {
    fn project_read_revision(&self) -> lpc_model::Revision {
        self.engine.revision()
    }

    fn apply_project_mutations(
        &mut self,
        mutations: Vec<WireSlotMutationRequest>,
    ) -> Vec<WireSlotMutationResponse> {
        log_project_mutations(&mutations);
        let responses = self.engine.mutate_project_slots(mutations);
        log_project_mutation_responses(&responses);
        responses
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
            ProjectReadQuery::Runtime(query) => self.write_project_runtime_result_json(query, out),
            other => ProjectReadJsonSource::write_project_read_result_json(
                self.engine,
                since,
                other,
                out,
            ),
        }
    }

    fn write_project_probe_result_json<W>(
        &mut self,
        probe: ProjectProbeRequest,
        out: W,
    ) -> Result<W, JsonWriterError<W::Error>>
    where
        W: JsonWrite,
    {
        ProjectReadJsonSource::write_project_probe_result_json(self.engine, probe, out)
    }
}

fn log_project_mutations(mutations: &[WireSlotMutationRequest]) {
    if mutations.is_empty() {
        return;
    }
    log::info!("received {} project slot mutation(s)", mutations.len());
    for mutation in mutations {
        log::info!(
            "slot mutation id={} root={} path={} op={:?}",
            mutation.id.id(),
            mutation.root,
            mutation.path,
            mutation.op,
        );
        log::info!(
            "slot mutation id={} expected shape_rev={} data_rev={}",
            mutation.id.id(),
            mutation.expected_shape_version.0,
            mutation.expected_data_version.0,
        );
    }
}

fn log_project_mutation_responses(responses: &[WireSlotMutationResponse]) {
    for response in responses {
        match &response.result {
            WireSlotMutationResult::Accepted => {
                log::info!("slot mutation id={} accepted", response.id.id());
            }
            WireSlotMutationResult::Rejected(rejection) => {
                log::warn!(
                    "slot mutation id={} rejected: {:?}",
                    response.id.id(),
                    rejection,
                );
            }
        }
    }
}

impl ServerProjectReadSource<'_> {
    fn write_project_runtime_result_json<W>(
        &mut self,
        query: RuntimeReadQuery,
        out: W,
    ) -> Result<W, JsonWriterError<W::Error>>
    where
        W: JsonWrite,
    {
        let result = ProjectReadResult::Runtime(
            self.engine
                .read_project_runtime(query, self.server_status.clone()),
        );
        let mut writer = JsonWriter::new(out);
        write_project_read_result_json(&mut writer, &result)?;
        Ok(writer.into_inner())
    }
}
