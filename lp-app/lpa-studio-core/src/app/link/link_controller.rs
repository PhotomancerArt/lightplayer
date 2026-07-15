use std::cell::RefCell;
use std::rc::Rc;

use lpa_link::providers::{LinkConnector, LinkEnv, LinkProviderRegistry};
use lpa_link::{
    LinkConnection, LinkDiagnosticSeverity, LinkEndpointId, LinkError, LinkLogLevel,
    LinkManagementRequest, LinkManagementResult, LinkProvider, LinkProviderKind, LinkSession,
    LinkSessionId,
};
#[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
use lpc_model::DEFAULT_SERIAL_BAUD_RATE;

use crate::{
    ConnectedDeviceSummary, ControllerId, EndpointChoice, LinkSnapshot, LinkState, ProgressState,
    ProviderChoice, UiActivityView, UiError, UiIssue, UiLogDraft, UiLogLevel, UiLogOrigin,
    UiLogSource, UiProgress, UiStatus, UxUpdate, UxUpdateSink,
};
use lpa_link::{LinkManagementEvent, LinkManagementEventSink};

pub struct LinkController {
    state: LinkState,
    /// Catalog + factory: consulted only when a flow needs the picker list
    /// or a fresh connector; never borrowed across an await (its methods are
    /// synchronous and it is owned by value).
    registry: LinkProviderRegistry,
    /// The connection's owned provider handle. Created per open flow from
    /// the registry factory; client I/O adapters hold clones of this `Rc`.
    active_connector: Option<Rc<LinkConnector>>,
    active_provider: Option<LinkProviderKind>,
    active_endpoint: Option<LinkEndpointId>,
    active_session: Option<LinkSession>,
    active_connection: Option<LinkConnection>,
}

impl LinkController {
    pub const NODE_ID: &'static str = "studio|link";

    pub fn new() -> Self {
        Self::with_env(LinkEnv::default())
    }

    pub fn with_env(env: LinkEnv) -> Self {
        Self::with_registry(LinkProviderRegistry::from_env(env))
    }

    pub fn with_registry(registry: LinkProviderRegistry) -> Self {
        let providers = provider_choices(&registry);
        Self {
            state: LinkState::SelectingProvider {
                providers,
                issue: None,
            },
            registry,
            active_connector: None,
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

    #[cfg(test)]
    pub(crate) fn set_active_session_for_test(&mut self, session: LinkSession) {
        self.active_session = Some(session);
    }

    #[cfg(test)]
    pub(crate) fn set_active_connection_for_test(&mut self, connection: LinkConnection) {
        self.active_connection = Some(connection);
    }

    pub fn snapshot(&self) -> LinkSnapshot {
        LinkSnapshot::new(self.state.clone())
    }

    /// The owned provider handle behind the active connection, for handing
    /// to client I/O adapters (`ServerController::attach_link_connection`).
    pub fn active_connector(&self) -> Option<Rc<LinkConnector>> {
        self.active_connector.clone()
    }

    pub fn active_connection(&self) -> Option<LinkConnection> {
        self.active_connection.clone()
    }

    pub fn refresh_provider_catalog(&mut self) {
        self.reset_to_provider_selection(None);
    }

    fn reset_to_provider_selection(&mut self, issue: Option<UiIssue>) {
        self.active_connector = None;
        self.active_provider = None;
        self.active_endpoint = None;
        self.active_session = None;
        self.active_connection = None;
        let providers = provider_choices(&self.registry);
        self.state = LinkState::SelectingProvider { providers, issue };
    }

    fn recover_to_provider_selection(&mut self, message: impl Into<String>) {
        self.reset_to_provider_selection(Some(UiIssue::new(message)));
    }

    /// The active connector if it serves `provider_id`, otherwise a fresh
    /// one from the registry factory (stored as active).
    fn connector_for(
        &mut self,
        provider_id: LinkProviderKind,
    ) -> Result<Rc<LinkConnector>, UiError> {
        if let Some(connector) = &self.active_connector {
            if connector.kind() == provider_id {
                return Ok(Rc::clone(connector));
            }
        }
        let connector = self
            .registry
            .create_connector(provider_id)
            .map_err(map_link_error)?;
        self.active_connector = Some(Rc::clone(&connector));
        Ok(connector)
    }

    pub async fn disconnect(&mut self) -> Result<(), UiError> {
        let provider_id = self.active_provider;
        let session_id = self
            .active_session
            .as_ref()
            .map(|session| session.id.clone());
        let result = match (provider_id, session_id) {
            (Some(provider_id), Some(session_id)) => {
                let connector = self.connector_for(provider_id)?;
                connector.close(&session_id).await.map_err(map_link_error)
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
    ) -> Result<LinkOpenOutcome, UiError> {
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
    ) -> Result<(), UiError> {
        self.active_connector = None;
        self.active_provider = Some(provider_id);
        self.active_endpoint = None;
        self.active_session = None;
        self.active_connection = None;
        self.state = LinkState::DiscoveringEndpoints {
            provider_id,
            progress: ProgressState::new("Discovering endpoints"),
        };

        let result = match self.connector_for(provider_id) {
            Ok(connector) => connector.discover().await.map_err(map_link_error),
            Err(error) => Err(error),
        };
        let endpoints = match result {
            Ok(endpoints) => endpoints,
            Err(error) => {
                self.recover_to_provider_selection(error.message());
                return Err(error);
            }
        };
        if endpoints.is_empty() {
            let message = format!("{} did not report any endpoints", provider_id.label());
            self.recover_to_provider_selection(message.clone());
            return Err(UiError::Link(message));
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
    async fn open_browser_serial_provider(&mut self) -> Result<LinkOpenOutcome, UiError> {
        self.active_connector = None;
        self.active_provider = Some(LinkProviderKind::BrowserSerialEsp32);
        self.active_endpoint = None;
        self.active_session = None;
        self.active_connection = None;
        self.state = LinkState::DiscoveringEndpoints {
            provider_id: LinkProviderKind::BrowserSerialEsp32,
            progress: ProgressState::new("Requesting browser serial access"),
        };

        let result = match self.connector_for(LinkProviderKind::BrowserSerialEsp32) {
            Ok(connector) => match &*connector {
                LinkConnector::BrowserSerialEsp32(provider) => {
                    provider.request_access().await.map_err(map_link_error)
                }
                _ => Err(UiError::Link(
                    "browser serial connector has the wrong provider type".to_string(),
                )),
            },
            Err(error) => Err(error),
        };
        let endpoint = match result {
            Ok(endpoint) => endpoint,
            Err(UiError::Cancelled(message)) => {
                self.reset_to_provider_selection(None);
                return Ok(LinkOpenOutcome::Cancelled { message });
            }
            Err(error) => {
                self.recover_to_provider_selection(error.message());
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
    async fn open_browser_serial_provider(&mut self) -> Result<LinkOpenOutcome, UiError> {
        Err(UiError::UnsupportedFeature(
            "browser serial ESP32 access requires the browser-serial-esp32 feature on wasm"
                .to_string(),
        ))
    }

    pub async fn connect_endpoint(
        &mut self,
        provider_id: LinkProviderKind,
        endpoint_id: LinkEndpointId,
    ) -> Result<ConnectedLink, UiError> {
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

        let result = match self.connector_for(provider_id) {
            Ok(connector) => open_connected_provider(provider_id, &connector, &endpoint_id).await,
            Err(error) => Err(error),
        };
        let (session, connection, logs) = match result {
            Ok(result) => result,
            Err(error) => {
                self.active_session = None;
                self.active_connection = None;
                self.recover_to_provider_selection(error.message());
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
    ) -> Result<LinkManagementOutcome, UiError> {
        self.manage_with_updates(request, progress_label, UxUpdateSink::noop())
            .await
    }

    pub async fn manage_with_updates(
        &mut self,
        request: LinkManagementRequest,
        progress_label: impl Into<String>,
        updates: UxUpdateSink,
    ) -> Result<LinkManagementOutcome, UiError> {
        let provider_id = self
            .active_provider
            .ok_or_else(|| UiError::MissingSession("link provider is not selected".to_string()))?;
        let session_id = self
            .active_session
            .as_ref()
            .map(|session| session.id.clone())
            .ok_or_else(|| UiError::MissingSession("link session is not open".to_string()))?;
        let device = self.connected_device_summary()?;
        let progress_label = progress_label.into();
        self.active_connection = None;
        self.state = LinkState::Managing {
            device: device.clone(),
            progress: ProgressState::new(progress_label.clone()),
        };
        let node_id = ControllerId::new(Self::NODE_ID);
        let activity = Rc::new(RefCell::new(
            UiActivityView::new(progress_label.clone())
                .with_progress(UiProgress::indeterminate(progress_label.clone())),
        ));
        updates.emit(UxUpdate::Activity {
            target: crate::UxActivityTarget::pane(node_id.clone()),
            status: UiStatus::working("Managing"),
            activity: activity.borrow().clone(),
        });
        let event_sink = management_activity_sink(node_id, activity, updates);

        let result = match self.connector_for(provider_id) {
            Ok(connector) => connector
                .manage_with_events(&session_id, request, event_sink)
                .await
                .map_err(map_link_error),
            Err(error) => Err(error),
        };
        self.state = LinkState::Connected { device };
        let result = result?;
        let logs = management_result_logs(&result);
        Ok(LinkManagementOutcome { result, logs })
    }

    pub async fn reopen_active_connection(&mut self) -> Result<ConnectedLink, UiError> {
        let provider_id = self
            .active_provider
            .ok_or_else(|| UiError::MissingSession("link provider is not selected".to_string()))?;
        let session_id = self
            .active_session
            .as_ref()
            .map(|session| session.id.clone())
            .ok_or_else(|| UiError::MissingSession("link session is not open".to_string()))?;
        let connector = self.connector_for(provider_id)?;
        open_provider_protocol_if_needed(provider_id, &connector, &session_id).await?;
        let connection = connector
            .connection(&session_id)
            .await
            .map_err(map_link_error)?;
        let logs = link_session_logs(&connector, &session_id)?;
        self.active_connection = Some(connection.clone());
        Ok(ConnectedLink { connection, logs })
    }

    pub fn fail(&mut self, message: impl Into<String>) {
        self.state = LinkState::Failed {
            issue: UiIssue::new(message),
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

    fn connected_device_summary(&self) -> Result<ConnectedDeviceSummary, UiError> {
        match &self.state {
            LinkState::Connected { device } | LinkState::Managing { device, .. } => {
                Ok(device.clone())
            }
            _ => Err(UiError::MissingSession(
                "link is not connected to a device".to_string(),
            )),
        }
    }
}

pub struct ConnectedLink {
    pub connection: LinkConnection,
    pub logs: Vec<UiLogDraft>,
}

pub enum LinkOpenOutcome {
    Opened,
    Connected(ConnectedLink),
    Cancelled { message: String },
}

pub struct LinkManagementOutcome {
    pub result: LinkManagementResult,
    pub logs: Vec<UiLogDraft>,
}

impl Default for LinkController {
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

fn management_result_logs(result: &LinkManagementResult) -> Vec<UiLogDraft> {
    match result {
        LinkManagementResult::FlashFirmware(result) => {
            let mut logs = result
                .logs
                .iter()
                .map(|message| {
                    UiLogDraft::new(UiLogLevel::Info, UiLogOrigin::Link, message.clone())
                })
                .collect::<Vec<_>>();
            logs.extend(result.progress.iter().map(|progress| {
                UiLogDraft::new(UiLogLevel::Info, UiLogOrigin::Link, progress.label.clone())
            }));
            logs
        }
        LinkManagementResult::EraseDeviceFlash(result) => {
            let mut logs = result
                .logs
                .iter()
                .map(|message| {
                    UiLogDraft::new(UiLogLevel::Info, UiLogOrigin::Link, message.clone())
                })
                .collect::<Vec<_>>();
            logs.extend(result.progress.iter().map(|progress| {
                UiLogDraft::new(UiLogLevel::Info, UiLogOrigin::Link, progress.label.clone())
            }));
            logs
        }
        LinkManagementResult::EraseRawFilesystem(result) => {
            let mut logs = result
                .logs
                .iter()
                .map(|message| {
                    UiLogDraft::new(UiLogLevel::Info, UiLogOrigin::Link, message.clone())
                })
                .collect::<Vec<_>>();
            logs.extend(result.progress.iter().map(|progress| {
                UiLogDraft::new(UiLogLevel::Info, UiLogOrigin::Link, progress.label.clone())
            }));
            logs
        }
        LinkManagementResult::ResetRuntime => {
            vec![UiLogDraft::new(
                UiLogLevel::Info,
                UiLogOrigin::Link,
                "runtime reset completed",
            )]
        }
    }
}

fn management_activity_sink(
    node_id: ControllerId,
    activity: Rc<RefCell<UiActivityView>>,
    updates: UxUpdateSink,
) -> LinkManagementEventSink {
    LinkManagementEventSink::new(move |event| {
        let log_update = management_event_log(&event);
        let mut activity = activity.borrow_mut();
        apply_management_event(&mut activity, event);
        updates.emit(UxUpdate::Activity {
            target: crate::UxActivityTarget::pane(node_id.clone()),
            status: UiStatus::working("Managing"),
            activity: (*activity).clone(),
        });
        if let Some(log) = log_update {
            updates.emit(UxUpdate::Log(log));
        }
    })
}

fn apply_management_event(activity: &mut UiActivityView, event: LinkManagementEvent) {
    match event {
        LinkManagementEvent::Log { .. } => {}
        LinkManagementEvent::Progress(progress) => {
            let mut ux_progress = match progress.percent {
                Some(percent) => UiProgress::determinate(progress.label, percent),
                None => UiProgress::indeterminate(progress.label),
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

fn management_event_log(event: &LinkManagementEvent) -> Option<UiLogDraft> {
    match event {
        LinkManagementEvent::Log { message } if !message.trim().is_empty() => Some(
            UiLogDraft::new(UiLogLevel::Info, UiLogOrigin::Link, message.clone()),
        ),
        _ => None,
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

fn provider_auto_connects(kind: LinkProviderKind) -> bool {
    matches!(
        kind,
        LinkProviderKind::BrowserWorker | LinkProviderKind::HostProcess
    )
}

async fn open_connected_provider(
    provider_id: LinkProviderKind,
    connector: &LinkConnector,
    endpoint_id: &LinkEndpointId,
) -> Result<(LinkSession, LinkConnection, Vec<UiLogDraft>), UiError> {
    let session = connector
        .connect(endpoint_id)
        .await
        .map_err(map_link_error)?;
    if let Err(error) = open_provider_protocol_if_needed(provider_id, connector, session.id()).await
    {
        close_failed_session(connector, session.id()).await;
        return Err(error);
    }
    let connection = match connector.connection(session.id()).await {
        Ok(connection) => connection,
        Err(error) => {
            close_failed_session(connector, session.id()).await;
            return Err(map_link_error(error));
        }
    };
    let logs = match link_session_logs(connector, session.id()) {
        Ok(logs) => logs,
        Err(error) => {
            close_failed_session(connector, session.id()).await;
            return Err(error);
        }
    };
    Ok((session, connection, logs))
}

async fn close_failed_session(connector: &LinkConnector, session_id: &LinkSessionId) {
    let _ = connector.close(session_id).await;
}

#[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
async fn open_provider_protocol_if_needed(
    provider_id: LinkProviderKind,
    connector: &LinkConnector,
    session_id: &LinkSessionId,
) -> Result<(), UiError> {
    if provider_id != LinkProviderKind::BrowserSerialEsp32 {
        return Ok(());
    }
    let LinkConnector::BrowserSerialEsp32(provider) = connector else {
        return Err(UiError::Link(
            "browser serial connector has the wrong provider type".to_string(),
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
    _connector: &LinkConnector,
    _session_id: &LinkSessionId,
) -> Result<(), UiError> {
    let _ = provider_id;
    Ok(())
}

fn link_session_logs(
    connector: &LinkConnector,
    session_id: &lpa_link::LinkSessionId,
) -> Result<Vec<UiLogDraft>, UiError> {
    let mut logs = connector
        .logs(session_id)
        .map_err(map_link_error)?
        .into_iter()
        .map(link_log_draft)
        .collect::<Vec<_>>();
    logs.extend(
        connector
            .diagnostics(session_id)
            .map_err(map_link_error)?
            .into_iter()
            .map(|diagnostic| {
                UiLogDraft::new(
                    map_diagnostic_level(diagnostic.severity),
                    UiLogOrigin::Link,
                    diagnostic.message,
                )
            }),
    );
    Ok(logs)
}

fn map_link_error(error: LinkError) -> UiError {
    match error {
        LinkError::Cancelled { message } => UiError::Cancelled(message),
        _ => UiError::Link(error.to_string()),
    }
}

/// Map a provider log entry to a console draft: origin `Link`, the endpoint
/// id as display-only detail.
///
/// The session id is deliberately omitted from the detail: providers derive
/// session ids from the endpoint id plus a counter (`{endpoint}:{n}`), and
/// the studio drives at most one session per endpoint, so an
/// `endpoint/session` detail would only repeat the endpoint stem and widen
/// the console's source column.
fn link_log_draft(entry: lpa_link::LinkLogEntry) -> UiLogDraft {
    UiLogDraft::new(
        map_link_log_level(entry.level),
        UiLogSource::with_detail(UiLogOrigin::Link, entry.endpoint_id.as_str()),
        entry.message,
    )
}

/// Link log levels map one-to-one; `Trace` is preserved (it collapsed to
/// `Debug` before the console gained a Trace level).
fn map_link_log_level(level: LinkLogLevel) -> UiLogLevel {
    match level {
        LinkLogLevel::Trace => UiLogLevel::Trace,
        LinkLogLevel::Debug => UiLogLevel::Debug,
        LinkLogLevel::Info => UiLogLevel::Info,
        LinkLogLevel::Warn => UiLogLevel::Warn,
        LinkLogLevel::Error => UiLogLevel::Error,
    }
}

fn map_diagnostic_level(level: LinkDiagnosticSeverity) -> UiLogLevel {
    match level {
        LinkDiagnosticSeverity::Info => UiLogLevel::Info,
        LinkDiagnosticSeverity::Warning => UiLogLevel::Warn,
        LinkDiagnosticSeverity::Error => UiLogLevel::Error,
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
        LinkCapabilities, LinkConnectionKind, LinkEndpoint, LinkManagementEvent,
        LinkManagementRequest, LinkProviderKind, LinkSession,
    };

    use super::*;

    #[test]
    fn failed_management_returns_to_recoverable_connected_state() {
        let mut link = LinkController::with_registry(registry_with_fake_endpoint());
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

        let result =
            block_on_ready(link.manage(LinkManagementRequest::EraseDeviceFlash, "Wiping device"));

        assert!(matches!(result, Err(UiError::Link(_))));
        assert!(matches!(link.state(), LinkState::Connected { .. }));
    }

    #[test]
    fn management_log_events_are_ux_logs_not_activity_terminal_lines() {
        let mut activity = UiActivityView::new("Flashing firmware");
        let event = LinkManagementEvent::log("Writing at 0x10000... (42%)");

        let log = management_event_log(&event).expect("log event should produce a UX log");
        apply_management_event(&mut activity, event);

        assert_eq!(log.source, UiLogSource::new(UiLogOrigin::Link));
        assert_eq!(log.message, "Writing at 0x10000... (42%)");
        assert!(activity.terminal.is_empty());
    }

    #[test]
    fn link_log_drafts_preserve_trace_and_carry_endpoint_detail() {
        let entry = lpa_link::LinkLogEntry::new(
            "usb-serial-0",
            Some(LinkSessionId::new("usb-serial-0:1")),
            LinkLogLevel::Trace,
            "probe ok",
        );

        let draft = link_log_draft(entry);

        assert_eq!(draft.level, UiLogLevel::Trace);
        assert_eq!(
            draft.source,
            UiLogSource::with_detail(UiLogOrigin::Link, "usb-serial-0")
        );
        assert_eq!(draft.message, "probe ok");
    }

    #[test]
    fn link_log_levels_map_one_to_one() {
        assert_eq!(map_link_log_level(LinkLogLevel::Trace), UiLogLevel::Trace);
        assert_eq!(map_link_log_level(LinkLogLevel::Debug), UiLogLevel::Debug);
        assert_eq!(map_link_log_level(LinkLogLevel::Info), UiLogLevel::Info);
        assert_eq!(map_link_log_level(LinkLogLevel::Warn), UiLogLevel::Warn);
        assert_eq!(map_link_log_level(LinkLogLevel::Error), UiLogLevel::Error);
    }

    #[test]
    fn cancelled_link_error_maps_to_cancelled_ux_error() {
        let error = map_link_error(LinkError::cancelled("Port selection canceled"));

        assert_eq!(
            error,
            UiError::Cancelled("Port selection canceled".to_string())
        );
    }

    #[test]
    fn failed_endpoint_discovery_returns_to_provider_selection_with_issue() {
        let mut link = LinkController::with_registry(registry_with_fake_discover_error(
            "serial discovery failed",
        ));

        let result = block_on_ready(link.open_provider(LinkProviderKind::Fake));

        assert!(matches!(result, Err(UiError::Link(_))));
        assert!(matches!(
            link.state(),
            LinkState::SelectingProvider {
                issue: Some(issue),
                ..
            } if issue.message.contains("serial discovery failed")
        ));
    }

    #[test]
    fn failed_endpoint_connect_returns_to_provider_selection_with_issue() {
        let mut link = LinkController::with_registry(registry_with_fake_connect_error(
            "Failed to open serial port.",
        ));

        let result = block_on_ready(
            link.connect_endpoint(LinkProviderKind::Fake, LinkEndpointId::new("fake-runtime")),
        );

        assert!(matches!(result, Err(UiError::Link(_))));
        assert!(matches!(
            link.state(),
            LinkState::SelectingProvider {
                issue: Some(issue),
                ..
            } if issue.message.contains("Failed to open serial port")
        ));
    }

    #[test]
    fn failed_connection_handoff_returns_to_provider_selection_with_issue() {
        let mut link = LinkController::with_registry(registry_with_fake_connection_error(
            "server handoff failed",
        ));

        let result = block_on_ready(
            link.connect_endpoint(LinkProviderKind::Fake, LinkEndpointId::new("fake-runtime")),
        );

        assert!(matches!(result, Err(UiError::Link(_))));
        assert!(matches!(
            link.state(),
            LinkState::SelectingProvider {
                issue: Some(issue),
                ..
            } if issue.message.contains("server handoff failed")
        ));
    }

    #[test]
    fn snapshot_projects_provider_catalog_from_registry() {
        let link = LinkController::with_registry(registry_with_fake_endpoint());

        assert!(matches!(
            link.snapshot().state,
            LinkState::SelectingProvider { ref providers, .. }
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
