use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct LinkEndpointId(String);

impl LinkEndpointId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for LinkEndpointId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for LinkEndpointId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}
