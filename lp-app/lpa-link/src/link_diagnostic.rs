use serde::{Deserialize, Serialize};

use crate::link_endpoint::LinkEndpointId;
use crate::link_session::LinkSessionId;

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct LinkDiagnostic {
    pub endpoint_id: LinkEndpointId,
    pub session_id: Option<LinkSessionId>,
    pub severity: LinkDiagnosticSeverity,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum LinkDiagnosticSeverity {
    Info,
    Warning,
    Error,
}

impl LinkDiagnostic {
    pub fn new(
        endpoint_id: impl Into<LinkEndpointId>,
        session_id: Option<LinkSessionId>,
        severity: LinkDiagnosticSeverity,
        message: impl Into<String>,
    ) -> Self {
        Self {
            endpoint_id: endpoint_id.into(),
            session_id,
            severity,
            message: message.into(),
        }
    }
}
