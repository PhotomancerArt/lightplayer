use serde::{Deserialize, Serialize};

use crate::provider::endpoint::LinkEndpointId;
use crate::provider::session::LinkSessionId;

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct LinkLogEntry {
    pub endpoint_id: LinkEndpointId,
    pub session_id: Option<LinkSessionId>,
    pub level: LinkLogLevel,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum LinkLogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LinkLogEntry {
    pub fn new(
        endpoint_id: impl Into<LinkEndpointId>,
        session_id: Option<LinkSessionId>,
        level: LinkLogLevel,
        message: impl Into<String>,
    ) -> Self {
        Self {
            endpoint_id: endpoint_id.into(),
            session_id,
            level,
            message: message.into(),
        }
    }
}
