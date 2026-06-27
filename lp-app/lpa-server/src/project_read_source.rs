//! Server-decorated project-read source.

use lpc_engine::EngineProjectReadSource;
use lpc_shared::transport::ProjectReadEventSink;
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

    pub(crate) async fn stream_project_read_events<S>(
        &mut self,
        request: lpc_wire::ProjectReadRequest,
        sink: &mut S,
    ) -> Result<(), lpc_engine::ProjectReadEventStreamError<S::Error>>
    where
        S: ProjectReadEventSink,
    {
        self.source.stream_project_read_events(request, sink).await
    }
}
