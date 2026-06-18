use std::fmt::{self, Display};

#[derive(Debug)]
pub enum StudioRuntimeError {
    Link(String),
    Transport(String),
    Protocol(String),
    MissingClient,
    MissingSession,
    UnsupportedProvider(String),
    Browser(String),
}

impl Display for StudioRuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Link(message) => write!(f, "link error: {message}"),
            Self::Transport(message) => write!(f, "transport error: {message}"),
            Self::Protocol(message) => write!(f, "protocol error: {message}"),
            Self::MissingClient => f.write_str("no Studio client session is connected"),
            Self::MissingSession => f.write_str("no Studio device session is connected"),
            Self::UnsupportedProvider(provider) => {
                write!(f, "unsupported Studio runtime provider: {provider}")
            }
            Self::Browser(message) => write!(f, "browser runtime error: {message}"),
        }
    }
}

impl std::error::Error for StudioRuntimeError {}
