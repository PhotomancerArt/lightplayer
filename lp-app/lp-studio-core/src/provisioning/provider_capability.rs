use serde::{Deserialize, Serialize};

/// Studio-facing provider capability before a concrete device session exists.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum ProviderCapability {
    RequestAccess,
    DiscoverEndpoints,
    Connect,
    Simulate,
    ResetDevice,
    FlashFirmware,
    ReadLogs,
    ReadDiagnostics,
    ReadHeartbeat,
    DeployProject,
    ReadProjectInventory,
    DirectFilesystem,
}
