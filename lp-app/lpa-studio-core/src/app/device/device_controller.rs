use crate::core::view::steps_view::{UiStepState, UiStepView};
use crate::{
    ConnectedDeviceSummary, Controller, ControllerId, DeviceOp, DeviceSnapshot, EndpointChoice,
    LinkController, LinkOp, LinkState, ProjectOp, ProjectState, ProviderChoice, ServerController,
    ServerFailureKind, ServerState, UiAction, UiLogEntry, UiMetric, UiPaneView, UiStatus,
    UiStepsView, UiTerminalLine, UiViewContent,
};

pub struct DeviceController {
    pub(crate) link: LinkController,
    pub(crate) server: ServerController,
    terminal: Vec<UiTerminalLine>,
}

impl DeviceController {
    pub const NODE_ID: &'static str = "studio|device";
    pub const SECTION_SELECT_CONNECTION: &'static str = "select-connection";
    pub const SECTION_CONNECT_DEVICE: &'static str = "connect-device";
    pub const SECTION_CONNECT_LIGHTPLAYER: &'static str = "connect-lightplayer";
    pub const SECTION_OPEN_PROJECT: &'static str = "open-project";

    pub fn new() -> Self {
        Self {
            link: LinkController::new(),
            server: ServerController::new(),
            terminal: Vec::new(),
        }
    }

    pub fn snapshot(&self) -> DeviceSnapshot {
        DeviceSnapshot::new(self.link.snapshot(), self.server.snapshot())
    }

    pub fn is_lightplayer_connected(&self) -> bool {
        self.server.is_connected()
    }

    pub fn has_lightplayer_state(&self) -> bool {
        matches!(self.server.snapshot().state, ServerState::Connected { .. })
    }

    pub fn needs_firmware(&self) -> bool {
        matches!(
            self.server.snapshot().state,
            ServerState::Failed {
                kind: ServerFailureKind::NoFirmware,
                ..
            }
        )
    }

    pub fn has_meaningful_terminal(&self) -> bool {
        !matches!(self.link.state(), LinkState::SelectingProvider { .. })
    }

    pub fn record_logs(&mut self, logs: &[UiLogEntry]) {
        self.terminal.extend(
            logs.iter()
                .filter(|log| is_device_log_source(&log.source))
                .map(|log| UiTerminalLine::new(format!("[{}] {}", log.source, log.message))),
        );
        if self.terminal.len() > 240 {
            let remove_count = self.terminal.len() - 240;
            self.terminal.drain(0..remove_count);
        }
    }

    pub fn view(&self, project_state: &ProjectState, project_actions: Vec<UiAction>) -> UiPaneView {
        let stack = UiStepsView::new(self.sections(project_state, project_actions)).with_terminal(
            if self.has_meaningful_terminal() {
                self.terminal.clone()
            } else {
                Vec::new()
            },
        );

        UiPaneView::new(
            Self::NODE_ID,
            "Device",
            self.status(),
            UiViewContent::Stack(Box::new(stack)),
            Vec::new(),
        )
    }

    fn sections(
        &self,
        project_state: &ProjectState,
        project_actions: Vec<UiAction>,
    ) -> Vec<UiStepView> {
        let mut sections = vec![self.select_connection_section()];
        if self.should_show_connect_device() {
            sections.push(self.connect_device_section());
        }
        if self.should_show_connect_lightplayer() {
            sections.push(self.connect_lightplayer_section());
        }
        if self.should_show_open_project(project_state) {
            sections.push(self.open_project_section(project_state, project_actions));
        }
        sections
    }

    fn should_show_connect_device(&self) -> bool {
        !matches!(
            self.link.state(),
            LinkState::SelectingProvider { .. } | LinkState::Failed { .. }
        )
    }

    fn should_show_connect_lightplayer(&self) -> bool {
        matches!(
            self.link.state(),
            LinkState::Connected { .. } | LinkState::Managing { .. }
        )
    }

    fn should_show_open_project(&self, _project_state: &ProjectState) -> bool {
        self.has_lightplayer_state()
    }

    fn should_show_device_controls(&self) -> bool {
        matches!(
            self.server.snapshot().state,
            ServerState::Disconnected | ServerState::Failed { .. }
        )
    }

    fn status(&self) -> UiStatus {
        match (self.link.state(), &self.server.snapshot().state) {
            (LinkState::SelectingProvider { issue: Some(_), .. }, _) => {
                UiStatus::error("Needs attention")
            }
            (LinkState::SelectingProvider { .. }, _) => UiStatus::neutral("Choose connection"),
            (
                LinkState::Connected { .. },
                ServerState::Failed {
                    kind: ServerFailureKind::NoFirmware,
                    ..
                },
            ) => UiStatus::warning("Ready to flash"),
            (LinkState::Failed { .. }, _) | (_, ServerState::Failed { .. }) => {
                UiStatus::error("Needs attention")
            }
            (LinkState::DiscoveringEndpoints { .. }, _)
            | (LinkState::SelectingEndpoint { .. }, _)
            | (LinkState::Connecting { .. }, _)
            | (LinkState::Managing { .. }, _)
            | (_, ServerState::Connecting { .. }) => UiStatus::working("Connecting"),
            (_, ServerState::Connected { .. }) => UiStatus::good("LightPlayer ready"),
            (LinkState::Connected { device }, ServerState::Disconnected) => {
                UiStatus::good(device.label.clone())
            }
        }
    }

    fn select_connection_section(&self) -> UiStepView {
        match self.link.state() {
            LinkState::SelectingProvider { providers, issue } => {
                let section = UiStepView::new(
                    Self::SECTION_SELECT_CONNECTION,
                    "Select connection",
                    if issue.is_some() {
                        UiStepState::NeedsAttention
                    } else {
                        UiStepState::Active
                    },
                )
                .with_actions(provider_actions(providers, self.node_id()));
                match issue {
                    Some(issue) => section.with_body(UiViewContent::Issue(issue.clone())),
                    None => {
                        section.with_body(UiViewContent::text("Choose how Studio should connect."))
                    }
                }
            }
            LinkState::Failed { .. } => UiStepView::new(
                Self::SECTION_SELECT_CONNECTION,
                "Select connection",
                UiStepState::NeedsAttention,
            )
            .with_body(UiViewContent::text("Refresh connections to try again."))
            .with_actions(vec![self.action(DeviceOp::RefreshConnections)]),
            _ => UiStepView::new(
                Self::SECTION_SELECT_CONNECTION,
                "Select connection",
                UiStepState::Complete,
            )
            .with_body(UiViewContent::text(selected_connection_label(
                self.link.state(),
            ))),
        }
    }

    fn connect_device_section(&self) -> UiStepView {
        match self.link.state() {
            LinkState::SelectingProvider { .. } => UiStepView::new(
                Self::SECTION_CONNECT_DEVICE,
                "Connect device",
                UiStepState::Pending,
            )
            .with_body(UiViewContent::text("Choose a connection first.")),
            LinkState::DiscoveringEndpoints {
                provider_id,
                progress,
            } => UiStepView::new(
                Self::SECTION_CONNECT_DEVICE,
                "Connect device",
                UiStepState::Active,
            )
            .with_body(UiViewContent::Progress(
                progress
                    .clone()
                    .with_detail(format!(
                        "Discovering endpoints from {}.",
                        provider_id.label()
                    ))
                    .into(),
            )),
            LinkState::SelectingEndpoint {
                provider_id,
                endpoints,
            } => UiStepView::new(
                Self::SECTION_CONNECT_DEVICE,
                "Connect device",
                UiStepState::Active,
            )
            .with_body(UiViewContent::text("Choose the device endpoint to open."))
            .with_actions(endpoint_actions(*provider_id, endpoints, self.node_id())),
            LinkState::Connecting { progress, .. } => UiStepView::new(
                Self::SECTION_CONNECT_DEVICE,
                "Connect device",
                UiStepState::Active,
            )
            .with_body(UiViewContent::Progress(progress.clone().into())),
            LinkState::Connected { device } | LinkState::Managing { device, .. } => {
                let section = UiStepView::new(
                    Self::SECTION_CONNECT_DEVICE,
                    "Connect device",
                    UiStepState::Complete,
                )
                .with_body(device_summary_body(device));
                if self.should_show_device_controls() {
                    section.with_actions(self.device_control_actions())
                } else {
                    section
                }
            }
            LinkState::Failed { issue } => UiStepView::new(
                Self::SECTION_CONNECT_DEVICE,
                "Connect device",
                UiStepState::NeedsAttention,
            )
            .with_body(UiViewContent::Issue(issue.clone()))
            .with_actions(vec![self.action(DeviceOp::RefreshConnections)]),
        }
    }

    fn connect_lightplayer_section(&self) -> UiStepView {
        match (self.link.state(), &self.server.snapshot().state) {
            (LinkState::Connected { .. }, ServerState::Disconnected) => UiStepView::new(
                Self::SECTION_CONNECT_LIGHTPLAYER,
                "Connect LightPlayer",
                UiStepState::Active,
            )
            .with_body(UiViewContent::text(
                "Attach Studio to LightPlayer on the connected device.",
            ))
            .with_actions(self.connect_lightplayer_actions()),
            (LinkState::Connected { .. }, ServerState::Connecting { progress }) => UiStepView::new(
                Self::SECTION_CONNECT_LIGHTPLAYER,
                "Connect LightPlayer",
                UiStepState::Active,
            )
            .with_body(UiViewContent::Progress(progress.clone().into())),
            (LinkState::Connected { .. }, ServerState::Connected { protocol }) => UiStepView::new(
                Self::SECTION_CONNECT_LIGHTPLAYER,
                "Connect LightPlayer",
                UiStepState::Complete,
            )
            .with_body(UiViewContent::Metrics(vec![UiMetric::new(
                "Protocol", protocol,
            )]))
            .with_actions(self.connected_lightplayer_actions()),
            (LinkState::Connected { .. }, ServerState::Failed { issue, kind }) => {
                let no_firmware = *kind == ServerFailureKind::NoFirmware;
                UiStepView::new(
                    Self::SECTION_CONNECT_LIGHTPLAYER,
                    if no_firmware {
                        "LightPlayer unavailable"
                    } else {
                        "Connect LightPlayer"
                    },
                    if no_firmware {
                        UiStepState::Active
                    } else {
                        UiStepState::NeedsAttention
                    },
                )
                .with_body(if no_firmware {
                    UiViewContent::text("No LightPlayer firmware is running on this ESP32.")
                } else {
                    UiViewContent::Issue(issue.clone())
                })
                .with_actions(if no_firmware {
                    Vec::new()
                } else {
                    self.connect_lightplayer_actions()
                })
            }
            (LinkState::Managing { progress, .. }, _) => UiStepView::new(
                Self::SECTION_CONNECT_LIGHTPLAYER,
                progress.label.clone(),
                UiStepState::Active,
            )
            .with_body(UiViewContent::Progress(progress.clone().into())),
            _ => UiStepView::new(
                Self::SECTION_CONNECT_LIGHTPLAYER,
                "Connect LightPlayer",
                UiStepState::Pending,
            )
            .with_body(UiViewContent::text("Connect a device first.")),
        }
    }

    fn open_project_section(
        &self,
        project_state: &ProjectState,
        actions: Vec<UiAction>,
    ) -> UiStepView {
        if !self.has_lightplayer_state() {
            if self.needs_firmware() {
                return UiStepView::new(
                    Self::SECTION_OPEN_PROJECT,
                    "Open project",
                    UiStepState::Pending,
                )
                .with_body(UiViewContent::text(
                    "Flash firmware before opening a project.",
                ));
            }
            return UiStepView::new(
                Self::SECTION_OPEN_PROJECT,
                "Open project",
                UiStepState::Pending,
            )
            .with_body(UiViewContent::text("Connect LightPlayer first."));
        }

        match project_state {
            ProjectState::NotLoaded => UiStepView::new(
                Self::SECTION_OPEN_PROJECT,
                "Open project",
                UiStepState::Active,
            )
            .with_body(UiViewContent::text(not_loaded_project_prompt(&actions)))
            .with_actions(actions),
            ProjectState::SelectingLoadedProject { projects } => UiStepView::new(
                Self::SECTION_OPEN_PROJECT,
                "Open project",
                UiStepState::Active,
            )
            .with_body(UiViewContent::text(format!(
                "{} projects are running. Choose one to open.",
                projects.len()
            )))
            .with_actions(actions),
            ProjectState::ConnectingRunningProject { progress }
            | ProjectState::LoadingDemoProject { progress } => UiStepView::new(
                Self::SECTION_OPEN_PROJECT,
                "Open project",
                UiStepState::Active,
            )
            .with_body(UiViewContent::Progress(progress.clone().into())),
            ProjectState::Ready { project_id, .. } => UiStepView::new(
                Self::SECTION_OPEN_PROJECT,
                "Open project",
                UiStepState::Complete,
            )
            .with_body(UiViewContent::text(format!("{project_id} is loaded."))),
            ProjectState::Failed { issue } => UiStepView::new(
                Self::SECTION_OPEN_PROJECT,
                "Open project",
                UiStepState::NeedsAttention,
            )
            .with_body(UiViewContent::Issue(issue.clone()))
            .with_actions(actions),
        }
    }

    fn lightplayer_actions(&self, server_connected: bool) -> Vec<UiAction> {
        self.link
            .actions(server_connected)
            .into_iter()
            .filter_map(|action| map_link_action(action, self.node_id()))
            .collect()
    }

    fn connect_lightplayer_actions(&self) -> Vec<UiAction> {
        self.lightplayer_actions(false)
            .into_iter()
            .filter(|action| {
                matches!(
                    action.op_as::<DeviceOp>(),
                    Some(DeviceOp::ConnectLightPlayer)
                )
            })
            .collect()
    }

    fn connected_lightplayer_actions(&self) -> Vec<UiAction> {
        let mut actions = self
            .lightplayer_actions(false)
            .into_iter()
            .filter(|action| matches!(action.op_as::<DeviceOp>(), Some(DeviceOp::ResetDevice)))
            .collect::<Vec<_>>();
        actions.push(self.action(DeviceOp::DisconnectLightPlayer));
        actions
    }

    fn device_control_actions(&self) -> Vec<UiAction> {
        self.lightplayer_actions(false)
            .into_iter()
            .filter(|action| {
                !matches!(
                    action.op_as::<DeviceOp>(),
                    Some(DeviceOp::ConnectLightPlayer)
                )
            })
            .collect()
    }
}

fn not_loaded_project_prompt(actions: &[UiAction]) -> &'static str {
    if actions.iter().any(|action| {
        matches!(
            action.op_as::<ProjectOp>(),
            Some(ProjectOp::ConnectRunningProject)
        )
    }) {
        "Connect to a running project or load the demo project."
    } else {
        "No running project is loaded. Load the demo project when you're ready."
    }
}

impl Controller for DeviceController {
    type Op = DeviceOp;

    fn node_id(&self) -> ControllerId {
        ControllerId::new(Self::NODE_ID)
    }
}

impl Default for DeviceController {
    fn default() -> Self {
        Self::new()
    }
}

fn provider_actions(providers: &[ProviderChoice], node_id: ControllerId) -> Vec<UiAction> {
    providers
        .iter()
        .map(|provider| {
            UiAction::from_op(
                node_id.clone(),
                DeviceOp::OpenProvider {
                    provider_id: provider.id,
                },
            )
            .with_label(provider_action_label(provider.id))
            .with_summary(provider.summary.clone())
            .with_short_label(provider_action_short_label(provider.id))
            .with_icon(provider_action_icon(provider.id))
            .with_priority(provider_action_priority(provider.id))
        })
        .collect()
}

fn endpoint_actions(
    provider_id: lpa_link::LinkProviderKind,
    endpoints: &[EndpointChoice],
    node_id: ControllerId,
) -> Vec<UiAction> {
    endpoints
        .iter()
        .map(|endpoint| {
            UiAction::from_op(
                node_id.clone(),
                DeviceOp::ConnectEndpoint {
                    provider_id,
                    endpoint_id: endpoint.id.clone(),
                },
            )
            .with_label(format!("Open {}", endpoint.label))
            .with_summary(endpoint.summary.clone())
        })
        .collect()
}

fn map_link_action(action: UiAction, node_id: ControllerId) -> Option<UiAction> {
    let meta = action.meta().clone();
    let op = match action.op_as::<LinkOp>()? {
        LinkOp::RefreshProviders => DeviceOp::RefreshConnections,
        LinkOp::ConnectServer => DeviceOp::ConnectLightPlayer,
        LinkOp::ResetDevice => DeviceOp::ResetDevice,
        LinkOp::ProvisionFirmware => DeviceOp::ProvisionFirmware,
        LinkOp::ResetToBlank => DeviceOp::ResetToBlank,
        LinkOp::DisconnectLink => DeviceOp::DisconnectDevice,
        LinkOp::OpenProvider { provider_id } => DeviceOp::OpenProvider {
            provider_id: *provider_id,
        },
        LinkOp::ConnectEndpoint {
            provider_id,
            endpoint_id,
        } => DeviceOp::ConnectEndpoint {
            provider_id: *provider_id,
            endpoint_id: endpoint_id.clone(),
        },
    };
    let action = UiAction::from_op(node_id, op).with_meta(meta);
    if matches!(
        action.op_as::<DeviceOp>(),
        Some(DeviceOp::ConnectLightPlayer)
    ) {
        Some(
            action
                .with_label("Connect LightPlayer")
                .with_summary("Attach Studio to LightPlayer on the connected device."),
        )
    } else if matches!(action.op_as::<DeviceOp>(), Some(DeviceOp::DisconnectDevice)) {
        Some(
            action
                .with_label("Disconnect")
                .with_summary("Close the current device session."),
        )
    } else {
        Some(action)
    }
}

fn device_summary_body(device: &ConnectedDeviceSummary) -> UiViewContent {
    UiViewContent::Metrics(vec![
        UiMetric::new("Provider", device.provider_id.label()),
        UiMetric::new("Endpoint", &device.endpoint_id),
        UiMetric::new("Session", &device.session_id),
    ])
}

fn selected_connection_label(state: &LinkState) -> String {
    match state {
        LinkState::DiscoveringEndpoints { provider_id, .. }
        | LinkState::SelectingEndpoint { provider_id, .. } => provider_id.label().to_string(),
        LinkState::Connecting { endpoint, .. } => endpoint.label.clone(),
        LinkState::Managing { device, .. } | LinkState::Connected { device } => {
            device.label.clone()
        }
        LinkState::Failed { .. } => "Connection needs attention.".to_string(),
        LinkState::SelectingProvider {
            issue: Some(issue), ..
        } => issue.message.clone(),
        LinkState::SelectingProvider { .. } => "Choose how to connect.".to_string(),
    }
}

fn provider_action_label(kind: lpa_link::LinkProviderKind) -> String {
    match kind {
        lpa_link::LinkProviderKind::BrowserWorker => "Start simulator".to_string(),
        lpa_link::LinkProviderKind::HostProcess => "Start host runtime".to_string(),
        lpa_link::LinkProviderKind::BrowserSerialEsp32 => "Connect ESP32".to_string(),
        lpa_link::LinkProviderKind::HostSerialEsp32 => "Select hardware".to_string(),
        lpa_link::LinkProviderKind::Fake => "Select fake provider".to_string(),
    }
}

fn provider_action_short_label(kind: lpa_link::LinkProviderKind) -> String {
    match kind {
        lpa_link::LinkProviderKind::BrowserWorker => "Simulator".to_string(),
        lpa_link::LinkProviderKind::HostProcess => "Host".to_string(),
        lpa_link::LinkProviderKind::BrowserSerialEsp32
        | lpa_link::LinkProviderKind::HostSerialEsp32 => "ESP32".to_string(),
        lpa_link::LinkProviderKind::Fake => "Fake".to_string(),
    }
}

fn provider_action_icon(kind: lpa_link::LinkProviderKind) -> String {
    match kind {
        lpa_link::LinkProviderKind::BrowserWorker | lpa_link::LinkProviderKind::HostProcess => {
            "play".to_string()
        }
        lpa_link::LinkProviderKind::BrowserSerialEsp32
        | lpa_link::LinkProviderKind::HostSerialEsp32 => "usb".to_string(),
        lpa_link::LinkProviderKind::Fake => "test-tube".to_string(),
    }
}

fn provider_action_priority(kind: lpa_link::LinkProviderKind) -> crate::ActionPriority {
    match kind {
        lpa_link::LinkProviderKind::BrowserWorker | lpa_link::LinkProviderKind::HostProcess => {
            crate::ActionPriority::Primary
        }
        lpa_link::LinkProviderKind::BrowserSerialEsp32
        | lpa_link::LinkProviderKind::HostSerialEsp32 => crate::ActionPriority::Secondary,
        lpa_link::LinkProviderKind::Fake => crate::ActionPriority::Tertiary,
    }
}

fn is_device_log_source(source: &str) -> bool {
    matches!(
        source,
        "lpa-link" | "browser-serial" | "fw-esp32" | "fw-browser" | "lp-server"
    )
}
