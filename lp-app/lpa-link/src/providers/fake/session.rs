use crate::{
    LinkConnection, LinkDiagnostic, LinkDiagnosticSeverity, LinkEndpointId, LinkError,
    LinkLogEntry, LinkLogLevel, LinkSession, LinkSessionId,
};

#[derive(Clone, Debug)]
pub struct FakeSession {
    endpoint_id: LinkEndpointId,
    id: LinkSessionId,
    closed: bool,
    logs: Vec<LinkLogEntry>,
    diagnostics: Vec<LinkDiagnostic>,
}

impl FakeSession {
    pub fn new(endpoint_id: LinkEndpointId, id: LinkSessionId) -> Self {
        let logs = vec![LinkLogEntry::new(
            endpoint_id.clone(),
            Some(id.clone()),
            LinkLogLevel::Info,
            "fake link session opened",
        )];
        let diagnostics = vec![LinkDiagnostic::new(
            endpoint_id.clone(),
            Some(id.clone()),
            LinkDiagnosticSeverity::Info,
            "fake link session ready",
        )];

        Self {
            endpoint_id,
            id,
            closed: false,
            logs,
            diagnostics,
        }
    }
}

impl LinkSession for FakeSession {
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
        if self.closed {
            return Err(LinkError::Closed);
        }

        Ok(LinkConnection::fake(
            self.endpoint_id.clone(),
            self.id.clone(),
        ))
    }

    async fn close(&mut self) -> Result<(), LinkError> {
        self.closed = true;
        self.logs.push(LinkLogEntry::new(
            self.endpoint_id.clone(),
            Some(self.id.clone()),
            LinkLogLevel::Info,
            "fake link session closed",
        ));
        Ok(())
    }
}
