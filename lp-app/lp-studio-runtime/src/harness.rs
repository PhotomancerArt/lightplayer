use lp_studio_core::{
    ActionOrigin, HOST_PROCESS_PROVIDER_ID, StudioActionKind, StudioApp, StudioEffect,
};
use lpa_link::LinkProviderId;

use crate::effect_executor::EffectExecutor;
use crate::{HostProcessStudioRuntime, StudioRuntimeError};

pub struct RuntimeHarness {
    app: StudioApp,
    runtime: HostProcessStudioRuntime,
}

impl RuntimeHarness {
    pub fn host_process() -> Self {
        let mut app = StudioApp::new();
        app.dispatch_kind(
            StudioActionKind::SelectLinkProvider {
                provider_id: LinkProviderId::new(HOST_PROCESS_PROVIDER_ID),
            },
            ActionOrigin::Harness,
        );
        Self {
            app,
            runtime: HostProcessStudioRuntime::new(),
        }
    }

    pub fn app(&self) -> &StudioApp {
        &self.app
    }

    pub fn runtime_mut(&mut self) -> &mut HostProcessStudioRuntime {
        &mut self.runtime
    }

    pub async fn dispatch(
        &mut self,
        action: StudioActionKind,
        origin: ActionOrigin,
    ) -> Result<(), StudioRuntimeError> {
        let effects = self.app.dispatch_kind(action, origin);
        self.drain_effects(effects).await
    }

    async fn drain_effects(
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

pub async fn run_host_process_demo() -> Result<StudioApp, StudioRuntimeError> {
    let mut harness = RuntimeHarness::host_process();
    harness
        .dispatch(StudioActionKind::DiscoverDevices, ActionOrigin::Harness)
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
            StudioActionKind::ConnectDevice { endpoint_id },
            ActionOrigin::Harness,
        )
        .await?;
    harness
        .dispatch(StudioActionKind::LoadDemoProject, ActionOrigin::Harness)
        .await?;
    harness.runtime.close().await?;
    Ok(harness.app)
}
