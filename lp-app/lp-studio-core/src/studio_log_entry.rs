use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum StudioLogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct StudioLogEntry {
    pub level: StudioLogLevel,
    pub target: String,
    pub message: String,
}

impl StudioLogEntry {
    pub fn new(
        level: StudioLogLevel,
        target: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            level,
            target: target.into(),
            message: message.into(),
        }
    }
}
