use lpa_link::{LinkConnectionKind, LinkEndpointId, LinkSessionId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ConnectionSession {
    pub endpoint_id: LinkEndpointId,
    pub session_id: LinkSessionId,
    pub kind: LinkConnectionKind,
}
