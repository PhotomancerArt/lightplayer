use lpa_link::{LinkEndpointId, LinkProviderId};

use crate::{
    ActionDescriptor, ActionId, ActionMeta, ActionOrigin, ClientSession, ConnectionSession,
    DeviceId, DeviceSession, InFlightAction, ProjectSession, STUDIO_DEMO_PROJECT_ID, StudioAction,
    StudioActionKind, StudioDiagnostic, StudioEffect, StudioEvent, StudioState,
};

pub struct StudioApp {
    state: StudioState,
    next_action_id: u64,
}

impl StudioApp {
    pub fn new() -> Self {
        Self {
            state: StudioState::default(),
            next_action_id: 1,
        }
    }

    pub fn state(&self) -> &StudioState {
        &self.state
    }

    pub fn dispatch_kind(
        &mut self,
        kind: StudioActionKind,
        origin: ActionOrigin,
    ) -> Vec<StudioEffect> {
        let action_type = kind.action_type();
        let descriptor = ActionDescriptor::for_type(action_type);
        let action_id = self.next_action_id();
        let action = StudioAction::new(
            ActionMeta::new(action_id, origin, descriptor.history_policy.clone()),
            kind,
        );
        self.dispatch(action)
    }

    pub fn dispatch(&mut self, action: StudioAction) -> Vec<StudioEffect> {
        let descriptor = action.kind.descriptor();
        let mut effects = Vec::new();
        match action.kind {
            StudioActionKind::SelectLinkProvider { provider_id } => {
                self.state.link_selection.selected_provider_id = provider_id;
                self.state.link_selection.endpoints.clear();
            }
            StudioActionKind::DiscoverDevices => {
                self.mark_in_flight(action.meta.action_id, descriptor);
                effects.push(StudioEffect::DiscoverEndpoints {
                    action_id: action.meta.action_id,
                    provider_id: self.state.link_selection.selected_provider_id.clone(),
                });
            }
            StudioActionKind::ConnectDevice { endpoint_id } => {
                self.mark_in_flight(action.meta.action_id, descriptor);
                effects.push(StudioEffect::ConnectEndpoint {
                    action_id: action.meta.action_id,
                    endpoint_id,
                });
            }
            StudioActionKind::DisconnectDevice => {
                self.mark_in_flight(action.meta.action_id, descriptor);
                if let Some(session) = &self.state.device_session {
                    effects.push(StudioEffect::DisconnectSession {
                        action_id: action.meta.action_id,
                        session_id: session.session_id.clone(),
                    });
                } else {
                    self.finish_action(action.meta.action_id);
                    self.state
                        .diagnostics
                        .push(StudioDiagnostic::info("No device session is connected."));
                }
            }
            StudioActionKind::LoadDemoProject => {
                self.mark_in_flight(action.meta.action_id, descriptor);
                effects.push(StudioEffect::SeedDemoProject {
                    action_id: action.meta.action_id,
                    project_id: STUDIO_DEMO_PROJECT_ID.to_string(),
                });
            }
            StudioActionKind::RefreshStatus => {
                self.mark_in_flight(action.meta.action_id, descriptor);
                effects.push(StudioEffect::RefreshStatus {
                    action_id: action.meta.action_id,
                });
            }
            StudioActionKind::ReadProjectInventory => {
                self.mark_in_flight(action.meta.action_id, descriptor);
                if let Some(project) = &self.state.project_session {
                    effects.push(StudioEffect::ReadProjectInventory {
                        action_id: action.meta.action_id,
                        handle: project.handle,
                    });
                } else {
                    self.finish_action(action.meta.action_id);
                    self.state.diagnostics.push(StudioDiagnostic::error(
                        Some(action.meta.action_id),
                        "No project is loaded.",
                    ));
                }
            }
            StudioActionKind::SelectProjectNode { node_id } => {
                if let Some(project) = &mut self.state.project_session {
                    project.selected_node_id = node_id;
                }
            }
        }
        effects
    }

    pub fn apply_event(&mut self, event: StudioEvent) -> Vec<StudioEffect> {
        let mut effects = Vec::new();
        match event {
            StudioEvent::EndpointsDiscovered {
                action_id,
                provider_id,
                endpoints,
            } => {
                self.finish_action(action_id);
                self.state.link_selection.selected_provider_id = provider_id;
                self.state.link_selection.endpoints = endpoints;
            }
            StudioEvent::DeviceConnected {
                action_id,
                provider_id,
                endpoint_id,
                session_id,
                connection_kind,
                capabilities,
            } => {
                self.finish_action(action_id);
                let device_id = device_id_for(&provider_id, &endpoint_id);
                self.state.device_session = Some(DeviceSession {
                    device_id,
                    provider_id,
                    endpoint_id: endpoint_id.clone(),
                    session_id: session_id.clone(),
                    capabilities,
                });
                self.state.connection_session = Some(ConnectionSession {
                    endpoint_id,
                    session_id,
                    kind: connection_kind,
                });
                self.state.client_session = Some(ClientSession::connected("lp-server"));
            }
            StudioEvent::DeviceDisconnected {
                action_id,
                session_id: _,
            } => {
                self.finish_action(action_id);
                self.state.device_session = None;
                self.state.connection_session = None;
                self.state.client_session = None;
                self.state.project_session = None;
            }
            StudioEvent::DemoProjectSeeded {
                action_id,
                project_id,
            } => {
                effects.push(StudioEffect::LoadProject {
                    action_id,
                    project_id,
                });
            }
            StudioEvent::ProjectLoaded {
                action_id,
                project_id,
                handle,
            } => {
                self.state.project_session = Some(ProjectSession::new(project_id, handle));
                effects.push(StudioEffect::ReadProjectInventory { action_id, handle });
            }
            StudioEvent::ProjectInventoryRead {
                action_id,
                inventory,
            } => {
                self.finish_action(action_id);
                if let Some(project) = &mut self.state.project_session {
                    project.inventory = Some(inventory);
                }
            }
            StudioEvent::LoadedProjectsRefreshed {
                action_id,
                projects: _,
            } => {
                self.finish_action(action_id);
            }
            StudioEvent::HeartbeatReceived { heartbeat } => {
                self.state.heartbeat = Some(heartbeat);
            }
            StudioEvent::LogReceived { entry } => {
                self.state.logs.push(entry);
            }
            StudioEvent::DiagnosticRaised { diagnostic } => {
                self.state.diagnostics.push(diagnostic);
            }
            StudioEvent::ActionFailed { action_id, message } => {
                self.finish_action(action_id);
                self.state
                    .diagnostics
                    .push(StudioDiagnostic::error(Some(action_id), message));
            }
        }
        effects
    }

    fn next_action_id(&mut self) -> ActionId {
        let action_id = ActionId::new(self.next_action_id);
        self.next_action_id += 1;
        action_id
    }

    fn mark_in_flight(&mut self, action_id: ActionId, descriptor: ActionDescriptor) {
        self.state.in_flight.push(InFlightAction::new(
            action_id,
            descriptor.action_type,
            descriptor.label,
        ));
    }

    fn finish_action(&mut self, action_id: ActionId) {
        self.state
            .in_flight
            .retain(|in_flight| in_flight.action_id != action_id);
    }
}

impl Default for StudioApp {
    fn default() -> Self {
        Self::new()
    }
}

fn device_id_for(provider_id: &LinkProviderId, endpoint_id: &LinkEndpointId) -> DeviceId {
    DeviceId::new(format!("{}:{}", provider_id.as_str(), endpoint_id.as_str()))
}

#[cfg(test)]
mod tests {
    use lpa_link::{
        LinkConnectionKind, LinkEndpoint, LinkEndpointId, LinkProviderId, LinkSessionId,
    };
    use lpc_wire::{WireProjectHandle, WireProjectInventoryReadResponse};

    use crate::{ActionOrigin, BROWSER_WORKER_PROVIDER_ID, DeviceCapability};

    use super::*;

    #[test]
    fn discover_devices_produces_effect_and_tracks_in_flight() {
        let mut app = StudioApp::new();

        let effects = app.dispatch_kind(StudioActionKind::DiscoverDevices, ActionOrigin::User);

        assert_eq!(effects.len(), 1);
        assert!(matches!(effects[0], StudioEffect::DiscoverEndpoints { .. }));
        assert_eq!(app.state().in_flight.len(), 1);
    }

    #[test]
    fn discovered_endpoints_update_state_and_finish_action() {
        let mut app = StudioApp::new();
        let action_id = ActionId::new(7);
        let endpoint = LinkEndpoint::new(
            "browser-worker-worker-1",
            BROWSER_WORKER_PROVIDER_ID,
            "Browser runtime",
        );

        app.apply_event(StudioEvent::EndpointsDiscovered {
            action_id,
            provider_id: LinkProviderId::new(BROWSER_WORKER_PROVIDER_ID),
            endpoints: vec![endpoint.clone()],
        });

        assert_eq!(app.state().link_selection.endpoints, vec![endpoint]);
    }

    #[test]
    fn device_connected_populates_sessions() {
        let mut app = StudioApp::new();

        app.apply_event(StudioEvent::DeviceConnected {
            action_id: ActionId::new(1),
            provider_id: LinkProviderId::new(BROWSER_WORKER_PROVIDER_ID),
            endpoint_id: LinkEndpointId::new("browser-worker-worker-1"),
            session_id: LinkSessionId::new("session-1"),
            connection_kind: LinkConnectionKind::BrowserWorker {
                protocol: "fw-browser-post-message-v1".to_string(),
            },
            capabilities: vec![DeviceCapability::Connect],
        });

        assert!(app.state().device_session.is_some());
        assert!(app.state().connection_session.is_some());
        assert!(app.state().client_session.is_some());
    }

    #[test]
    fn demo_seed_event_loads_project_and_project_load_reads_inventory() {
        let mut app = StudioApp::new();
        let action_id = ActionId::new(9);

        let load_effects = app.apply_event(StudioEvent::DemoProjectSeeded {
            action_id,
            project_id: STUDIO_DEMO_PROJECT_ID.to_string(),
        });

        assert!(matches!(load_effects[0], StudioEffect::LoadProject { .. }));

        let read_effects = app.apply_event(StudioEvent::ProjectLoaded {
            action_id,
            project_id: STUDIO_DEMO_PROJECT_ID.to_string(),
            handle: WireProjectHandle::new(3),
        });

        assert!(matches!(
            read_effects[0],
            StudioEffect::ReadProjectInventory { .. }
        ));
        assert!(app.state().project_session.is_some());
    }

    #[test]
    fn inventory_event_updates_project_session() {
        let mut app = StudioApp::new();
        let action_id = ActionId::new(4);
        app.apply_event(StudioEvent::ProjectLoaded {
            action_id,
            project_id: STUDIO_DEMO_PROJECT_ID.to_string(),
            handle: WireProjectHandle::new(1),
        });

        app.apply_event(StudioEvent::ProjectInventoryRead {
            action_id,
            inventory: WireProjectInventoryReadResponse::default(),
        });

        assert!(
            app.state()
                .project_session
                .as_ref()
                .and_then(|project| project.inventory.as_ref())
                .is_some()
        );
    }

    #[test]
    fn node_selection_is_state_only() {
        let mut app = StudioApp::new();
        app.apply_event(StudioEvent::ProjectLoaded {
            action_id: ActionId::new(1),
            project_id: STUDIO_DEMO_PROJECT_ID.to_string(),
            handle: WireProjectHandle::new(1),
        });

        let effects = app.dispatch_kind(
            StudioActionKind::SelectProjectNode {
                node_id: Some("node-a".to_string()),
            },
            ActionOrigin::User,
        );

        assert!(effects.is_empty());
        assert_eq!(
            app.state()
                .project_session
                .as_ref()
                .and_then(|project| project.selected_node_id.as_deref()),
            Some("node-a")
        );
    }
}
