use lp_studio_core::{DeviceCapability, DeviceIssue};
use lpa_link::{LinkConnectionKind, LinkSessionId};
use serde::{Deserialize, Serialize};

/// Scripted result of opening a link endpoint.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum ConnectOutcome {
    Connected {
        session_id: LinkSessionId,
        connection_kind: LinkConnectionKind,
        capabilities: Vec<DeviceCapability>,
    },
    Failed {
        issue: DeviceIssue,
    },
}

impl ConnectOutcome {
    pub fn connected(
        session_id: impl Into<LinkSessionId>,
        connection_kind: LinkConnectionKind,
        capabilities: Vec<DeviceCapability>,
    ) -> Self {
        Self::Connected {
            session_id: session_id.into(),
            connection_kind,
            capabilities,
        }
    }

    pub fn failed(issue: DeviceIssue) -> Self {
        Self::Failed { issue }
    }
}
