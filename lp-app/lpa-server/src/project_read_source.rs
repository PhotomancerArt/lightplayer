//! Server-decorated project-read source.

use lpc_engine::EngineProjectReadSource;
use lpc_shared::transport::ProjectReadJsonSource;
use lpc_wire::ServerRuntimeStatus;

use crate::project::Project;

/// Project-read source that adds server-loop status to runtime queries.
pub(crate) struct ServerProjectReadSource<'a> {
    source: EngineProjectReadSource<'a>,
}

impl<'a> ServerProjectReadSource<'a> {
    pub(crate) fn new(
        project: &'a mut Project,
        server_status: Option<ServerRuntimeStatus>,
    ) -> Self {
        let (engine, registry) = project.runtime_read_parts();
        Self {
            source: EngineProjectReadSource::with_server_status(engine, registry, server_status),
        }
    }
}

impl ProjectReadJsonSource for ServerProjectReadSource<'_> {
    fn project_read_revision(&self) -> lpc_model::Revision {
        self.source.project_read_revision()
    }

    fn write_project_read_result_json<W>(
        &mut self,
        since: Option<lpc_model::Revision>,
        query: lpc_wire::ProjectReadQuery,
        out: W,
    ) -> Result<W, lpc_wire::json::json_writer::JsonWriterError<W::Error>>
    where
        W: lpc_wire::json::json_write::JsonWrite,
    {
        self.source
            .write_project_read_result_json(since, query, out)
    }

    fn write_project_probe_result_json<W>(
        &mut self,
        probe: lpc_wire::ProjectProbeRequest,
        out: W,
    ) -> Result<W, lpc_wire::json::json_writer::JsonWriterError<W::Error>>
    where
        W: lpc_wire::json::json_write::JsonWrite,
    {
        self.source.write_project_probe_result_json(probe, out)
    }
}
