use core::fmt;

pub type UxResult = Result<crate::UxOutcome, UxError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UxError {
    UnsupportedFeature(String),
    UnsupportedAction(String),
    MissingSession(String),
    Link(String),
    Project(String),
    Transport(String),
    Protocol(String),
    Browser(String),
    NoFirmwareDetected(String),
}

impl UxError {
    pub fn message(&self) -> &str {
        match self {
            Self::UnsupportedFeature(message)
            | Self::UnsupportedAction(message)
            | Self::MissingSession(message)
            | Self::Link(message)
            | Self::Project(message)
            | Self::Transport(message)
            | Self::Protocol(message)
            | Self::Browser(message)
            | Self::NoFirmwareDetected(message) => message,
        }
    }
}

impl fmt::Display for UxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedFeature(message) => write!(f, "unsupported feature: {message}"),
            Self::UnsupportedAction(message) => write!(f, "unsupported action: {message}"),
            Self::MissingSession(message) => write!(f, "missing session: {message}"),
            Self::Link(message) => write!(f, "link error: {message}"),
            Self::Project(message) => write!(f, "project error: {message}"),
            Self::Transport(message) => write!(f, "transport error: {message}"),
            Self::Protocol(message) => write!(f, "protocol error: {message}"),
            Self::Browser(message) => write!(f, "browser error: {message}"),
            Self::NoFirmwareDetected(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for UxError {}
