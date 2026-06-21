use lpa_link::LinkConnectionKind;
use lpa_link::link_endpoint::LinkEndpointId;
use lpa_link::link_session::LinkSessionId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ConnectionSession {
    pub endpoint_id: LinkEndpointId,
    pub session_id: LinkSessionId,
    pub kind: LinkConnectionKind,
}
