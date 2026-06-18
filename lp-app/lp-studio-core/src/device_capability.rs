use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum DeviceCapability {
    Connect,
    UseBrowserWorker,
    UseHostProcess,
    ReadHeartbeat,
    ListProjects,
    LoadProject,
    ReadProjectInventory,
    ReadLogs,
    ReadDiagnostics,
}
