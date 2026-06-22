use std::cell::RefCell;
use std::rc::Rc;

use lpa_link::providers::{LinkEnv, LinkProviderInstance, LinkProviderRegistry};
use lpa_link::{
    LinkConnection, LinkDiagnosticSeverity, LinkEndpointId, LinkLogLevel, LinkManagementRequest,
    LinkManagementResult, LinkOperation, LinkProvider, LinkProviderKind, LinkSession,
    LinkSessionId,
};
#[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
use lpc_model::DEFAULT_SERIAL_BAUD_RATE;

use crate::{
    ActionPriority, ConnectedDeviceSummary, EndpointChoice, LinkOp, LinkSnapshot, LinkState,
    ProgressState, ProviderChoice, UxAction, UxActivity, UxBody, UxError, UxIssue, UxLogEntry,
    UxLogLevel, UxMetric, UxNode, UxNodeId, UxPaneView, UxProgress, UxStatus, UxUpdate,
    UxUpdateSink,
};
use lpa_link::{LinkManagementEvent, LinkManagementEventSink};

pub type SharedLinkRegistry = Rc<RefCell<LinkProviderRegistry>>;

pub struct LinkUx {
    state: LinkState,
    registry: SharedLinkRegistry,
    active_provider: Option<LinkProviderKind>,
    active_endpoint: Option<LinkEndpointId>,
    active_session: Option<LinkSession>,
    active_connection: Option<LinkConnection>,
}

impl LinkUx {
    pub const NODE_ID: &'static str = "studio.link";

    pub fn new() -> Self {
        Self::with_env(LinkEnv::default())
    }

    pub fn with_env(env: LinkEnv) -> Self {
        Self::with_registry(LinkProviderRegistry::from_env(env))
    }

    pub fn with_registry(registry: LinkProviderRegistry) -> Self {
        let registry = Rc::new(RefCell::new(registry));
        let providers = provider_choices(&registry.borrow());
        Self {
            state: LinkState::SelectingProvider { providers },
            registry,
            active_provider: None,
            active_endpoint: None,
            active_session: None,
            active_connection: None,
        }
    }

    pub fn state(&self) -> &LinkState {
        &self.state
    }

    pub fn set_state(&mut self, state: LinkState) {
        self.state = state;
    }

    pub fn snapshot(&self) -> LinkSnapshot {
        LinkSnapshot::new(self.state.clone())
    }

    pub fn registry_handle(&self) -> SharedLinkRegistry {
        Rc::clone(&self.registry)
    }

    pub fn active_connection(&self) -> Option<LinkConnection> {
        self.active_connection.clone()
    }

    pub fn actions(&self, server_connected: bool) -> Vec<UxAction> {
        match &self.state {
            LinkState::SelectingProvider { providers } => providers
                .iter()
                .map(|provider| {
                    self.action(LinkOp::OpenProvider {
                        provider_id: provider.id,
                    })
                    .with_label(provider_action_label(provider.id))
                    .with_summary(provider.summary.clone())
                    .with_short_label(provider_action_short_label(provider.id))
                    .with_icon(provider_action_icon(provider.id))
                    .with_priority(provider_action_priority(provider.id))
                })
                .collect(),
            LinkState::SelectingEndpoint {
                provider_id,
                endpoints,
            } => endpoints
                .iter()
                .map(|endpoint| {
                    self.action(LinkOp::ConnectEndpoint {
                        provider_id: *provider_id,
                        endpoint_id: endpoint.id.clone(),
                    })
                    .with_label(format!("Open {}", endpoint.label))
                    .with_summary(endpoint.summary.clone())
                })
                .collect(),
            LinkState::Failed { .. } => vec![self.action(LinkOp::RefreshProviders)],
            LinkState::DiscoveringEndpoints { .. }
            | LinkState::Connecting { .. }
            | LinkState::Managing { .. } => Vec::new(),
            LinkState::Connected { .. } if server_connected => {
                let mut actions = Vec::new();
                if self.active_supports(LinkOperation::EraseDeviceFlash) {
                    actions.push(self.action(LinkOp::ResetToBlank));
                }
                actions.push(self.action(LinkOp::DisconnectLink));
                actions
            }
            LinkState::Connected { .. } => {
                let mut actions = Vec::new();
                if self.active_supports(LinkOperation::FlashFirmware) {
                    actions.push(self.action(LinkOp::ProvisionFirmware));
                }
                actions.push(self.action(LinkOp::ConnectServer));
                if self.active_supports(LinkOperation::EraseDeviceFlash) {
                    actions.push(self.action(LinkOp::ResetToBlank));
                }
                actions.push(self.action(LinkOp::DisconnectLink));
                actions
            }
        }
    }

    pub fn view(&self, server_connected: bool) -> UxPaneView {
        UxPaneView::new(
            Self::NODE_ID,
            "Link",
            link_status(&self.state),
            link_body(&self.state),
            self.actions(server_connected),
        )
    }

    pub fn refresh_provider_catalog(&mut self) {
        self.active_provider = None;
        self.active_endpoint = None;
        self.active_session = None;
        self.active_connection = None;
        self.state = LinkState::SelectingProvider {
            providers: provider_choices(&self.registry.borrow()),
        };
    }

    pub async fn disconnect(&mut self) -> Result<(), UxError> {
        let provider_id = self.active_provider;
        let session_id = self
            .active_session
            .as_ref()
            .map(|session| session.id.clone());
        let result = match (provider_id, session_id) {
            (Some(provider_id), Some(session_id)) => {
                let mut registry = self.registry.borrow_mut();
                let provider = registry
                    .provider_mut(provider_id)
                    .ok_or_else(|| missing_provider(provider_id))?;
                provider.close(&session_id).await.map_err(map_link_error)
            }
            _ => Ok(()),
        };
        match result {
            Ok(()) => {
                self.refresh_provider_catalog();
                Ok(())
            }
            Err(error) => {
                self.fail(error.message());
                Err(error)
            }
        }
    }

    pub async fn open_provider(
        &mut self,
        provider_id: LinkProviderKind,
    ) -> Result<LinkOpenOutcome, UxError> {
        if provider_id == LinkProviderKind::BrowserSerialEsp32 {
            return self.open_browser_serial_provider().await;
        }

        self.discover_provider_endpoints(provider_id).await?;
        let endpoints = match &self.state {
            LinkState::SelectingEndpoint { endpoints, .. } => endpoints.clone(),
            _ => Vec::new(),
        };
        if endpoints.len() == 1 && provider_auto_connects(provider_id) {
            let endpoint_id = endpoints[0].id.clone();
            return self
                .connect_endpoint(provider_id, endpoint_id)
                .await
                .map(LinkOpenOutcome::Connected);
        }
        Ok(LinkOpenOutcome::Opened)
    }

    pub async fn discover_provider_endpoints(
        &mut self,
        provider_id: LinkProviderKind,
    ) -> Result<(), UxError> {
        self.active_provider = Some(provider_id);
        self.active_endpoint = None;
        self.active_session = None;
        self.active_connection = None;
        self.state = LinkState::DiscoveringEndpoints {
            provider_id,
            progress: ProgressState::new("Discovering endpoints"),
        };

        let result = {
            let mut registry = self.registry.borrow_mut();
            match registry.provider_mut(provider_id) {
                Some(provider) => provider.discover().await.map_err(map_link_error),
                None => Err(missing_provider(provider_id)),
            }
        };
        let endpoints = match result {
            Ok(endpoints) => endpoints,
            Err(error) => {
                self.fail(error.message());
                return Err(error);
            }
        };
        if endpoints.is_empty() {
            let error = UxError::Link(format!(
                "{} did not report any endpoints",
                provider_id.label()
            ));
            self.fail(error.message());
            return Err(error);
        }

        self.state = LinkState::SelectingEndpoint {
            provider_id,
            endpoints: endpoints
                .into_iter()
                .map(EndpointChoice::from_endpoint)
                .collect(),
        };
        Ok(())
    }

    #[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
    async fn open_browser_serial_provider(&mut self) -> Result<LinkOpenOutcome, UxError> {
        self.active_provider = Some(LinkProviderKind::BrowserSerialEsp32);
        self.active_endpoint = None;
        self.active_session = None;
        self.active_connection = None;
        self.state = LinkState::DiscoveringEndpoints {
            provider_id: LinkProviderKind::BrowserSerialEsp32,
            progress: ProgressState::new("Requesting browser serial access"),
        };

        let result = {
            let mut registry = self.registry.borrow_mut();
            match registry.provider_mut(LinkProviderKind::BrowserSerialEsp32) {
                Some(LinkProviderInstance::BrowserSerialEsp32(provider)) => {
                    provider.request_access().await.map_err(map_link_error)
                }
                Some(_) => Err(UxError::Link(
                    "browser serial registry entry has the wrong provider type".to_string(),
                )),
                None => Err(missing_provider(LinkProviderKind::BrowserSerialEsp32)),
            }
        };
        let endpoint = match result {
            Ok(endpoint) => endpoint,
            Err(error) => {
                self.fail(error.message());
                return Err(error);
            }
        };
        let endpoint_choice = EndpointChoice::from_endpoint(endpoint);
        let endpoint_id = endpoint_choice.id.clone();
        self.state = LinkState::SelectingEndpoint {
            provider_id: LinkProviderKind::BrowserSerialEsp32,
            endpoints: vec![endpoint_choice],
        };
        self.connect_endpoint(LinkProviderKind::BrowserSerialEsp32, endpoint_id)
            .await
            .map(LinkOpenOutcome::Connected)
    }

    #[cfg(not(all(feature = "browser-serial-esp32", target_arch = "wasm32")))]
    async fn open_browser_serial_provider(&mut self) -> Result<LinkOpenOutcome, UxError> {
        Err(UxError::UnsupportedFeature(
            "browser serial ESP32 access requires the browser-serial-esp32 feature on wasm"
                .to_string(),
        ))
    }

    pub async fn connect_endpoint(
        &mut self,
        provider_id: LinkProviderKind,
        endpoint_id: LinkEndpointId,
    ) -> Result<ConnectedLink, UxError> {
        let endpoint = self
            .endpoint_choice(provider_id, &endpoint_id)
            .unwrap_or_else(|| EndpointChoice {
                provider_id,
                id: endpoint_id.clone(),
                label: endpoint_id.as_str().to_string(),
                summary: "Open this endpoint.".to_string(),
                status: lpa_link::LinkEndpointStatus::Available,
            });
        self.state = LinkState::Connecting {
            endpoint: endpoint.clone(),
            progress: ProgressState::new("Opening link session"),
        };

        let result = {
            let mut registry = self.registry.borrow_mut();
            match registry.provider_mut(provider_id) {
                Some(provider) => {
                    open_connected_provider(provider_id, provider, &endpoint_id).await
                }
                None => Err(missing_provider(provider_id)),
            }
        };
        let (session, connection, logs) = match result {
            Ok(result) => result,
            Err(error) => {
                self.active_session = None;
                self.active_connection = None;
                self.fail(error.message());
                return Err(error);
            }
        };

        self.active_provider = Some(provider_id);
        self.active_endpoint = Some(endpoint_id);
        self.active_session = Some(session.clone());
        self.active_connection = Some(connection.clone());
        self.state = LinkState::Connected {
            device: ConnectedDeviceSummary::new(
                provider_id,
                session.endpoint_id.as_str(),
                session.id().as_str(),
                endpoint.label,
            ),
        };

        Ok(ConnectedLink { connection, logs })
    }

    pub async fn manage(
        &mut self,
        request: LinkManagementRequest,
        progress_label: impl Into<String>,
    ) -> Result<LinkManagementOutcome, UxError> {
        self.manage_with_updates(request, progress_label, UxUpdateSink::noop())
            .await
    }

    pub async fn manage_with_updates(
        &mut self,
        request: LinkManagementRequest,
        progress_label: impl Into<String>,
        updates: UxUpdateSink,
    ) -> Result<LinkManagementOutcome, UxError> {
        let provider_id = self
            .active_provider
            .ok_or_else(|| UxError::MissingSession("link provider is not selected".to_string()))?;
        let session_id = self
            .active_session
            .as_ref()
            .map(|session| session.id.clone())
            .ok_or_else(|| UxError::MissingSession("link session is not open".to_string()))?;
        let device = self.connected_device_summary()?;
        let progress_label = progress_label.into();
        self.active_connection = None;
        self.state = LinkState::Managing {
            device: device.clone(),
            progress: ProgressState::new(progress_label.clone()),
        };
        let node_id = self.node_id();
        let activity = Rc::new(RefCell::new(
            UxActivity::new(progress_label.clone())
                .with_progress(UxProgress::indeterminate(progress_label.clone())),
        ));
        updates.emit(UxUpdate::Activity {
            node_id: node_id.clone(),
            status: UxStatus::working("Managing"),
            activity: activity.borrow().clone(),
        });
        let event_sink = management_activity_sink(node_id, activity, updates);

        let result = {
            let mut registry = self.registry.borrow_mut();
            match registry.provider_mut(provider_id) {
                Some(provider) => provider
                    .manage_with_events(&session_id, request, event_sink)
                    .await
                    .map_err(map_link_error),
                None => Err(missing_provider(provider_id)),
            }
        };
        self.state = LinkState::Connected { device };
        let result = result?;
        let logs = management_result_logs(&result);
        Ok(LinkManagementOutcome { result, logs })
    }

    pub async fn reopen_active_connection(&mut self) -> Result<ConnectedLink, UxError> {
        let provider_id = self
            .active_provider
            .ok_or_else(|| UxError::MissingSession("link provider is not selected".to_string()))?;
        let session_id = self
            .active_session
            .as_ref()
            .map(|session| session.id.clone())
            .ok_or_else(|| UxError::MissingSession("link session is not open".to_string()))?;
        let (connection, logs) = {
            let mut registry = self.registry.borrow_mut();
            let provider = registry
                .provider_mut(provider_id)
                .ok_or_else(|| missing_provider(provider_id))?;
            open_provider_protocol_if_needed(provider_id, provider, &session_id).await?;
            let connection = provider
                .connection(&session_id)
                .await
                .map_err(map_link_error)?;
            let logs = link_session_logs(provider, &session_id)?;
            (connection, logs)
        };
        self.active_connection = Some(connection.clone());
        Ok(ConnectedLink { connection, logs })
    }

    pub fn fail(&mut self, message: impl Into<String>) {
        self.state = LinkState::Failed {
            issue: UxIssue::new(message),
        };
    }

    fn endpoint_choice(
        &self,
        provider_id: LinkProviderKind,
        endpoint_id: &LinkEndpointId,
    ) -> Option<EndpointChoice> {
        match &self.state {
            LinkState::SelectingEndpoint {
                provider_id: state_provider,
                endpoints,
            } if *state_provider == provider_id => endpoints
                .iter()
                .find(|endpoint| endpoint.id == *endpoint_id)
                .cloned(),
            LinkState::Connecting { endpoint, .. }
                if endpoint.provider_id == provider_id && endpoint.id == *endpoint_id =>
            {
                Some(endpoint.clone())
            }
            _ => None,
        }
    }

    fn active_supports(&self, operation: LinkOperation) -> bool {
        self.active_session
            .as_ref()
            .is_some_and(|session| session.capabilities.supports(operation))
    }

    fn connected_device_summary(&self) -> Result<ConnectedDeviceSummary, UxError> {
        match &self.state {
            LinkState::Connected { device } | LinkState::Managing { device, .. } => {
                Ok(device.clone())
            }
            _ => Err(UxError::MissingSession(
                "link is not connected to a device".to_string(),
            )),
        }
    }
}

impl UxNode for LinkUx {
    type Op = LinkOp;

    fn node_id(&self) -> UxNodeId {
        UxNodeId::new(Self::NODE_ID)
    }
}

pub struct ConnectedLink {
    pub connection: LinkConnection,
    pub logs: Vec<UxLogEntry>,
}

pub enum LinkOpenOutcome {
    Opened,
    Connected(ConnectedLink),
}

pub struct LinkManagementOutcome {
    pub result: LinkManagementResult,
    pub logs: Vec<UxLogEntry>,
}

impl Default for LinkUx {
    fn default() -> Self {
        Self::new()
    }
}

fn provider_choices(registry: &LinkProviderRegistry) -> Vec<ProviderChoice> {
    let descriptors = registry.descriptors();
    let server_descriptors = descriptors
        .iter()
        .filter(|descriptor| provider_can_open_server(descriptor.kind))
        .cloned()
        .collect::<Vec<_>>();
    let visible_descriptors = if server_descriptors.is_empty() {
        descriptors
    } else {
        server_descriptors
    };
    visible_descriptors
        .into_iter()
        .map(ProviderChoice::from_descriptor)
        .collect()
}

fn link_status(state: &LinkState) -> UxStatus {
    match state {
        LinkState::SelectingProvider { .. } => UxStatus::neutral("Choose runtime"),
        LinkState::DiscoveringEndpoints { .. } => UxStatus::working("Discovering"),
        LinkState::SelectingEndpoint { .. } => UxStatus::neutral("Choose endpoint"),
        LinkState::Connecting { .. } => UxStatus::working("Connecting"),
        LinkState::Managing { .. } => UxStatus::working("Managing"),
        LinkState::Connected { device } => UxStatus::good(device.label.clone()),
        LinkState::Failed { .. } => UxStatus::error("Link failed"),
    }
}

fn link_body(state: &LinkState) -> UxBody {
    match state {
        LinkState::SelectingProvider { providers } => providers
            .first()
            .map(|provider| UxBody::text(provider.summary.clone()))
            .unwrap_or_else(|| UxBody::text("No link providers are available.")),
        LinkState::DiscoveringEndpoints {
            provider_id,
            progress,
        } => UxBody::Progress(progress.clone().with_detail(format!(
            "Discovering endpoints from {}.",
            provider_id.label()
        ))),
        LinkState::SelectingEndpoint { endpoints, .. } => endpoints
            .first()
            .map(|endpoint| UxBody::text(endpoint.summary.clone()))
            .unwrap_or_else(|| UxBody::text("No endpoints are available for this provider.")),
        LinkState::Connecting { progress, .. } => UxBody::Progress(progress.clone()),
        LinkState::Managing { progress, .. } => UxBody::Progress(progress.clone()),
        LinkState::Connected { device } => UxBody::Metrics(vec![
            UxMetric::new("Provider", device.provider_id.label()),
            UxMetric::new("Endpoint", &device.endpoint_id),
            UxMetric::new("Session", &device.session_id),
        ]),
        LinkState::Failed { issue } => UxBody::Issue(issue.clone()),
    }
}

fn management_result_logs(result: &LinkManagementResult) -> Vec<UxLogEntry> {
    match result {
        LinkManagementResult::FlashFirmware(result) => {
            let mut logs = result
                .logs
                .iter()
                .map(|message| UxLogEntry::new(UxLogLevel::Info, "lpa-link", message.clone()))
                .collect::<Vec<_>>();
            logs.extend(result.progress.iter().map(|progress| {
                UxLogEntry::new(UxLogLevel::Info, "lpa-link", progress.label.clone())
            }));
            logs
        }
        LinkManagementResult::EraseDeviceFlash(result) => {
            let mut logs = result
                .logs
                .iter()
                .map(|message| UxLogEntry::new(UxLogLevel::Info, "lpa-link", message.clone()))
                .collect::<Vec<_>>();
            logs.extend(result.progress.iter().map(|progress| {
                UxLogEntry::new(UxLogLevel::Info, "lpa-link", progress.label.clone())
            }));
            logs
        }
        LinkManagementResult::EraseRawFilesystem(result) => {
            let mut logs = result
                .logs
                .iter()
                .map(|message| UxLogEntry::new(UxLogLevel::Info, "lpa-link", message.clone()))
                .collect::<Vec<_>>();
            logs.extend(result.progress.iter().map(|progress| {
                UxLogEntry::new(UxLogLevel::Info, "lpa-link", progress.label.clone())
            }));
            logs
        }
        LinkManagementResult::ResetRuntime => {
            vec![UxLogEntry::new(
                UxLogLevel::Info,
                "lpa-link",
                "runtime reset completed",
            )]
        }
    }
}

fn management_activity_sink(
    node_id: UxNodeId,
    activity: Rc<RefCell<UxActivity>>,
    updates: UxUpdateSink,
) -> LinkManagementEventSink {
    LinkManagementEventSink::new(move |event| {
        let mut activity = activity.borrow_mut();
        apply_management_event(&mut activity, event);
        updates.emit(UxUpdate::Activity {
            node_id: node_id.clone(),
            status: UxStatus::working("Managing"),
            activity: (*activity).clone(),
        });
    })
}

fn apply_management_event(activity: &mut UxActivity, event: LinkManagementEvent) {
    match event {
        LinkManagementEvent::Log { message } => {
            if !message.trim().is_empty() {
                activity.push_terminal_line(message);
                activity.retain_recent_terminal_lines(80);
            }
        }
        LinkManagementEvent::Progress(progress) => {
            let mut ux_progress = match progress.percent {
                Some(percent) => UxProgress::determinate(progress.label, percent),
                None => UxProgress::indeterminate(progress.label),
            };
            if let Some(total_steps) = progress.total_steps {
                ux_progress = ux_progress.with_detail(format!(
                    "Step {} of {}",
                    progress.completed_steps.min(total_steps),
                    total_steps
                ));
            }
            activity.progress = Some(ux_progress);
        }
    }
}

fn provider_can_open_server(kind: LinkProviderKind) -> bool {
    matches!(
        kind,
        LinkProviderKind::BrowserWorker
            | LinkProviderKind::HostProcess
            | LinkProviderKind::BrowserSerialEsp32
            | LinkProviderKind::HostSerialEsp32
    )
}

fn provider_action_label(kind: LinkProviderKind) -> String {
    match kind {
        LinkProviderKind::BrowserWorker => "Start simulator".to_string(),
        LinkProviderKind::HostProcess => "Start host runtime".to_string(),
        LinkProviderKind::BrowserSerialEsp32 => "Connect ESP32".to_string(),
        LinkProviderKind::HostSerialEsp32 => "Select hardware".to_string(),
        LinkProviderKind::Fake => "Select fake provider".to_string(),
    }
}

fn provider_action_short_label(kind: LinkProviderKind) -> String {
    match kind {
        LinkProviderKind::BrowserWorker => "Simulator".to_string(),
        LinkProviderKind::HostProcess => "Host".to_string(),
        LinkProviderKind::BrowserSerialEsp32 | LinkProviderKind::HostSerialEsp32 => {
            "ESP32".to_string()
        }
        LinkProviderKind::Fake => "Fake".to_string(),
    }
}

fn provider_action_icon(kind: LinkProviderKind) -> String {
    match kind {
        LinkProviderKind::BrowserWorker | LinkProviderKind::HostProcess => "play".to_string(),
        LinkProviderKind::BrowserSerialEsp32 | LinkProviderKind::HostSerialEsp32 => {
            "usb".to_string()
        }
        LinkProviderKind::Fake => "test-tube".to_string(),
    }
}

fn provider_auto_connects(kind: LinkProviderKind) -> bool {
    matches!(
        kind,
        LinkProviderKind::BrowserWorker | LinkProviderKind::HostProcess
    )
}

async fn open_connected_provider(
    provider_id: LinkProviderKind,
    provider: &mut LinkProviderInstance,
    endpoint_id: &LinkEndpointId,
) -> Result<(LinkSession, LinkConnection, Vec<UxLogEntry>), UxError> {
    let session = provider
        .connect(endpoint_id)
        .await
        .map_err(map_link_error)?;
    if let Err(error) = open_provider_protocol_if_needed(provider_id, provider, session.id()).await
    {
        close_failed_session(provider, session.id()).await;
        return Err(error);
    }
    let connection = match provider.connection(session.id()).await {
        Ok(connection) => connection,
        Err(error) => {
            close_failed_session(provider, session.id()).await;
            return Err(map_link_error(error));
        }
    };
    let logs = match link_session_logs(provider, session.id()) {
        Ok(logs) => logs,
        Err(error) => {
            close_failed_session(provider, session.id()).await;
            return Err(error);
        }
    };
    Ok((session, connection, logs))
}

async fn close_failed_session(provider: &mut LinkProviderInstance, session_id: &LinkSessionId) {
    let _ = provider.close(session_id).await;
}

#[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
async fn open_provider_protocol_if_needed(
    provider_id: LinkProviderKind,
    provider: &mut LinkProviderInstance,
    session_id: &LinkSessionId,
) -> Result<(), UxError> {
    if provider_id != LinkProviderKind::BrowserSerialEsp32 {
        return Ok(());
    }
    let LinkProviderInstance::BrowserSerialEsp32(provider) = provider else {
        return Err(UxError::Link(
            "browser serial registry entry has the wrong provider type".to_string(),
        ));
    };
    provider
        .open_protocol(session_id, DEFAULT_SERIAL_BAUD_RATE)
        .await
        .map_err(map_link_error)
}

#[cfg(not(all(feature = "browser-serial-esp32", target_arch = "wasm32")))]
async fn open_provider_protocol_if_needed(
    provider_id: LinkProviderKind,
    _provider: &mut LinkProviderInstance,
    _session_id: &LinkSessionId,
) -> Result<(), UxError> {
    let _ = provider_id;
    Ok(())
}

fn provider_action_priority(kind: LinkProviderKind) -> ActionPriority {
    match kind {
        LinkProviderKind::BrowserWorker | LinkProviderKind::HostProcess => ActionPriority::Primary,
        LinkProviderKind::BrowserSerialEsp32 | LinkProviderKind::HostSerialEsp32 => {
            ActionPriority::Secondary
        }
        LinkProviderKind::Fake => ActionPriority::Tertiary,
    }
}

fn link_session_logs(
    provider: &lpa_link::providers::LinkProviderInstance,
    session_id: &lpa_link::LinkSessionId,
) -> Result<Vec<UxLogEntry>, UxError> {
    let mut logs = provider
        .logs(session_id)
        .map_err(map_link_error)?
        .into_iter()
        .map(|entry| UxLogEntry::new(map_link_log_level(entry.level), "lpa-link", entry.message))
        .collect::<Vec<_>>();
    logs.extend(
        provider
            .diagnostics(session_id)
            .map_err(map_link_error)?
            .into_iter()
            .map(|diagnostic| {
                UxLogEntry::new(
                    map_diagnostic_level(diagnostic.severity),
                    "lpa-link",
                    diagnostic.message,
                )
            }),
    );
    Ok(logs)
}

fn missing_provider(provider_id: LinkProviderKind) -> UxError {
    UxError::Link(format!("provider {} is not available", provider_id.key()))
}

fn map_link_error(error: impl core::fmt::Display) -> UxError {
    UxError::Link(error.to_string())
}

fn map_link_log_level(level: LinkLogLevel) -> UxLogLevel {
    match level {
        LinkLogLevel::Trace | LinkLogLevel::Debug => UxLogLevel::Debug,
        LinkLogLevel::Info => UxLogLevel::Info,
        LinkLogLevel::Warn => UxLogLevel::Warn,
        LinkLogLevel::Error => UxLogLevel::Error,
    }
}

fn map_diagnostic_level(level: LinkDiagnosticSeverity) -> UxLogLevel {
    match level {
        LinkDiagnosticSeverity::Info => UxLogLevel::Info,
        LinkDiagnosticSeverity::Warning => UxLogLevel::Warn,
        LinkDiagnosticSeverity::Error => UxLogLevel::Error,
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    use lpa_link::providers::LinkProviderRegistry;
    use lpa_link::providers::fake::FakeProvider;
    use lpa_link::{
        LinkCapabilities, LinkConnectionKind, LinkEndpoint, LinkManagementRequest,
        LinkProviderKind, LinkSession,
    };

    use super::*;

    #[test]
    fn selecting_provider_offers_registry_provider_actions() {
        let link = LinkUx::with_registry(registry_with_fake_endpoint());

        let actions = link.actions(false);

        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0].op_as::<LinkOp>(),
            Some(&LinkOp::OpenProvider {
                provider_id: LinkProviderKind::Fake
            })
        );
        assert_eq!(actions[0].node_id().as_str(), LinkUx::NODE_ID);
        assert_eq!(actions[0].meta().label, "Select fake provider");
    }

    #[test]
    fn connected_link_without_server_offers_server_attach() {
        let mut link = LinkUx::with_registry(registry_with_fake_endpoint());
        link.set_state(LinkState::Connected {
            device: ConnectedDeviceSummary::new(
                LinkProviderKind::Fake,
                "fake-runtime",
                "fake-session",
                "Fake runtime",
            ),
        });

        let actions = link.actions(false);

        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].op_as::<LinkOp>(), Some(&LinkOp::ConnectServer));
        assert_eq!(actions[1].op_as::<LinkOp>(), Some(&LinkOp::DisconnectLink));
    }

    #[test]
    fn connected_link_with_server_only_offers_link_disconnect() {
        let mut link = LinkUx::with_registry(registry_with_fake_endpoint());
        link.set_state(LinkState::Connected {
            device: ConnectedDeviceSummary::new(
                LinkProviderKind::Fake,
                "fake-runtime",
                "fake-session",
                "Fake runtime",
            ),
        });

        let actions = link.actions(true);

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].op_as::<LinkOp>(), Some(&LinkOp::DisconnectLink));
    }

    #[test]
    fn connected_management_capable_link_offers_provision_and_reset_without_server() {
        let mut link = LinkUx::with_registry(registry_with_fake_endpoint());
        link.active_session = Some(management_capable_session());
        link.set_state(LinkState::Connected {
            device: ConnectedDeviceSummary::new(
                LinkProviderKind::Fake,
                "fake-runtime",
                "fake-session",
                "Fake runtime",
            ),
        });

        let actions = link.actions(false);

        assert_eq!(actions.len(), 4);
        assert_eq!(
            actions[0].op_as::<LinkOp>(),
            Some(&LinkOp::ProvisionFirmware)
        );
        assert_eq!(actions[1].op_as::<LinkOp>(), Some(&LinkOp::ConnectServer));
        assert_eq!(actions[2].op_as::<LinkOp>(), Some(&LinkOp::ResetToBlank));
        assert_eq!(actions[3].op_as::<LinkOp>(), Some(&LinkOp::DisconnectLink));
    }

    #[test]
    fn connected_management_capable_link_keeps_reset_available_with_server() {
        let mut link = LinkUx::with_registry(registry_with_fake_endpoint());
        link.active_session = Some(management_capable_session());
        link.set_state(LinkState::Connected {
            device: ConnectedDeviceSummary::new(
                LinkProviderKind::Fake,
                "fake-runtime",
                "fake-session",
                "Fake runtime",
            ),
        });

        let actions = link.actions(true);

        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].op_as::<LinkOp>(), Some(&LinkOp::ResetToBlank));
        assert_eq!(actions[1].op_as::<LinkOp>(), Some(&LinkOp::DisconnectLink));
    }

    #[test]
    fn failed_management_returns_to_recoverable_connected_state() {
        let mut link = LinkUx::with_registry(registry_with_fake_endpoint());
        link.active_provider = Some(LinkProviderKind::Fake);
        link.active_session = Some(management_capable_session());
        link.set_state(LinkState::Connected {
            device: ConnectedDeviceSummary::new(
                LinkProviderKind::Fake,
                "fake-runtime",
                "fake-session",
                "Fake runtime",
            ),
        });

        let result = block_on_ready(link.manage(
            LinkManagementRequest::EraseDeviceFlash,
            "Resetting device to blank",
        ));

        assert!(matches!(result, Err(UxError::Link(_))));
        assert!(matches!(link.state(), LinkState::Connected { .. }));
        assert!(!link.actions(false).is_empty());
    }

    #[test]
    fn failed_endpoint_discovery_enters_recoverable_failed_state() {
        let mut link =
            LinkUx::with_registry(registry_with_fake_discover_error("serial discovery failed"));

        let result = block_on_ready(link.open_provider(LinkProviderKind::Fake));

        assert!(matches!(result, Err(UxError::Link(_))));
        assert!(matches!(
            link.state(),
            LinkState::Failed { issue } if issue.message.contains("serial discovery failed")
        ));
        assert_eq!(link.actions(false).len(), 1);
        assert_eq!(
            link.actions(false)[0].op_as::<LinkOp>(),
            Some(&LinkOp::RefreshProviders)
        );
    }

    #[test]
    fn failed_endpoint_connect_enters_recoverable_failed_state() {
        let mut link = LinkUx::with_registry(registry_with_fake_connect_error(
            "Failed to open serial port.",
        ));

        let result = block_on_ready(
            link.connect_endpoint(LinkProviderKind::Fake, LinkEndpointId::new("fake-runtime")),
        );

        assert!(matches!(result, Err(UxError::Link(_))));
        assert!(matches!(
            link.state(),
            LinkState::Failed { issue } if issue.message.contains("Failed to open serial port")
        ));
        assert_eq!(link.actions(false).len(), 1);
        assert_eq!(
            link.actions(false)[0].op_as::<LinkOp>(),
            Some(&LinkOp::RefreshProviders)
        );
    }

    #[test]
    fn failed_connection_handoff_enters_recoverable_failed_state() {
        let mut link =
            LinkUx::with_registry(registry_with_fake_connection_error("server handoff failed"));

        let result = block_on_ready(
            link.connect_endpoint(LinkProviderKind::Fake, LinkEndpointId::new("fake-runtime")),
        );

        assert!(matches!(result, Err(UxError::Link(_))));
        assert!(matches!(
            link.state(),
            LinkState::Failed { issue } if issue.message.contains("server handoff failed")
        ));
        assert_eq!(link.actions(false).len(), 1);
        assert_eq!(
            link.actions(false)[0].op_as::<LinkOp>(),
            Some(&LinkOp::RefreshProviders)
        );
    }

    #[test]
    fn snapshot_projects_provider_catalog_from_registry() {
        let link = LinkUx::with_registry(registry_with_fake_endpoint());

        assert!(matches!(
            link.snapshot().state,
            LinkState::SelectingProvider { ref providers }
                if providers.len() == 1 && providers[0].id == LinkProviderKind::Fake
        ));
    }

    fn registry_with_fake_endpoint() -> LinkProviderRegistry {
        let mut registry = LinkProviderRegistry::new();
        registry.insert(FakeProvider::new().with_endpoint(LinkEndpoint::new(
            "fake-runtime",
            LinkProviderKind::Fake,
            "Fake runtime",
        )));
        registry
    }

    fn registry_with_fake_discover_error(message: impl Into<String>) -> LinkProviderRegistry {
        let mut registry = LinkProviderRegistry::new();
        registry.insert(
            FakeProvider::new()
                .with_endpoint(LinkEndpoint::new(
                    "fake-runtime",
                    LinkProviderKind::Fake,
                    "Fake runtime",
                ))
                .with_discover_error(message),
        );
        registry
    }

    fn registry_with_fake_connect_error(message: impl Into<String>) -> LinkProviderRegistry {
        let mut registry = LinkProviderRegistry::new();
        registry.insert(
            FakeProvider::new()
                .with_endpoint(LinkEndpoint::new(
                    "fake-runtime",
                    LinkProviderKind::Fake,
                    "Fake runtime",
                ))
                .with_connect_error(message),
        );
        registry
    }

    fn registry_with_fake_connection_error(message: impl Into<String>) -> LinkProviderRegistry {
        let mut registry = LinkProviderRegistry::new();
        registry.insert(
            FakeProvider::new()
                .with_endpoint(LinkEndpoint::new(
                    "fake-runtime",
                    LinkProviderKind::Fake,
                    "Fake runtime",
                ))
                .with_connection_error(message),
        );
        registry
    }

    fn management_capable_session() -> LinkSession {
        LinkSession::new(
            "fake-session",
            LinkProviderKind::Fake,
            "fake-runtime",
            LinkConnectionKind::Fake,
            LinkCapabilities::esp32_serial_base()
                .with_flash()
                .with_device_erase(),
        )
    }

    fn block_on_ready<F>(future: F) -> F::Output
    where
        F: Future,
    {
        let waker = Waker::from(Arc::new(NoopWake));
        let mut context = Context::from_waker(&waker);
        let mut future = Box::pin(future);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("test future unexpectedly yielded"),
        }
    }

    struct NoopWake;

    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }
}
