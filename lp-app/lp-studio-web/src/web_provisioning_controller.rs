use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use lp_studio_core::{
    ActionOrigin, LinkState, StudioActionKind, StudioApp,
    StudioEffect, StudioEvent, BROWSER_SERIAL_ESP32_PROVIDER_ID, BROWSER_WORKER_PROVIDER_ID,
};
use lp_studio_runtime::{
    BrowserSerialStudioRuntime, BrowserWorkerStudioRuntime, EffectExecutor, StudioRuntimeError,
};
use lpa_link::{LinkConnectionKind, LinkEndpointId, LinkProviderId};

/// Browser-side controller for dispatching Studio actions into web runtimes.
pub struct WebProvisioningController {
    runtime: Option<WebStudioRuntime>,
    error: Option<String>,
}

impl WebProvisioningController {
    pub fn new(worker_url: &str) -> Self {
        Self {
            runtime: Some(WebStudioRuntime::new(worker_url)),
            error: None,
        }
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    fn clear_error(&mut self) {
        self.error = None;
    }

    fn set_error(&mut self, error: StudioRuntimeError) {
        self.error = Some(error.to_string());
    }

    fn take_runtime(&mut self) -> Result<WebStudioRuntime, StudioRuntimeError> {
        self.runtime
            .take()
            .ok_or_else(|| StudioRuntimeError::Browser("web runtime is already busy".to_string()))
    }

    fn replace_runtime(&mut self, runtime: WebStudioRuntime) {
        self.runtime = Some(runtime);
    }
}

pub async fn dispatch_web_action(
    mut studio: Signal<StudioApp>,
    mut controller: Signal<WebProvisioningController>,
    kind: StudioActionKind,
    origin: ActionOrigin,
) {
    controller.write().clear_error();
    let effects = studio.write().dispatch_kind(kind, origin);
    if let Err(error) = drain_effects(studio, controller, effects).await {
        controller.write().set_error(error);
    }
}

pub async fn auto_advance_web_flow(
    studio: Signal<StudioApp>,
    controller: Signal<WebProvisioningController>,
) {
    for _ in 0..8 {
        let state = studio.read().state().clone();
        let next_action = match &state.device_manager.active_flow {
            LinkState::EndpointGranted { endpoint_id, .. } => {
                Some(StudioActionKind::ConnectDevice {
                    endpoint_id: endpoint_id.clone(),
                })
            }
            LinkState::ServerReady { .. } if is_browser_serial_connection(&state) => {
                Some(StudioActionKind::ProbeTarget { endpoint_id: None })
            }
            LinkState::ServerReady { .. } => Some(StudioActionKind::ReadProjectState),
            LinkState::OpeningServer { endpoint_id } if state.connection_session.is_none() => {
                Some(StudioActionKind::ConnectDevice {
                    endpoint_id: endpoint_id.clone(),
                })
            }
            LinkState::OpeningServer { .. } if state.connection_session.is_some() => {
                Some(StudioActionKind::ReadProjectState)
            }
            _ => None,
        };

        let Some(action) = next_action else {
            break;
        };
        dispatch_web_action(studio, controller, action, ActionOrigin::System).await;
        if controller.read().error().is_some() {
            break;
        }
    }
}

fn is_browser_serial_connection(state: &lp_studio_core::StudioState) -> bool {
    state.connection_session.as_ref().is_some_and(|session| {
        matches!(session.kind, LinkConnectionKind::BrowserSerialEsp32 { .. })
    })
}

async fn drain_effects(
    mut studio: Signal<StudioApp>,
    mut controller: Signal<WebProvisioningController>,
    mut effects: Vec<StudioEffect>,
) -> Result<(), StudioRuntimeError> {
    while let Some(effect) = effects.pop() {
        let mut runtime = controller.write().take_runtime()?;
        let result = runtime.execute_effect(effect).await;
        controller.write().replace_runtime(runtime);
        let events = result?;
        for event in events {
            effects.extend(studio.write().apply_event(event));
        }
    }
    Ok(())
}

struct WebStudioRuntime {
    browser_worker: BrowserWorkerStudioRuntime,
    browser_serial: BrowserSerialStudioRuntime,
    active_provider_id: Option<LinkProviderId>,
}

impl WebStudioRuntime {
    fn new(worker_url: &str) -> Self {
        Self {
            browser_worker: BrowserWorkerStudioRuntime::new(worker_url),
            browser_serial: BrowserSerialStudioRuntime::new(),
            active_provider_id: None,
        }
    }

    async fn execute_effect(
        &mut self,
        effect: StudioEffect,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        match effect {
            StudioEffect::RefreshProviderCatalog { action_id } => {
                self.refresh_provider_catalog(action_id).await
            }
            StudioEffect::RequestDeviceAccess {
                action_id,
                provider_id,
            } => {
                self.active_provider_id = Some(provider_id.clone());
                self.execute_for_provider(
                    &provider_id,
                    StudioEffect::RequestDeviceAccess {
                        action_id,
                        provider_id: provider_id.clone(),
                    },
                )
                .await
            }
            StudioEffect::DiscoverEndpoints {
                action_id,
                provider_id,
            } => {
                self.active_provider_id = Some(provider_id.clone());
                self.execute_for_provider(
                    &provider_id,
                    StudioEffect::DiscoverEndpoints {
                        action_id,
                        provider_id: provider_id.clone(),
                    },
                )
                .await
            }
            StudioEffect::ConnectEndpoint {
                action_id,
                endpoint_id,
            } => {
                self.activate_provider_for_endpoint(&endpoint_id);
                self.execute_active(StudioEffect::ConnectEndpoint {
                    action_id,
                    endpoint_id,
                })
                .await
            }
            effect => self.execute_active(effect).await,
        }
    }

    async fn refresh_provider_catalog(
        &mut self,
        action_id: lp_studio_core::ActionId,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        let mut providers = Vec::new();
        let mut events = Vec::new();

        for runtime_events in [
            self.browser_worker
                .execute_effect(StudioEffect::RefreshProviderCatalog { action_id })
                .await?,
            self.browser_serial
                .execute_effect(StudioEffect::RefreshProviderCatalog { action_id })
                .await?,
        ] {
            for event in runtime_events {
                match event {
                    StudioEvent::ProviderCatalogUpdated {
                        providers: next, ..
                    } => providers.extend(next),
                    other => events.push(other),
                }
            }
        }

        events.push(StudioEvent::ProviderCatalogUpdated {
            action_id: Some(action_id),
            providers,
        });
        Ok(events)
    }

    async fn execute_active(
        &mut self,
        effect: StudioEffect,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        let provider_id = self
            .active_provider_id
            .clone()
            .ok_or_else(|| StudioRuntimeError::Link("no active web provider".to_string()))?;
        self.execute_for_provider(&provider_id, effect).await
    }

    async fn execute_for_provider(
        &mut self,
        provider_id: &LinkProviderId,
        effect: StudioEffect,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        match provider_id.as_str() {
            BROWSER_WORKER_PROVIDER_ID => self.browser_worker.execute_effect(effect).await,
            BROWSER_SERIAL_ESP32_PROVIDER_ID => self.browser_serial.execute_effect(effect).await,
            other => Err(StudioRuntimeError::UnsupportedProvider(other.to_string())),
        }
    }

    fn activate_provider_for_endpoint(&mut self, endpoint_id: &LinkEndpointId) {
        let endpoint = endpoint_id.as_str();
        if endpoint.starts_with(BROWSER_WORKER_PROVIDER_ID) {
            self.active_provider_id = Some(LinkProviderId::new(BROWSER_WORKER_PROVIDER_ID));
        } else if endpoint.starts_with(BROWSER_SERIAL_ESP32_PROVIDER_ID) {
            self.active_provider_id = Some(LinkProviderId::new(BROWSER_SERIAL_ESP32_PROVIDER_ID));
        }
    }
}
