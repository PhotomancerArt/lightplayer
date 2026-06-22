use crate::{
    ConnectedDeviceSummary, DeviceOp, DeviceSnapshot, EndpointChoice, LinkOp, LinkState, LinkUx,
    ProjectOp, ProjectState, ProviderChoice, ServerState, ServerUx, UiAction, UiBody, UiMetric,
    UiPaneView, UiStackSection, UiStackView, UiStatus, UiStepState, UiTerminalLine, UxLogEntry,
    UxNode, UxNodeId,
};

pub struct DeviceUx {
    pub(crate) link: LinkUx,
    pub(crate) server: ServerUx,
    terminal: Vec<UiTerminalLine>,
}

impl DeviceUx {
    pub const NODE_ID: &'static str = "studio.device";

    pub fn new() -> Self {
        Self {
            link: LinkUx::new(),
            server: ServerUx::new(),
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

    pub fn has_meaningful_terminal(&self) -> bool {
        !matches!(self.link.state(), LinkState::SelectingProvider { .. })
    }

    pub fn record_logs(&mut self, logs: &[UxLogEntry]) {
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
        let stack = UiStackView::new(vec![
            self.select_connection_section(),
            self.connect_device_section(),
            self.connect_lightplayer_section(),
            self.open_project_section(project_state, project_actions),
        ])
        .with_terminal(if self.has_meaningful_terminal() {
            self.terminal.clone()
        } else {
            Vec::new()
        });

        UiPaneView::new(
            Self::NODE_ID,
            "Device",
            self.status(),
            UiBody::Stack(Box::new(stack)),
            Vec::new(),
        )
    }

    fn status(&self) -> UiStatus {
        match (self.link.state(), &self.server.snapshot().state) {
            (LinkState::SelectingProvider { .. }, _) => UiStatus::neutral("Choose connection"),
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

    fn select_connection_section(&self) -> UiStackSection {
        match self.link.state() {
            LinkState::SelectingProvider { providers } => UiStackSection::new(
                "select-connection",
                "Select connection",
                UiStepState::Active,
            )
            .with_body(UiBody::text("Choose how Studio should connect."))
            .with_actions(provider_actions(providers, self.node_id())),
            LinkState::Failed { .. } => UiStackSection::new(
                "select-connection",
                "Select connection",
                UiStepState::NeedsAttention,
            )
            .with_body(UiBody::text("Refresh connections to try again."))
            .with_actions(vec![self.action(DeviceOp::RefreshConnections)]),
            _ => UiStackSection::new(
                "select-connection",
                "Select connection",
                UiStepState::Complete,
            )
            .with_body(UiBody::text(selected_connection_label(self.link.state()))),
        }
    }

    fn connect_device_section(&self) -> UiStackSection {
        match self.link.state() {
            LinkState::SelectingProvider { .. } => {
                UiStackSection::new("connect-device", "Connect device", UiStepState::Pending)
                    .with_body(UiBody::text("Choose a connection first."))
            }
            LinkState::DiscoveringEndpoints {
                provider_id,
                progress,
            } => UiStackSection::new("connect-device", "Connect device", UiStepState::Active)
                .with_body(UiBody::Progress(progress.clone().with_detail(format!(
                    "Discovering endpoints from {}.",
                    provider_id.label()
                )))),
            LinkState::SelectingEndpoint {
                provider_id,
                endpoints,
            } => UiStackSection::new("connect-device", "Connect device", UiStepState::Active)
                .with_body(UiBody::text("Choose the device endpoint to open."))
                .with_actions(endpoint_actions(*provider_id, endpoints, self.node_id())),
            LinkState::Connecting { progress, .. } => {
                UiStackSection::new("connect-device", "Connect device", UiStepState::Active)
                    .with_body(UiBody::Progress(progress.clone()))
            }
            LinkState::Managing { device, progress } => {
                UiStackSection::new("connect-device", "Connect device", UiStepState::Active)
                    .with_body(UiBody::Metrics(vec![
                        UiMetric::new("Device", &device.label),
                        UiMetric::new("Session", &device.session_id),
                        UiMetric::new("Operation", &progress.label),
                    ]))
            }
            LinkState::Connected { device } => {
                UiStackSection::new("connect-device", "Connect device", UiStepState::Complete)
                    .with_body(device_summary_body(device))
            }
            LinkState::Failed { issue } => UiStackSection::new(
                "connect-device",
                "Connect device",
                UiStepState::NeedsAttention,
            )
            .with_body(UiBody::Issue(issue.clone()))
            .with_actions(vec![self.action(DeviceOp::RefreshConnections)]),
        }
    }

    fn connect_lightplayer_section(&self) -> UiStackSection {
        match (self.link.state(), &self.server.snapshot().state) {
            (LinkState::Connected { .. }, ServerState::Disconnected) => UiStackSection::new(
                "connect-lightplayer",
                "Connect LightPlayer",
                UiStepState::Active,
            )
            .with_body(UiBody::text(
                "Attach Studio to LightPlayer on the connected device.",
            ))
            .with_actions(self.lightplayer_actions(false)),
            (LinkState::Connected { .. }, ServerState::Connecting { progress }) => {
                UiStackSection::new(
                    "connect-lightplayer",
                    "Connect LightPlayer",
                    UiStepState::Active,
                )
                .with_body(UiBody::Progress(progress.clone()))
            }
            (LinkState::Connected { .. }, ServerState::Connected { protocol }) => {
                UiStackSection::new(
                    "connect-lightplayer",
                    "Connect LightPlayer",
                    UiStepState::Complete,
                )
                .with_body(UiBody::Metrics(vec![UiMetric::new("Protocol", protocol)]))
                .with_actions(self.lightplayer_actions(true))
            }
            (LinkState::Connected { .. }, ServerState::Failed { issue }) => UiStackSection::new(
                "connect-lightplayer",
                "Connect LightPlayer",
                UiStepState::NeedsAttention,
            )
            .with_body(UiBody::Issue(issue.clone()))
            .with_actions(self.lightplayer_actions(false)),
            (LinkState::Managing { progress, .. }, _) => UiStackSection::new(
                "connect-lightplayer",
                "Connect LightPlayer",
                UiStepState::Active,
            )
            .with_body(UiBody::Progress(progress.clone())),
            _ => UiStackSection::new(
                "connect-lightplayer",
                "Connect LightPlayer",
                UiStepState::Pending,
            )
            .with_body(UiBody::text("Connect a device first.")),
        }
    }

    fn open_project_section(
        &self,
        project_state: &ProjectState,
        actions: Vec<UiAction>,
    ) -> UiStackSection {
        if !self.has_lightplayer_state() {
            return UiStackSection::new("open-project", "Open project", UiStepState::Pending)
                .with_body(UiBody::text("Connect LightPlayer first."));
        }

        match project_state {
            ProjectState::NotLoaded => {
                UiStackSection::new("open-project", "Open project", UiStepState::Active)
                    .with_body(UiBody::text(not_loaded_project_prompt(&actions)))
                    .with_actions(actions)
            }
            ProjectState::SelectingLoadedProject { projects } => {
                UiStackSection::new("open-project", "Open project", UiStepState::Active)
                    .with_body(UiBody::text(format!(
                        "{} projects are running. Choose one to open.",
                        projects.len()
                    )))
                    .with_actions(actions)
            }
            ProjectState::ConnectingRunningProject { progress }
            | ProjectState::LoadingDemoProject { progress } => {
                UiStackSection::new("open-project", "Open project", UiStepState::Active)
                    .with_body(UiBody::Progress(progress.clone()))
            }
            ProjectState::Ready { project_id, .. } => {
                UiStackSection::new("open-project", "Open project", UiStepState::Complete)
                    .with_body(UiBody::text(format!("{project_id} is loaded.")))
            }
            ProjectState::Failed { issue } => {
                UiStackSection::new("open-project", "Open project", UiStepState::NeedsAttention)
                    .with_body(UiBody::Issue(issue.clone()))
                    .with_actions(actions)
            }
        }
    }

    fn lightplayer_actions(&self, server_connected: bool) -> Vec<UiAction> {
        self.link
            .actions(server_connected)
            .into_iter()
            .filter_map(|action| map_link_action(action, self.node_id()))
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

impl UxNode for DeviceUx {
    type Op = DeviceOp;

    fn node_id(&self) -> UxNodeId {
        UxNodeId::new(Self::NODE_ID)
    }
}

impl Default for DeviceUx {
    fn default() -> Self {
        Self::new()
    }
}

fn provider_actions(providers: &[ProviderChoice], node_id: UxNodeId) -> Vec<UiAction> {
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
    node_id: UxNodeId,
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

fn map_link_action(action: UiAction, node_id: UxNodeId) -> Option<UiAction> {
    let meta = action.meta().clone();
    let op = match action.op_as::<LinkOp>()? {
        LinkOp::RefreshProviders => DeviceOp::RefreshConnections,
        LinkOp::ConnectServer => DeviceOp::ConnectLightPlayer,
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
                .with_label("Disconnect device")
                .with_summary("Close the current device session."),
        )
    } else {
        Some(action)
    }
}

fn device_summary_body(device: &ConnectedDeviceSummary) -> UiBody {
    UiBody::Metrics(vec![
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
