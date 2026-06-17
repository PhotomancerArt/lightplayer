use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum LinkEndpointStatus {
    Available,
    Launching,
    Connected,
    InUse,
    Unavailable { reason: String },
    Error { message: String },
}
