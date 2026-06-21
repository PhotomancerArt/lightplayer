use crate::{
    LinkConnection, LinkDiagnostic, LinkDiagnosticSeverity, LinkEndpointId, LinkError,
    LinkLogEntry, LinkLogLevel, LinkSession, LinkSessionId,
};
use fw_host::HostRuntime;

pub struct HostProcessSession {
    endpoint_id: LinkEndpointId,
    id: LinkSessionId,
    runtime: HostRuntime,
    logs: Vec<LinkLogEntry>,
    diagnostics: Vec<LinkDiagnostic>,
}

impl HostProcessSession {
    pub fn new(endpoint_id: LinkEndpointId, id: LinkSessionId, runtime: HostRuntime) -> Self {
        let logs = vec![LinkLogEntry::new(
            endpoint_id.clone(),
            Some(id.clone()),
            LinkLogLevel::Info,
            "host process runtime started",
        )];
        let diagnostics = vec![LinkDiagnostic::new(
            endpoint_id.clone(),
            Some(id.clone()),
            LinkDiagnosticSeverity::Info,
            "host process runtime ready",
        )];

        Self {
            endpoint_id,
            id,
            runtime,
            logs,
            diagnostics,
        }
    }
}

impl LinkSession for HostProcessSession {
    fn id(&self) -> &LinkSessionId {
        &self.id
    }

    fn endpoint_id(&self) -> &LinkEndpointId {
        &self.endpoint_id
    }

    fn logs(&self) -> Vec<LinkLogEntry> {
        self.logs.clone()
    }

    fn diagnostics(&self) -> Vec<LinkDiagnostic> {
        self.diagnostics.clone()
    }

    async fn connection(&mut self) -> Result<LinkConnection, LinkError> {
        Ok(LinkConnection::host_process(
            self.endpoint_id.clone(),
            self.id.clone(),
            self.runtime.client_transport(),
        ))
    }

    async fn close(&mut self) -> Result<(), LinkError> {
        self.runtime
            .close()
            .await
            .map_err(|error| LinkError::Other {
                message: error.to_string(),
            })?;
        self.logs.push(LinkLogEntry::new(
            self.endpoint_id.clone(),
            Some(self.id.clone()),
            LinkLogLevel::Info,
            "host process runtime stopped",
        ));
        Ok(())
    }
}
