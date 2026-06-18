use serde::{Deserialize, Serialize};

use crate::{
    ClientSession, ConnectionSession, DeviceAccess, DeviceSession, InFlightAction, LinkSelection,
    ProjectSession, StudioDiagnostic, StudioHeartbeat, StudioLogEntry,
};

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
pub struct StudioState {
    pub link_selection: LinkSelection,
    pub device_access: Option<DeviceAccess>,
    pub device_session: Option<DeviceSession>,
    pub connection_session: Option<ConnectionSession>,
    pub client_session: Option<ClientSession>,
    pub project_session: Option<ProjectSession>,
    pub heartbeat: Option<StudioHeartbeat>,
    pub logs: Vec<StudioLogEntry>,
    pub diagnostics: Vec<StudioDiagnostic>,
    pub in_flight: Vec<InFlightAction>,
}
