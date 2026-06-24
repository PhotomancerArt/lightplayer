#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiLogEntry {
    pub level: UiLogLevel,
    pub source: String,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiLogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl UiLogEntry {
    pub fn new(level: UiLogLevel, source: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level,
            source: source.into(),
            message: message.into(),
        }
    }
}
