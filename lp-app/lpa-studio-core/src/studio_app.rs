use crate::{
    ActionDescriptor, ActionId, ActionMeta, ActionOrigin, ClientSession, ConnectedDeviceState,
    ConnectionSession, DeviceAccess, DeviceAccessStatus, DeviceId, DeviceIssue, DeviceIssueKind,
    DeviceSession, InFlightAction, LinkActionRequest, LinkState, ProgressState,
    ProjectActionRequest, ProjectSelectionReason, ProjectSession, ProjectState, ProjectStateResult,
    ProjectSyncState, ProviderAvailability, RecoveryAction, ServerActionRequest,
    ServerState, StudioAction, StudioActionKind, StudioDiagnostic, StudioEffect,
    StudioEvent, StudioLogEntry, StudioLogLevel, StudioState, TargetKind, STUDIO_DEMO_PROJECT_ID,
};
use lpa_link::link_endpoint::LinkEndpointId;
use lpa_link::link_provider::LinkProviderId;
use lpa_link::link_session::LinkSessionId;

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
            StudioActionKind::Link(LinkActionRequest::RefreshProviderCatalog) => {
                self.mark_in_flight(action.meta.action_id, descriptor);
                effects.push(StudioEffect::RefreshProviderCatalog {
                    action_id: action.meta.action_id,
                });
            }
            StudioActionKind::Link(LinkActionRequest::StartProvisioning { provider_id }) => {
                self.mark_in_flight(action.meta.action_id, descriptor);
                self.state
                    .device_manager
                    .providers
                    .select_provider(provider_id.clone());
                let availability = self
                    .state
                    .device_manager
                    .providers
                    .provider(&provider_id)
                    .map(|provider| provider.availability.clone())
                    .unwrap_or(ProviderAvailability::Available);
                if availability.can_start() {
                    self.state.device_manager.active_flow = LinkState::RequestingAccess {
                        provider_id: provider_id.clone(),
                    };
                    match availability {
                        ProviderAvailability::AvailableWithPermission => {
                            effects.push(StudioEffect::RequestDeviceAccess {
                                action_id: action.meta.action_id,
                                provider_id,
                            });
                        }
                        _ => {
                            effects.push(StudioEffect::DiscoverEndpoints {
                                action_id: action.meta.action_id,
                                provider_id,
                            });
                        }
                    }
                } else {
                    self.finish_action(action.meta.action_id);
                    let issue = provider_unavailable_issue(
                        action.meta.action_id,
                        provider_id.clone(),
                        availability,
                    );
                    self.state.device_manager.active_flow = LinkState::AccessFailed {
                        provider_id,
                        issue: issue.clone(),
                    };
                    self.state.device_manager.push_issue(issue);
                }
            }
            StudioActionKind::Link(LinkActionRequest::CancelProvisioning) => {
                self.state.device_manager.active_flow = LinkState::ChoosingProvider;
                self.state.device_manager.providers.clear_selection();
            }
            StudioActionKind::Link(LinkActionRequest::RetryProvisioning) => {
                if let Some(provider_id) = self.selected_provider_id() {
                    self.mark_in_flight(action.meta.action_id, descriptor);
                    self.state.device_manager.active_flow = LinkState::RequestingAccess {
                        provider_id: provider_id.clone(),
                    };
                    effects.push(StudioEffect::RequestDeviceAccess {
                        action_id: action.meta.action_id,
                        provider_id,
                    });
                } else {
                    self.raise_no_provider_issue(action.meta.action_id);
                }
            }
            StudioActionKind::Link(LinkActionRequest::SelectProvider { provider_id }) => {
                self.state
                    .device_manager
                    .providers
                    .select_provider(provider_id.clone());
                self.state.device_manager.active_flow = LinkState::ChoosingProvider;
                self.state.device_access = None;
            }
            StudioActionKind::Link(LinkActionRequest::RequestDeviceAccess) => {
                if let Some(provider_id) = self.selected_provider_id() {
                    self.mark_in_flight(action.meta.action_id, descriptor);
                    self.state.device_manager.active_flow = LinkState::RequestingAccess {
                        provider_id: provider_id.clone(),
                    };
                    effects.push(StudioEffect::RequestDeviceAccess {
                        action_id: action.meta.action_id,
                        provider_id,
                    });
                } else {
                    self.raise_no_provider_issue(action.meta.action_id);
                }
            }
            StudioActionKind::Link(LinkActionRequest::DiscoverDevices) => {
                if let Some(provider_id) = self.selected_provider_id() {
                    self.mark_in_flight(action.meta.action_id, descriptor);
                    effects.push(StudioEffect::DiscoverEndpoints {
                        action_id: action.meta.action_id,
                        provider_id,
                    });
                } else {
                    self.raise_no_provider_issue(action.meta.action_id);
                }
            }
            StudioActionKind::Link(LinkActionRequest::ConnectEndpoint { endpoint_id }) => {
                self.mark_in_flight(action.meta.action_id, descriptor);
                self.state.device_manager.active_flow = LinkState::Opening {
                    endpoint_id: endpoint_id.clone(),
                };
                effects.push(StudioEffect::ConnectEndpoint {
                    action_id: action.meta.action_id,
                    endpoint_id,
                });
            }
            StudioActionKind::Link(LinkActionRequest::ConnectSelectedEndpoint) => {
                if let Some(endpoint_id) = self
                    .state
                    .device_manager
                    .providers
                    .first_selected_endpoint()
                    .map(|endpoint| endpoint.id.clone())
                {
                    self.mark_in_flight(action.meta.action_id, descriptor);
                    self.state.device_manager.active_flow = LinkState::Opening {
                        endpoint_id: endpoint_id.clone(),
                    };
                    effects.push(StudioEffect::ConnectEndpoint {
                        action_id: action.meta.action_id,
                        endpoint_id,
                    });
                } else {
                    self.raise_no_endpoint_issue(action.meta.action_id);
                }
            }
            StudioActionKind::Link(LinkActionRequest::ProbeTarget { endpoint_id }) => {
                let endpoint_id = endpoint_id
                    .or_else(|| {
                        self.state
                            .connection_session
                            .as_ref()
                            .map(|session| session.endpoint_id.clone())
                    })
                    .or_else(|| {
                        self.state
                            .device_manager
                            .providers
                            .first_selected_endpoint()
                            .map(|endpoint| endpoint.id.clone())
                    });
                if let Some(endpoint_id) = endpoint_id {
                    self.mark_in_flight(action.meta.action_id, descriptor);
                    self.state.device_manager.active_flow = LinkState::ProbingTarget {
                        endpoint_id: endpoint_id.clone(),
                    };
                    effects.push(StudioEffect::ProbeTarget {
                        action_id: action.meta.action_id,
                        endpoint_id,
                    });
                } else {
                    self.raise_no_endpoint_issue(action.meta.action_id);
                }
            }
            StudioActionKind::Link(LinkActionRequest::Disconnect) => {
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
            StudioActionKind::Link(LinkActionRequest::Reset) => {
                self.mark_in_flight(action.meta.action_id, descriptor);
                if let Some(session) = &self.state.device_session {
                    effects.push(StudioEffect::ResetDevice {
                        action_id: action.meta.action_id,
                        endpoint_id: session.endpoint_id.clone(),
                    });
                } else {
                    self.finish_action(action.meta.action_id);
                    self.state.diagnostics.push(StudioDiagnostic::error(
                        Some(action.meta.action_id),
                        "No device session is connected.",
                    ));
                }
            }
            StudioActionKind::Link(LinkActionRequest::ConfirmFirmwareFlash {
                endpoint_id,
                firmware_id,
            }) => {
                self.mark_in_flight(action.meta.action_id, descriptor);
                self.state.device_manager.active_flow = LinkState::Flashing {
                    endpoint_id: endpoint_id.clone(),
                    progress: Some(ProgressState::new("Preparing firmware flash")),
                };
                effects.push(StudioEffect::FlashDeviceFirmware {
                    action_id: action.meta.action_id,
                    endpoint_id,
                    firmware_id,
                });
            }
            StudioActionKind::Link(LinkActionRequest::FlashFirmware { firmware_id }) => {
                self.mark_in_flight(action.meta.action_id, descriptor);
                if let Some(session) = &self.state.device_session {
                    effects.push(StudioEffect::FlashDeviceFirmware {
                        action_id: action.meta.action_id,
                        endpoint_id: session.endpoint_id.clone(),
                        firmware_id,
                    });
                } else {
                    self.finish_action(action.meta.action_id);
                    self.state.diagnostics.push(StudioDiagnostic::error(
                        Some(action.meta.action_id),
                        "No device session is connected.",
                    ));
                }
            }
            StudioActionKind::Project(
                ProjectActionRequest::UploadDemoProject | ProjectActionRequest::LoadDemoProject,
            ) => {
                self.mark_in_flight(action.meta.action_id, descriptor);
                self.state.project = ProjectState::Deploying;
                self.state.device_manager.active_flow = LinkState::DeployingProject {
                    project_id: STUDIO_DEMO_PROJECT_ID.to_string(),
                    progress: Some(ProgressState::new("Writing demo project")),
                };
                effects.push(StudioEffect::SeedDemoProject {
                    action_id: action.meta.action_id,
                    project_id: STUDIO_DEMO_PROJECT_ID.to_string(),
                });
            }
            StudioActionKind::Link(LinkActionRequest::AcknowledgeIssue { issue_id }) => {
                self.state.device_manager.clear_issue(&issue_id);
            }
            StudioActionKind::Server(ServerActionRequest::RefreshStatus) => {
                self.mark_in_flight(action.meta.action_id, descriptor);
                effects.push(StudioEffect::RefreshStatus {
                    action_id: action.meta.action_id,
                });
            }
            StudioActionKind::Server(ServerActionRequest::ReadProjectState) => {
                self.mark_in_flight(action.meta.action_id, descriptor);
                if let Some(session_id) = self.current_link_session_id() {
                    self.state.project = ProjectState::ReadingServerProjects;
                    self.state.device_manager.active_flow = LinkState::ReadingProjectState {
                        session_id: session_id.clone(),
                    };
                    effects.push(StudioEffect::ReadProjectState {
                        action_id: action.meta.action_id,
                    });
                } else {
                    self.finish_action(action.meta.action_id);
                    self.state.diagnostics.push(StudioDiagnostic::error(
                        Some(action.meta.action_id),
                        "No server session is connected.",
                    ));
                }
            }
            StudioActionKind::Project(ProjectActionRequest::ReadProjectInventory) => {
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
            StudioActionKind::Project(ProjectActionRequest::SelectProjectNode { node_id }) => {
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
            StudioEvent::ProviderCatalogUpdated {
                action_id,
                providers,
            } => {
                if let Some(action_id) = action_id {
                    self.finish_action(action_id);
                }
                self.state.device_manager.providers.set_providers(providers);
                if self
                    .state
                    .device_manager
                    .providers
                    .selected_provider_id()
                    .is_none()
                {
                    self.state.device_manager.active_flow = LinkState::ChoosingProvider;
                }
            }
            StudioEvent::ProviderAvailabilityUpdated {
                action_id,
                provider_id,
                availability,
            } => {
                if let Some(action_id) = action_id {
                    self.finish_action(action_id);
                }
                self.state
                    .device_manager
                    .providers
                    .set_provider_availability(provider_id, availability);
            }
            StudioEvent::DeviceAccessUpdated {
                action_id,
                provider_id,
                status,
            } => {
                if let Some(action_id) = action_id {
                    self.finish_action(action_id);
                }
                self.apply_device_access_status(provider_id, status);
            }
            StudioEvent::EndpointsDiscovered {
                action_id,
                provider_id,
                endpoints,
            } => {
                self.finish_action(action_id);
                self.state
                    .device_manager
                    .providers
                    .select_provider(provider_id.clone());
                self.state
                    .device_manager
                    .providers
                    .set_provider_endpoints(provider_id.clone(), endpoints);
                if let Some(endpoint_id) = self
                    .state
                    .device_manager
                    .providers
                    .first_selected_endpoint()
                    .map(|endpoint| endpoint.id.clone())
                {
                    self.state.device_manager.active_flow = LinkState::Opening {
                        endpoint_id: endpoint_id.clone(),
                    };
                    effects.push(StudioEffect::ConnectEndpoint {
                        action_id,
                        endpoint_id,
                    });
                } else {
                    self.state.device_manager.active_flow = LinkState::ChoosingProvider;
                }
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
                self.state.server = ServerState::Ready;
                if let (Some(device), Some(connection)) =
                    (&self.state.device_session, &self.state.connection_session)
                {
                    self.state.device_manager.current_device =
                        Some(ConnectedDeviceState::connected(
                            device.device_id.clone(),
                            device.provider_id.clone(),
                            device.endpoint_id.clone(),
                            device.session_id.clone(),
                            connection.kind.clone(),
                            device.capabilities.clone(),
                        ));
                    self.state.device_manager.active_flow = LinkState::ServerReady {
                        session_id: device.session_id.clone(),
                    };
                }
            }
            StudioEvent::DeviceConnectionFailed {
                action_id,
                endpoint_id,
                issue,
            } => {
                self.finish_action(action_id);
                self.state.device_manager.push_issue(issue.clone());
                self.state.device_manager.active_flow =
                    LinkState::OpenFailed { endpoint_id, issue };
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
                self.state.server = ServerState::Disconnected;
                self.state.project = ProjectState::Detached;
                self.state.device_manager.current_device = None;
                self.state.device_manager.active_flow = LinkState::Disconnected {
                    reason: Some("Device disconnected".to_string()),
                };
            }
            StudioEvent::DeviceReset {
                action_id,
                endpoint_id,
            } => {
                self.finish_action(action_id);
                self.state.logs.push(StudioLogEntry::new(
                    StudioLogLevel::Info,
                    "lpa-studio-core",
                    format!("device reset requested for {}", endpoint_id.as_str()),
                ));
            }
            StudioEvent::FirmwareFlashCompleted {
                action_id,
                endpoint_id,
                firmware_id,
            } => {
                self.finish_action(action_id);
                self.state.server = ServerState::Opening;
                self.state.device_manager.active_flow = LinkState::OpeningServer {
                    endpoint_id: endpoint_id.clone(),
                };
                let firmware_label = firmware_id.unwrap_or_else(|| "selected firmware".to_string());
                self.state.logs.push(StudioLogEntry::new(
                    StudioLogLevel::Info,
                    "lpa-studio-core",
                    format!(
                        "firmware flash completed for {} using {firmware_label}",
                        endpoint_id.as_str()
                    ),
                ));
            }
            StudioEvent::TargetProbeCompleted { action_id, result } => {
                self.finish_action(action_id);
                if let Some(reason) = result.provisioning_reason.clone() {
                    if let Some(issue) = result.issue.clone() {
                        self.state.device_manager.push_issue(issue);
                    }
                    self.state.device_manager.active_flow = LinkState::ProvisioningRequired {
                        endpoint_id: result.endpoint_id,
                        reason,
                    };
                } else if let Some(issue) = result.issue.clone() {
                    self.state.device_manager.push_issue(issue.clone());
                    self.state.device_manager.active_flow = LinkState::Degraded { issue };
                } else {
                    self.state.device_manager.active_flow = match result.kind {
                        TargetKind::LightPlayerServer => {
                            self.state.server = ServerState::Opening;
                            LinkState::OpeningServer {
                                endpoint_id: result.endpoint_id,
                            }
                        }
                        TargetKind::Bootloader => LinkState::ProvisioningRequired {
                            endpoint_id: result.endpoint_id,
                            reason: crate::ProvisioningReason::BootloaderMode,
                        },
                        TargetKind::BlankDevice => LinkState::ProvisioningRequired {
                            endpoint_id: result.endpoint_id,
                            reason: crate::ProvisioningReason::DeviceBlank,
                        },
                        TargetKind::UnsupportedDevice | TargetKind::Unknown => {
                            let issue = DeviceIssue::error(
                                issue_id("target-probe", action_id),
                                DeviceIssueKind::UnknownTarget,
                                "The connected target could not be identified.",
                            )
                            .with_endpoint(result.endpoint_id);
                            self.state.device_manager.push_issue(issue.clone());
                            LinkState::Degraded { issue }
                        }
                    };
                }
            }
            StudioEvent::TargetProbeFailed {
                action_id,
                endpoint_id,
                issue,
            } => {
                self.finish_action(action_id);
                self.state.device_manager.push_issue(issue.clone());
                self.state.device_manager.active_flow =
                    LinkState::OpenFailed { endpoint_id, issue };
            }
            StudioEvent::ProvisioningIssueRaised { action_id, issue } => {
                if let Some(action_id) = action_id {
                    self.finish_action(action_id);
                }
                self.state.device_manager.push_issue(issue.clone());
                self.state.device_manager.active_flow = LinkState::Degraded { issue };
            }
            StudioEvent::ProvisioningProgressUpdated {
                action_id,
                progress,
            } => {
                if let Some(action_id) = action_id {
                    self.finish_action(action_id);
                }
                self.apply_progress(progress);
            }
            StudioEvent::ProvisioningFlowCanceled { action_id } => {
                self.finish_action(action_id);
                self.state.device_manager.active_flow = LinkState::ChoosingProvider;
            }
            StudioEvent::DemoProjectSeeded {
                action_id,
                project_id,
            } => {
                self.state.project = ProjectState::Loading;
                self.state.device_manager.active_flow = LinkState::DeployingProject {
                    project_id: project_id.clone(),
                    progress: Some(ProgressState::new("Loading project")),
                };
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
                self.state.project = ProjectState::Attaching;
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
                    self.state.project = ProjectState::Ready {
                        project_id: project.project_id.clone(),
                        sync: ProjectSyncState::Clean,
                    };
                    self.state.device_manager.active_flow = LinkState::Ready {
                        project_id: project.project_id.clone(),
                    };
                }
            }
            StudioEvent::LoadedProjectsRefreshed {
                action_id,
                projects: _,
            } => {
                self.finish_action(action_id);
            }
            StudioEvent::ProjectStateRead { action_id, result } => {
                effects.extend(self.apply_project_state_result(action_id, result));
            }
            StudioEvent::HeartbeatReceived { heartbeat } => {
                self.state.heartbeat = Some(heartbeat);
                if let Some(device) = &mut self.state.device_manager.current_device {
                    device.health = crate::DeviceHealthState::Connected;
                }
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
                    .push(StudioDiagnostic::error(Some(action_id), message.clone()));
                let issue = DeviceIssue::error(
                    issue_id("ux-failed", action_id),
                    DeviceIssueKind::ActionFailed,
                    message,
                )
                .with_recovery_actions(vec![RecoveryAction::Retry]);
                self.state.device_manager.push_issue(issue.clone());
                self.state.device_manager.active_flow = LinkState::Degraded { issue };
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

    fn selected_provider_id(&self) -> Option<LinkProviderId> {
        self.state
            .device_manager
            .providers
            .selected_provider_id()
            .cloned()
    }

    fn current_link_session_id(&self) -> Option<LinkSessionId> {
        self.state
            .device_session
            .as_ref()
            .map(|session| session.session_id.clone())
            .or_else(|| {
                self.state
                    .connection_session
                    .as_ref()
                    .map(|session| session.session_id.clone())
            })
    }

    fn apply_project_state_result(
        &mut self,
        action_id: ActionId,
        result: ProjectStateResult,
    ) -> Vec<StudioEffect> {
        match result {
            ProjectStateResult::LoadedProject { project } => {
                self.state.project = ProjectState::Attaching;
                self.state.project_session =
                    Some(ProjectSession::new(project.project_id, project.handle));
                vec![StudioEffect::ReadProjectInventory {
                    action_id,
                    handle: project.handle,
                }]
            }
            ProjectStateResult::NoLoadedProject => {
                self.finish_action(action_id);
                if let Some(session_id) = self.current_link_session_id() {
                    self.state.project = ProjectState::ProjectSelectionRequired {
                        reason: ProjectSelectionReason::NoLoadedProject,
                        projects: Vec::new(),
                    };
                    self.state.device_manager.active_flow = LinkState::ProjectSelectionRequired {
                        session_id,
                        reason: ProjectSelectionReason::NoLoadedProject,
                        projects: Vec::new(),
                    };
                }
                Vec::new()
            }
            ProjectStateResult::MultipleProjects { projects } => {
                self.finish_action(action_id);
                if let Some(session_id) = self.current_link_session_id() {
                    self.state.project = ProjectState::ProjectSelectionRequired {
                        reason: ProjectSelectionReason::MultipleLoadedProjects,
                        projects: projects.clone(),
                    };
                    self.state.device_manager.active_flow = LinkState::ProjectSelectionRequired {
                        session_id,
                        reason: ProjectSelectionReason::MultipleLoadedProjects,
                        projects,
                    };
                }
                Vec::new()
            }
            ProjectStateResult::RecoveryRequired { reason } => {
                self.finish_action(action_id);
                if let Some(session_id) = self.current_link_session_id() {
                    self.state.server = ServerState::RecoveryRequired {
                        reason: format!("{reason:?}"),
                    };
                    self.state.device_manager.active_flow =
                        LinkState::RecoveryRequired { session_id, reason };
                }
                Vec::new()
            }
        }
    }

    fn raise_no_provider_issue(&mut self, action_id: ActionId) {
        let issue = DeviceIssue::error(
            issue_id("no-provider", action_id),
            DeviceIssueKind::ProviderUnavailable,
            "No provider is selected.",
        )
        .with_recovery_actions(vec![RecoveryAction::ChooseSimulator]);
        self.state.device_manager.push_issue(issue.clone());
        self.state.device_manager.active_flow = LinkState::Degraded { issue };
    }

    fn raise_no_endpoint_issue(&mut self, action_id: ActionId) {
        let issue = DeviceIssue::error(
            issue_id("no-endpoint", action_id),
            DeviceIssueKind::NoEndpoint,
            "No provider endpoint is available.",
        )
        .with_recovery_actions(vec![RecoveryAction::Retry]);
        self.state.device_manager.push_issue(issue.clone());
        self.state.device_manager.active_flow = LinkState::Degraded { issue };
    }

    fn apply_device_access_status(
        &mut self,
        provider_id: LinkProviderId,
        status: DeviceAccessStatus,
    ) {
        self.state
            .device_manager
            .providers
            .select_provider(provider_id.clone());
        self.state.device_access = Some(DeviceAccess::new(provider_id.clone(), status.clone()));
        match status {
            DeviceAccessStatus::Unknown | DeviceAccessStatus::PermissionRequired => {
                self.state.device_manager.active_flow = LinkState::RequestingAccess { provider_id };
            }
            DeviceAccessStatus::Granted => {
                self.state
                    .device_manager
                    .providers
                    .set_provider_availability(
                        provider_id.clone(),
                        ProviderAvailability::Available,
                    );
                self.state.device_manager.active_flow = LinkState::ChoosingProvider;
            }
            DeviceAccessStatus::PermissionCanceled { reason } => {
                let issue = DeviceIssue::error(
                    issue_id_for_provider("permission-canceled", &provider_id),
                    DeviceIssueKind::PermissionCanceled,
                    reason,
                )
                .with_provider(provider_id.clone())
                .with_recovery_actions(vec![
                    RecoveryAction::Retry,
                    RecoveryAction::ChooseSimulator,
                ]);
                self.state.device_manager.active_flow = LinkState::AccessFailed {
                    provider_id,
                    issue: issue.clone(),
                };
                self.state.device_manager.push_issue(issue);
            }
            DeviceAccessStatus::Unsupported { reason } => {
                self.state
                    .device_manager
                    .providers
                    .set_provider_availability(
                        provider_id.clone(),
                        ProviderAvailability::unavailable(
                            reason.clone(),
                            vec![
                                RecoveryAction::UseCompatibleBrowser,
                                RecoveryAction::ChooseSimulator,
                            ],
                        ),
                    );
                let issue = DeviceIssue::error(
                    issue_id_for_provider("provider-unsupported", &provider_id),
                    DeviceIssueKind::RuntimeUnsupported,
                    reason,
                )
                .with_provider(provider_id.clone())
                .with_recovery_actions(vec![
                    RecoveryAction::UseCompatibleBrowser,
                    RecoveryAction::ChooseSimulator,
                ]);
                self.state.device_manager.active_flow = LinkState::AccessFailed {
                    provider_id,
                    issue: issue.clone(),
                };
                self.state.device_manager.push_issue(issue);
            }
            DeviceAccessStatus::PermissionDenied { reason } => {
                let issue = DeviceIssue::error(
                    issue_id_for_provider("permission-denied", &provider_id),
                    DeviceIssueKind::PermissionDenied,
                    reason,
                )
                .with_provider(provider_id.clone())
                .with_recovery_actions(vec![
                    RecoveryAction::Retry,
                    RecoveryAction::ChooseSimulator,
                ]);
                self.state.device_manager.active_flow = LinkState::AccessFailed {
                    provider_id,
                    issue: issue.clone(),
                };
                self.state.device_manager.push_issue(issue);
            }
        }
    }

    fn apply_progress(&mut self, progress: ProgressState) {
        match &mut self.state.device_manager.active_flow {
            LinkState::Flashing {
                progress: active_progress,
                ..
            }
            | LinkState::DeployingProject {
                progress: active_progress,
                ..
            } => {
                *active_progress = Some(progress);
            }
            _ => {}
        }
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

fn provider_unavailable_issue(
    action_id: ActionId,
    provider_id: LinkProviderId,
    availability: ProviderAvailability,
) -> DeviceIssue {
    let (message, recovery_actions) = match availability {
        ProviderAvailability::Unavailable {
            reason,
            recovery_actions,
        } => (reason, recovery_actions),
        ProviderAvailability::HiddenInThisBuild => (
            "The selected provider is not available in this build.".to_string(),
            vec![RecoveryAction::ChooseSimulator],
        ),
        _ => (
            "The selected provider is not available.".to_string(),
            vec![RecoveryAction::ChooseSimulator],
        ),
    };
    DeviceIssue::error(
        issue_id("provider-unavailable", action_id),
        DeviceIssueKind::ProviderUnavailable,
        message,
    )
    .with_provider(provider_id)
    .with_recovery_actions(recovery_actions)
}

fn issue_id(prefix: &str, action_id: ActionId) -> String {
    format!("{prefix}-{}", action_id.get())
}

fn issue_id_for_provider(prefix: &str, provider_id: &LinkProviderId) -> String {
    format!("{prefix}-{}", provider_id.as_str())
}

#[cfg(test)]
mod tests {
    use lpa_link::link_endpoint::LinkEndpointId;
    use lpa_link::link_provider::LinkProviderId;
    use lpa_link::link_session::LinkSessionId;
    use lpa_link::{LinkConnectionKind, LinkEndpoint};
    use lpc_wire::{WireProjectHandle, WireProjectInventoryReadResponse};

    use crate::{
        ActionOrigin, DeviceCapability, DeviceIssueKind,
        LinkState, ProjectSelectionReason, ProjectStateResult, ProviderAvailability, ProviderCardState,
        ProviderIntent, RecoveryAction, RecoveryReason, BROWSER_SERIAL_ESP32_PROVIDER_ID, BROWSER_WORKER_PROVIDER_ID,
    };

    use super::*;

    #[test]
    fn default_state_starts_without_selected_provider() {
        let app = StudioApp::new();

        assert!(
            app.state()
                .device_manager
                .providers
                .selected_provider_id()
                .is_none()
        );
        assert!(matches!(
            app.state().device_manager.active_flow,
            LinkState::ChoosingProvider
        ));
    }

    #[test]
    fn discover_devices_produces_effect_and_tracks_in_flight() {
        let mut app = StudioApp::new();
        app.dispatch_kind(
            StudioActionKind::from(LinkActionRequest::SelectProvider {
                provider_id: LinkProviderId::new(BROWSER_WORKER_PROVIDER_ID),
            }),
            ActionOrigin::User,
        );

        let effects = app.dispatch_kind(
            StudioActionKind::from(LinkActionRequest::DiscoverDevices),
            ActionOrigin::User,
        );

        assert_eq!(effects.len(), 1);
        assert!(matches!(effects[0], StudioEffect::DiscoverEndpoints { .. }));
        assert_eq!(app.state().in_flight.len(), 1);
    }

    #[test]
    fn request_device_access_produces_provider_scoped_effect() {
        let mut app = StudioApp::new();
        app.dispatch_kind(
            StudioActionKind::from(LinkActionRequest::SelectProvider {
                provider_id: LinkProviderId::new(BROWSER_WORKER_PROVIDER_ID),
            }),
            ActionOrigin::User,
        );

        let effects = app.dispatch_kind(
            StudioActionKind::from(LinkActionRequest::RequestDeviceAccess),
            ActionOrigin::User,
        );

        assert!(matches!(
            &effects[0],
            StudioEffect::RequestDeviceAccess { provider_id, .. }
                if provider_id.as_str() == BROWSER_WORKER_PROVIDER_ID
        ));
        assert_eq!(app.state().in_flight.len(), 1);
    }

    #[test]
    fn device_access_event_updates_state_and_finishes_action() {
        let mut app = StudioApp::new();
        let action_id = ActionId::new(11);
        app.mark_in_flight(
            action_id,
            ActionDescriptor::for_type(crate::StudioActionType::RequestDeviceAccess),
        );

        app.apply_event(StudioEvent::DeviceAccessUpdated {
            action_id: Some(action_id),
            provider_id: LinkProviderId::new(BROWSER_WORKER_PROVIDER_ID),
            status: crate::DeviceAccessStatus::Granted,
        });

        assert!(app.state().in_flight.is_empty());
        assert_eq!(
            app.state()
                .device_access
                .as_ref()
                .map(|access| access.provider_id.as_str()),
            Some(BROWSER_WORKER_PROVIDER_ID)
        );
        assert!(matches!(
            app.state().device_manager.active_flow,
            LinkState::ChoosingProvider
        ));
    }

    #[test]
    fn provider_catalog_event_updates_device_manager_state() {
        let mut app = StudioApp::new();
        let action_id = ActionId::new(12);
        app.mark_in_flight(
            action_id,
            ActionDescriptor::for_type(crate::StudioActionType::RefreshProviderCatalog),
        );

        app.apply_event(StudioEvent::ProviderCatalogUpdated {
            action_id: Some(action_id),
            providers: vec![
                ProviderCardState::new(
                    BROWSER_WORKER_PROVIDER_ID,
                    "Simulator",
                    ProviderIntent::SimulateInBrowser,
                ),
                ProviderCardState::new(
                    BROWSER_SERIAL_ESP32_PROVIDER_ID,
                    "USB ESP32",
                    ProviderIntent::ConnectUsbEsp32,
                )
                .with_availability(ProviderAvailability::unavailable(
                    "Web Serial is unavailable.",
                    vec![RecoveryAction::UseCompatibleBrowser],
                )),
            ],
        });

        assert!(app.state().in_flight.is_empty());
        assert_eq!(app.state().device_manager.providers.providers.len(), 2);
        assert!(matches!(
            app.state()
                .device_manager
                .providers
                .provider(&LinkProviderId::new(BROWSER_SERIAL_ESP32_PROVIDER_ID))
                .map(|provider| &provider.availability),
            Some(ProviderAvailability::Unavailable { .. })
        ));
    }

    #[test]
    fn unsupported_access_creates_typed_issue() {
        let mut app = StudioApp::new();

        app.apply_event(StudioEvent::DeviceAccessUpdated {
            action_id: None,
            provider_id: LinkProviderId::new(BROWSER_SERIAL_ESP32_PROVIDER_ID),
            status: crate::DeviceAccessStatus::Unsupported {
                reason: "Web Serial is not supported in this browser.".to_string(),
            },
        });

        assert_eq!(app.state().device_manager.issues.len(), 1);
        assert_eq!(
            app.state().device_manager.issues[0].kind,
            DeviceIssueKind::RuntimeUnsupported
        );
        assert!(matches!(
            app.state().device_manager.active_flow,
            LinkState::AccessFailed { .. }
        ));
    }

    #[test]
    fn hardware_management_requires_connected_device() {
        let mut app = StudioApp::new();

        let reset_effects = app.dispatch_kind(
            StudioActionKind::from(LinkActionRequest::Reset),
            ActionOrigin::User,
        );
        let flash_effects = app.dispatch_kind(
            StudioActionKind::from(LinkActionRequest::FlashFirmware { firmware_id: None }),
            ActionOrigin::User,
        );

        assert!(reset_effects.is_empty());
        assert!(flash_effects.is_empty());
        assert_eq!(app.state().diagnostics.len(), 2);
        assert!(app.state().in_flight.is_empty());
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

        assert_eq!(
            app.state()
                .device_manager
                .providers
                .selected_provider_endpoints(),
            &[endpoint]
        );
        assert!(matches!(
            app.state().device_manager.active_flow,
            LinkState::Opening { .. }
        ));
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
        assert!(app.state().device_manager.current_device.is_some());
        assert!(matches!(
            app.state().device_manager.active_flow,
            LinkState::ServerReady { .. }
        ));
    }

    #[test]
    fn read_project_state_marks_flow_and_emits_effect() {
        let mut app = connected_app();

        let effects = app.dispatch_kind(
            StudioActionKind::from(ServerActionRequest::ReadProjectState),
            ActionOrigin::User,
        );

        assert!(matches!(effects[0], StudioEffect::ReadProjectState { .. }));
        assert!(matches!(
            app.state().device_manager.active_flow,
            LinkState::ReadingProjectState { .. }
        ));
    }

    #[test]
    fn project_state_read_attaches_existing_project_and_reads_inventory() {
        let mut app = connected_app();
        let action_id = ActionId::new(42);
        app.mark_in_flight(
            action_id,
            ActionDescriptor::for_type(crate::StudioActionType::ReadProjectState),
        );
        app.state.device_manager.active_flow = LinkState::ReadingProjectState {
            session_id: LinkSessionId::new("session-1"),
        };

        let effects = app.apply_event(StudioEvent::ProjectStateRead {
            action_id,
            result: ProjectStateResult::loaded_project(
                STUDIO_DEMO_PROJECT_ID,
                "/projects/studio-demo",
                WireProjectHandle::new(7),
            ),
        });

        assert!(matches!(
            effects[0],
            StudioEffect::ReadProjectInventory {
                handle: WireProjectHandle(7),
                ..
            }
        ));
        assert_eq!(
            app.state()
                .project_session
                .as_ref()
                .map(|project| project.project_id.as_str()),
            Some(STUDIO_DEMO_PROJECT_ID)
        );

        app.apply_event(StudioEvent::ProjectInventoryRead {
            action_id,
            inventory: WireProjectInventoryReadResponse::default(),
        });

        assert!(app.state().in_flight.is_empty());
        assert!(matches!(
            app.state().device_manager.active_flow,
            LinkState::Ready { .. }
        ));
    }

    #[test]
    fn project_state_read_can_require_project_selection_or_recovery() {
        let mut no_project = connected_app();
        let action_id = ActionId::new(43);

        no_project.apply_event(StudioEvent::ProjectStateRead {
            action_id,
            result: ProjectStateResult::NoLoadedProject,
        });

        assert!(matches!(
            no_project.state().device_manager.active_flow,
            LinkState::ProjectSelectionRequired {
                reason: ProjectSelectionReason::NoLoadedProject,
                ..
            }
        ));

        let mut recovery = connected_app();
        recovery.apply_event(StudioEvent::ProjectStateRead {
            action_id,
            result: ProjectStateResult::RecoveryRequired {
                reason: RecoveryReason::ProjectCrash {
                    project_id: Some(STUDIO_DEMO_PROJECT_ID.to_string()),
                    message: Some("previous boot failed".to_string()),
                },
            },
        });

        assert!(matches!(
            recovery.state().device_manager.active_flow,
            LinkState::RecoveryRequired { .. }
        ));
    }

    #[test]
    fn device_connection_failed_event_sets_link_failed_flow() {
        let mut app = StudioApp::new();
        let action_id = ActionId::new(30);
        let endpoint_id = LinkEndpointId::new("endpoint-a");
        let issue = DeviceIssue::error(
            "endpoint-open-failed",
            DeviceIssueKind::EndpointOpenFailed,
            "Could not open endpoint.",
        )
        .with_endpoint(endpoint_id.clone());
        app.mark_in_flight(
            action_id,
            ActionDescriptor::for_type(crate::StudioActionType::ConnectDevice),
        );

        app.apply_event(StudioEvent::DeviceConnectionFailed {
            action_id,
            endpoint_id: endpoint_id.clone(),
            issue,
        });

        assert!(app.state().in_flight.is_empty());
        assert!(matches!(
            &app.state().device_manager.active_flow,
            LinkState::OpenFailed {
                endpoint_id: flow_endpoint_id,
                issue,
            } if flow_endpoint_id == &endpoint_id
                && issue.kind == DeviceIssueKind::EndpointOpenFailed
        ));
    }

    #[test]
    fn permission_canceled_access_event_sets_access_failed_flow() {
        let mut app = StudioApp::new();
        let action_id = ActionId::new(31);
        let provider_id = LinkProviderId::new(BROWSER_SERIAL_ESP32_PROVIDER_ID);
        app.mark_in_flight(
            action_id,
            ActionDescriptor::for_type(crate::StudioActionType::RequestDeviceAccess),
        );

        app.apply_event(StudioEvent::DeviceAccessUpdated {
            action_id: Some(action_id),
            provider_id: provider_id.clone(),
            status: crate::DeviceAccessStatus::PermissionCanceled {
                reason: "The browser chooser was canceled.".to_string(),
            },
        });

        assert!(app.state().in_flight.is_empty());
        assert!(matches!(
            &app.state().device_manager.active_flow,
            LinkState::AccessFailed {
                provider_id: flow_provider_id,
                issue,
            } if flow_provider_id == &provider_id
                && issue.kind == DeviceIssueKind::PermissionCanceled
        ));
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
        assert!(matches!(
            app.state().device_manager.active_flow,
            LinkState::Ready { .. }
        ));
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
            StudioActionKind::from(ProjectActionRequest::SelectProjectNode {
                node_id: Some("node-a".to_string()),
            }),
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

    fn connected_app() -> StudioApp {
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
        app
    }
}
