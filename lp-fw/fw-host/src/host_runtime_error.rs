use std::fmt::{self, Display};

#[derive(Debug)]
pub enum HostRuntimeError {
    SpawnFailed(std::io::Error),
    RuntimeCreateFailed(std::io::Error),
    ServerThreadPanicked,
    ServerThreadStopTimedOut,
    Transport(String),
}

impl Display for HostRuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SpawnFailed(error) => write!(f, "failed to spawn host runtime thread: {error}"),
            Self::RuntimeCreateFailed(error) => {
                write!(f, "failed to create host runtime tokio runtime: {error}")
            }
            Self::ServerThreadPanicked => f.write_str("host runtime thread panicked"),
            Self::ServerThreadStopTimedOut => f.write_str("host runtime thread did not stop"),
            Self::Transport(error) => write!(f, "host runtime transport error: {error}"),
        }
    }
}

impl std::error::Error for HostRuntimeError {}
