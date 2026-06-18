use serde::{Deserialize, Serialize};

/// Product-level reason a provider appears in the Studio device manager.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum ProviderIntent {
    SimulateInBrowser,
    ConnectUsbEsp32,
    RunHostRuntime,
    ConnectHostSerialEsp32,
    ConnectRemoteServer,
    Other { label: String },
}
