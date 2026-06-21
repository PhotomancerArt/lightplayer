//! Runtime test harness for driving a `StudioApp` through an effect executor.

#[cfg(feature = "host-process")]
use lpa_link::link_provider::LinkProviderId;
#[cfg(feature = "host-process")]
use lpa_studio_core::HOST_PROCESS_PROVIDER_ID;
use lpa_studio_core::{ActionOrigin, StudioActionKind, StudioApp, StudioEffect};
#[cfg(feature = "host-process")]
use lpa_studio_core::{LinkActionRequest, ProjectActionRequest};

#[cfg(feature = "host-process")]
use crate::HostProcessStudioRuntime;
use crate::StudioRuntimeError;
use crate::effect_executor::EffectExecutor;

/// Drives a `StudioApp` by dispatching actions and executing returned effects.
pub struct RuntimeHarness<R> {
    app: StudioApp,
    runtime: R,
}

impl<R> RuntimeHarness<R>
where
    R: EffectExecutor,
{
    pub fn new(app: StudioApp, runtime: R) -> Self {
        Self { app, runtime }
    }

    pub fn with_runtime(runtime: R) -> Self {
        Self {
            app: StudioApp::new(),
            runtime,
        }
    }

    pub fn app(&self) -> &StudioApp {
        &self.app
    }

    pub fn app_mut(&mut self) -> &mut StudioApp {
        &mut self.app
    }

    pub fn runtime(&self) -> &R {
        &self.runtime
    }

    pub fn runtime_mut(&mut self) -> &mut R {
        &mut self.runtime
    }

    pub fn into_app(self) -> StudioApp {
        self.app
    }

    pub async fn dispatch(
        &mut self,
        action: StudioActionKind,
        origin: ActionOrigin,
    ) -> Result<(), StudioRuntimeError> {
        let effects = self.app.dispatch_kind(action, origin);
        self.drain_effects(effects).await
    }

    pub async fn drain_effects(
        &mut self,
        mut effects: Vec<StudioEffect>,
    ) -> Result<(), StudioRuntimeError> {
        while let Some(effect) = effects.pop() {
            let events = self.runtime.execute_effect(effect).await?;
            for event in events {
                effects.extend(self.app.apply_event(event));
            }
        }
        Ok(())
    }
}

#[cfg(feature = "host-process")]
impl RuntimeHarness<HostProcessStudioRuntime> {
    pub fn host_process() -> Self {
        let mut app = StudioApp::new();
        app.dispatch_kind(
            StudioActionKind::from(LinkActionRequest::SelectProvider {
                provider_id: LinkProviderId::new(HOST_PROCESS_PROVIDER_ID),
            }),
            ActionOrigin::Harness,
        );
        Self {
            app,
            runtime: HostProcessStudioRuntime::new(),
        }
    }
}

#[cfg(feature = "host-process")]
pub async fn run_host_process_demo() -> Result<StudioApp, StudioRuntimeError> {
    let mut harness = RuntimeHarness::host_process();
    harness
        .dispatch(
            StudioActionKind::from(LinkActionRequest::DiscoverDevices),
            ActionOrigin::Harness,
        )
        .await?;
    let endpoint_id = harness
        .app()
        .state()
        .device_manager
        .providers
        .first_selected_endpoint()
        .ok_or_else(|| {
            StudioRuntimeError::Link("host-process discovery returned no endpoints".to_string())
        })?
        .id
        .clone();
    harness
        .dispatch(
            StudioActionKind::from(LinkActionRequest::ConnectEndpoint { endpoint_id }),
            ActionOrigin::Harness,
        )
        .await?;
    harness
        .dispatch(
            StudioActionKind::from(ProjectActionRequest::LoadDemoProject),
            ActionOrigin::Harness,
        )
        .await?;
    harness.runtime.close().await?;
    Ok(harness.app)
}
