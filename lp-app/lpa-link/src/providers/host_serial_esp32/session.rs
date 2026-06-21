use crate::{
    LinkConnection, LinkDiagnostic, LinkDiagnosticSeverity, LinkEndpointId, LinkError,
    LinkLogEntry, LinkLogLevel, LinkServerConnection, LinkSession, LinkSessionId,
};

pub struct HostSerialEsp32Session {
    endpoint_id: LinkEndpointId,
    id: LinkSessionId,
    port_name: String,
    baud_rate: u32,
    server_connection: Option<LinkServerConnection>,
    logs: Vec<LinkLogEntry>,
    diagnostics: Vec<LinkDiagnostic>,
    closed: bool,
}

impl HostSerialEsp32Session {
    pub fn new(
        endpoint_id: LinkEndpointId,
        id: LinkSessionId,
        port_name: String,
        baud_rate: u32,
        server_connection: LinkServerConnection,
    ) -> Self {
        let logs = vec![LinkLogEntry::new(
            endpoint_id.clone(),
            Some(id.clone()),
            LinkLogLevel::Info,
            format!("host serial ESP32 session opened on {port_name}"),
        )];
        let diagnostics = vec![LinkDiagnostic::new(
            endpoint_id.clone(),
            Some(id.clone()),
            LinkDiagnosticSeverity::Info,
            format!("host serial ESP32 transport ready at {baud_rate} baud"),
        )];
        Self {
            endpoint_id,
            id,
            port_name,
            baud_rate,
            server_connection: Some(server_connection),
            logs,
            diagnostics,
            closed: false,
        }
    }
}

impl LinkSession for HostSerialEsp32Session {
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
        let Some(server_connection) = &self.server_connection else {
            return Err(LinkError::Closed);
        };
        Ok(LinkConnection::host_serial_esp32(
            self.endpoint_id.clone(),
            self.id.clone(),
            server_connection.clone(),
        ))
    }

    async fn close(&mut self) -> Result<(), LinkError> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        if let Some(server_connection) = self.server_connection.take() {
            let mut transport = server_connection.lock().await;
            lpa_client::ClientTransport::close(&mut **transport)
                .await
                .map_err(|error| LinkError::other(error.to_string()))?;
        }
        self.logs.push(LinkLogEntry::new(
            self.endpoint_id.clone(),
            Some(self.id.clone()),
            LinkLogLevel::Info,
            format!(
                "host serial ESP32 session closed on {} at {} baud",
                self.port_name, self.baud_rate
            ),
        ));
        Ok(())
    }
}
