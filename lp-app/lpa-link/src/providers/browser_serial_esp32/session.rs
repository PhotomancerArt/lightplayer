use crate::{
    LinkConnection, LinkDiagnostic, LinkDiagnosticSeverity, LinkEndpointId, LinkError,
    LinkLogEntry, LinkLogLevel, LinkSession, LinkSessionId,
};

#[derive(Clone, Debug)]
pub struct BrowserSerialEsp32Session {
    endpoint_id: LinkEndpointId,
    id: LinkSessionId,
    closed: bool,
    logs: Vec<LinkLogEntry>,
    diagnostics: Vec<LinkDiagnostic>,
}

impl BrowserSerialEsp32Session {
    pub fn new(endpoint_id: LinkEndpointId, id: LinkSessionId) -> Self {
        let logs = vec![LinkLogEntry::new(
            endpoint_id.clone(),
            Some(id.clone()),
            LinkLogLevel::Info,
            "browser serial ESP32 session created",
        )];
        let diagnostics = vec![LinkDiagnostic::new(
            endpoint_id.clone(),
            Some(id.clone()),
            LinkDiagnosticSeverity::Info,
            "browser serial session modeled; Studio web owns the Web Serial stream binding",
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

impl LinkSession for BrowserSerialEsp32Session {
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
        Ok(LinkConnection::browser_serial_esp32(
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
            "browser serial ESP32 session closed",
        ));
        Ok(())
    }
}
