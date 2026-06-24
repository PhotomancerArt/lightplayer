#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UxLogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UxLogEntry {
    pub level: UxLogLevel,
    pub source: String,
    pub message: String,
}

impl UxLogEntry {
    pub fn new(level: UxLogLevel, source: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level,
            source: source.into(),
            message: message.into(),
        }
    }
}
