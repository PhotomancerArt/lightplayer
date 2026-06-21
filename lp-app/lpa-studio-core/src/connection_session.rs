use lpa_link::LinkConnectionKind;
use lpa_link::provider::endpoint::LinkEndpointId;
use lpa_link::provider::session::LinkSessionId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ConnectionSession {
    pub endpoint_id: LinkEndpointId,
    pub session_id: LinkSessionId,
    pub kind: LinkConnectionKind,
}
