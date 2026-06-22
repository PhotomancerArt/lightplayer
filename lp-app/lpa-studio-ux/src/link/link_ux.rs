use std::cell::RefCell;
use std::rc::Rc;

use lpa_link::providers::{LinkEnv, LinkProviderInstance, LinkProviderRegistry};
use lpa_link::{
    LinkConnection, LinkDiagnosticSeverity, LinkEndpointId, LinkLogLevel, LinkProvider,
    LinkProviderKind, LinkSession, LinkSessionId,
};
#[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
use lpc_model::DEFAULT_SERIAL_BAUD_RATE;

use crate::{
    ActionPriority, ConnectedDeviceSummary, EndpointChoice, LinkOp, LinkSnapshot, LinkState,
    ProgressState, ProviderChoice, UxAction, UxError, UxIssue, UxLogEntry, UxLogLevel, UxNode,
    UxNodeId,
};

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

    pub fn actions(&self) -> Vec<UxAction> {
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
            | LinkState::Connected { .. } => Vec::new(),
        }
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

        let endpoints = {
            let mut registry = self.registry.borrow_mut();
            let provider = registry
                .provider_mut(provider_id)
                .ok_or_else(|| missing_provider(provider_id))?;
            provider.discover().await.map_err(map_link_error)?
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

        let endpoint = {
            let mut registry = self.registry.borrow_mut();
            let provider = registry
                .provider_mut(LinkProviderKind::BrowserSerialEsp32)
                .ok_or_else(|| missing_provider(LinkProviderKind::BrowserSerialEsp32))?;
            let LinkProviderInstance::BrowserSerialEsp32(provider) = provider else {
                return Err(UxError::Link(
                    "browser serial registry entry has the wrong provider type".to_string(),
                ));
            };
            provider.request_access().await.map_err(map_link_error)?
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

        let (session, connection, logs) = {
            let mut registry = self.registry.borrow_mut();
            let provider = registry
                .provider_mut(provider_id)
                .ok_or_else(|| missing_provider(provider_id))?;
            let session = provider
                .connect(&endpoint_id)
                .await
                .map_err(map_link_error)?;
            open_provider_protocol_if_needed(provider_id, provider, session.id()).await?;
            let connection = provider
                .connection(session.id())
                .await
                .map_err(map_link_error)?;
            let logs = link_session_logs(provider, session.id())?;
            (session, connection, logs)
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
    use lpa_link::providers::LinkProviderRegistry;
    use lpa_link::providers::fake::FakeProvider;
    use lpa_link::{LinkEndpoint, LinkProviderKind};

    use super::*;

    #[test]
    fn selecting_provider_offers_registry_provider_actions() {
        let link = LinkUx::with_registry(registry_with_fake_endpoint());

        let actions = link.actions();

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
}
