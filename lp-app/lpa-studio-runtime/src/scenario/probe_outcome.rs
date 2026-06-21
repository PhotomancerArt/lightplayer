use lpa_studio_core::DeviceIssue;
use serde::{Deserialize, Serialize};

/// Scripted result of probing what is running behind an endpoint.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum ProbeOutcome {
    Server {
        version: Option<String>,
    },
    Bootloader,
    Blank,
    Unsupported {
        issue: DeviceIssue,
    },
    Timeout {
        issue: DeviceIssue,
    },
    IncompatibleFirmware {
        version: Option<String>,
        issue: DeviceIssue,
    },
}

impl ProbeOutcome {
    pub fn server(version: impl Into<Option<String>>) -> Self {
        Self::Server {
            version: version.into(),
        }
    }

    pub fn unsupported(issue: DeviceIssue) -> Self {
        Self::Unsupported { issue }
    }

    pub fn timeout(issue: DeviceIssue) -> Self {
        Self::Timeout { issue }
    }

    pub fn incompatible_firmware(version: Option<String>, issue: DeviceIssue) -> Self {
        Self::IncompatibleFirmware { version, issue }
    }
}
