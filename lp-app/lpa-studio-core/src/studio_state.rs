use serde::{Deserialize, Serialize};

use crate::{
    AvailableAction, ClientSession, ConnectionSession, DeviceAccess, DeviceManagerState,
    DeviceSession, InFlightAction, ProjectSession, ProjectState, ServerState, StudioActionKind,
    StudioDiagnostic, StudioHeartbeat, StudioLogEntry,
};

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
pub struct StudioState {
    pub device_manager: DeviceManagerState,
    pub server: ServerState,
    pub project: ProjectState,
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

impl StudioState {
    pub fn available_actions(&self) -> Vec<AvailableAction<StudioActionKind>> {
        let mut actions = Vec::new();
        actions.extend(
            self.device_manager
                .available_actions()
                .into_iter()
                .map(|action| action.map_action(StudioActionKind::from)),
        );
        actions.extend(
            self.server
                .available_actions()
                .into_iter()
                .map(|action| action.map_action(StudioActionKind::from)),
        );
        actions.extend(
            self.project
                .available_actions()
                .into_iter()
                .map(|action| action.map_action(StudioActionKind::from)),
        );
        actions
    }
}
