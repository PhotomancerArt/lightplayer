use serde::{Deserialize, Serialize};

/// Why Studio should enter a recovery-oriented flow instead of attaching normally.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum RecoveryReason {
    SafeMode {
        message: Option<String>,
    },
    ProjectCrash {
        project_id: Option<String>,
        message: Option<String>,
    },
    BootLoopDetected {
        message: Option<String>,
    },
    FirmwarePanic {
        message: Option<String>,
    },
}
