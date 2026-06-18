use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ClientSession {
    pub connected: bool,
    pub label: String,
}

impl ClientSession {
    pub fn connected(label: impl Into<String>) -> Self {
        Self {
            connected: true,
            label: label.into(),
        }
    }
}
