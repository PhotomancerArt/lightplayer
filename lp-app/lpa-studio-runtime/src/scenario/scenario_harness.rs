use lpa_link::LinkEndpointId;
use lpa_studio_core::{
    ActionOrigin, LinkState, StudioActionKind, StudioApp, StudioEffect, StudioEvent,
};

use crate::StudioRuntimeError;
use crate::effect_executor::EffectExecutor;
use crate::harness::RuntimeHarness;
use crate::scenario::{ProvisioningScenario, ScenarioRuntime, ScenarioSnapshot};

/// Convenience harness for executing a `ProvisioningScenario` as a user journey.
pub struct ScenarioHarness {
    harness: RuntimeHarness<ScenarioRuntime>,
    snapshots: Vec<ScenarioSnapshot>,
}

impl ScenarioHarness {
    pub fn new(scenario: ProvisioningScenario) -> Self {
        let runtime = ScenarioRuntime::new(scenario);
        let harness = RuntimeHarness::with_runtime(runtime);
        Self {
            harness,
            snapshots: Vec::new(),
        }
    }

    pub fn app(&self) -> &StudioApp {
        self.harness.app()
    }

    pub fn runtime(&self) -> &ScenarioRuntime {
        self.harness.runtime()
    }

    pub fn snapshots(&self) -> &[ScenarioSnapshot] {
        &self.snapshots
    }

    pub async fn refresh_catalog(&mut self) -> Result<(), StudioRuntimeError> {
        self.dispatch(
            StudioActionKind::RefreshProviderCatalog,
            ActionOrigin::Harness,
        )
        .await
    }

    pub async fn start_default_provider(&mut self) -> Result<(), StudioRuntimeError> {
        let provider_id = self
            .runtime()
            .scenario()
            .primary_provider_id()
            .cloned()
            .ok_or_else(|| StudioRuntimeError::Link("scenario has no provider".to_string()))?;
        self.dispatch(
            StudioActionKind::StartProvisioning { provider_id },
            ActionOrigin::Harness,
        )
        .await
    }

    pub async fn connect_selected_endpoint(&mut self) -> Result<(), StudioRuntimeError> {
        let endpoint_id = self.selected_endpoint_id()?;
        self.dispatch(
            StudioActionKind::ConnectDevice { endpoint_id },
            ActionOrigin::Harness,
        )
        .await
    }

    pub async fn probe_current_target(&mut self) -> Result<(), StudioRuntimeError> {
        self.dispatch(
            StudioActionKind::ProbeTarget { endpoint_id: None },
            ActionOrigin::Harness,
        )
        .await
    }

    pub async fn confirm_firmware_flash(
        &mut self,
        firmware_id: Option<String>,
    ) -> Result<(), StudioRuntimeError> {
        let endpoint_id = self.active_endpoint_id()?;
        self.dispatch(
            StudioActionKind::ConfirmFirmwareFlash {
                endpoint_id,
                firmware_id,
            },
            ActionOrigin::Harness,
        )
        .await
    }

    pub async fn load_demo_project(&mut self) -> Result<(), StudioRuntimeError> {
        self.dispatch(StudioActionKind::LoadDemoProject, ActionOrigin::Harness)
            .await
    }

    pub async fn read_project_state(&mut self) -> Result<(), StudioRuntimeError> {
        self.dispatch(StudioActionKind::ReadProjectState, ActionOrigin::Harness)
            .await
    }

    pub async fn refresh_status(&mut self) -> Result<(), StudioRuntimeError> {
        self.dispatch(StudioActionKind::RefreshStatus, ActionOrigin::Harness)
            .await
    }

    pub async fn disconnect_device(&mut self) -> Result<(), StudioRuntimeError> {
        self.dispatch(StudioActionKind::DisconnectDevice, ActionOrigin::Harness)
            .await
    }

    pub async fn dispatch(
        &mut self,
        action: StudioActionKind,
        origin: ActionOrigin,
    ) -> Result<(), StudioRuntimeError> {
        let action_type = action.action_type();
        let effects = self.harness.app_mut().dispatch_kind(action, origin);
        self.record(format!("{action_type:?} dispatched"));
        self.drain_effects(effects).await
    }

    async fn drain_effects(
        &mut self,
        mut effects: Vec<StudioEffect>,
    ) -> Result<(), StudioRuntimeError> {
        while let Some(effect) = effects.pop() {
            let events = self.harness.runtime_mut().execute_effect(effect).await?;
            for event in events {
                let label = event_label(&event);
                effects.extend(self.harness.app_mut().apply_event(event));
                self.record(label);
            }
        }
        Ok(())
    }

    fn selected_endpoint_id(&self) -> Result<LinkEndpointId, StudioRuntimeError> {
        self.app()
            .state()
            .device_manager
            .providers
            .first_selected_endpoint()
            .map(|endpoint| endpoint.id.clone())
            .ok_or_else(|| {
                StudioRuntimeError::Link("scenario has no selected endpoint".to_string())
            })
    }

    fn active_endpoint_id(&self) -> Result<LinkEndpointId, StudioRuntimeError> {
        match &self.app().state().device_manager.active_flow {
            LinkState::ProvisioningRequired { endpoint_id, .. }
            | LinkState::FlashConfirm { endpoint_id, .. }
            | LinkState::Flashing { endpoint_id, .. }
            | LinkState::OpeningServer { endpoint_id }
            | LinkState::OpeningLink { endpoint_id }
            | LinkState::ProbingTarget { endpoint_id }
            | LinkState::EndpointGranted { endpoint_id, .. } => Ok(endpoint_id.clone()),
            _ => self
                .app()
                .state()
                .connection_session
                .as_ref()
                .map(|session| session.endpoint_id.clone())
                .or_else(|| {
                    self.app()
                        .state()
                        .device_manager
                        .providers
                        .first_selected_endpoint()
                        .map(|endpoint| endpoint.id.clone())
                })
                .ok_or_else(|| {
                    StudioRuntimeError::Link("scenario has no active endpoint".to_string())
                }),
        }
    }

    fn record(&mut self, label: impl Into<String>) {
        self.snapshots
            .push(ScenarioSnapshot::from_app(label, self.harness.app()));
    }
}

fn event_label(event: &StudioEvent) -> String {
    match event {
        StudioEvent::ProviderCatalogUpdated { .. } => "provider catalog updated",
        StudioEvent::ProviderAvailabilityUpdated { .. } => "provider availability updated",
        StudioEvent::DeviceAccessUpdated { .. } => "device access updated",
        StudioEvent::EndpointsDiscovered { .. } => "endpoints discovered",
        StudioEvent::DeviceConnected { .. } => "device connected",
        StudioEvent::DeviceConnectionFailed { .. } => "device connection failed",
        StudioEvent::DeviceDisconnected { .. } => "device disconnected",
        StudioEvent::DeviceReset { .. } => "device reset",
        StudioEvent::FirmwareFlashCompleted { .. } => "firmware flash completed",
        StudioEvent::TargetProbeCompleted { .. } => "target probe completed",
        StudioEvent::TargetProbeFailed { .. } => "target probe failed",
        StudioEvent::ProvisioningIssueRaised { .. } => "link issue raised",
        StudioEvent::ProvisioningProgressUpdated { .. } => "link progress updated",
        StudioEvent::ProvisioningFlowCanceled { .. } => "link flow canceled",
        StudioEvent::DemoProjectSeeded { .. } => "demo project seeded",
        StudioEvent::ProjectLoaded { .. } => "project loaded",
        StudioEvent::ProjectInventoryRead { .. } => "project inventory read",
        StudioEvent::LoadedProjectsRefreshed { .. } => "loaded projects refreshed",
        StudioEvent::ProjectStateRead { .. } => "project state read",
        StudioEvent::HeartbeatReceived { .. } => "heartbeat received",
        StudioEvent::LogReceived { .. } => "log received",
        StudioEvent::DiagnosticRaised { .. } => "diagnostic raised",
        StudioEvent::ActionFailed { .. } => "ux failed",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::{
        DeviceIssueKind, LinkState, ProjectSelectionReason, ProvisioningReason,
        STUDIO_DEMO_PROJECT_ID,
    };

    use super::*;

    #[tokio::test]
    async fn provider_catalog_refresh_records_providers() {
        let mut harness = ScenarioHarness::new(ProvisioningScenario::host_runtime_ready());

        harness.refresh_catalog().await.unwrap();

        assert_eq!(
            harness
                .app()
                .state()
                .device_manager
                .providers
                .providers
                .len(),
            1
        );
        assert!(matches!(
            harness.app().state().device_manager.active_flow,
            LinkState::ChooseProvider
        ));
    }

    #[tokio::test]
    async fn unsupported_runtime_reaches_access_failed() {
        let mut harness = ScenarioHarness::new(ProvisioningScenario::web_serial_unsupported());

        harness.refresh_catalog().await.unwrap();
        harness.start_default_provider().await.unwrap();

        assert_active_issue_kind(&harness, DeviceIssueKind::RuntimeUnsupported);
        assert!(matches!(
            harness.app().state().device_manager.active_flow,
            LinkState::AccessFailed { .. }
        ));
    }

    #[tokio::test]
    async fn permission_denied_and_canceled_are_typed() {
        let mut denied = ScenarioHarness::new(ProvisioningScenario::permission_denied());
        denied.refresh_catalog().await.unwrap();
        denied.start_default_provider().await.unwrap();

        let mut canceled = ScenarioHarness::new(ProvisioningScenario::permission_canceled());
        canceled.refresh_catalog().await.unwrap();
        canceled.start_default_provider().await.unwrap();

        assert_active_issue_kind(&denied, DeviceIssueKind::PermissionDenied);
        assert_active_issue_kind(&canceled, DeviceIssueKind::PermissionCanceled);
    }

    #[tokio::test]
    async fn ready_scenario_reaches_ready_flow() {
        let mut harness = ScenarioHarness::new(ProvisioningScenario::host_runtime_ready());

        harness.refresh_catalog().await.unwrap();
        harness.start_default_provider().await.unwrap();
        harness.connect_selected_endpoint().await.unwrap();
        harness.read_project_state().await.unwrap();

        assert!(matches!(
            harness.app().state().device_manager.active_flow,
            LinkState::Ready { ref project_id }
                if project_id == STUDIO_DEMO_PROJECT_ID
        ));
        assert_flow_snapshot(&harness, |flow| {
            matches!(flow, LinkState::GrantPermission { .. })
        });
        assert_flow_snapshot(&harness, |flow| {
            matches!(flow, LinkState::EndpointGranted { .. })
        });
        assert_flow_snapshot(&harness, |flow| {
            matches!(flow, LinkState::OpeningLink { .. })
        });
        assert_flow_snapshot(&harness, |flow| {
            matches!(flow, LinkState::ServerReady { .. })
        });
        assert_flow_snapshot(&harness, |flow| {
            matches!(flow, LinkState::ReadingProjectState { .. })
        });
    }

    #[tokio::test]
    async fn no_loaded_project_requires_project_selection() {
        let mut harness = ScenarioHarness::new(ProvisioningScenario::no_loaded_project());

        connect_ready_server(&mut harness).await;
        harness.read_project_state().await.unwrap();

        assert!(matches!(
            harness.app().state().device_manager.active_flow,
            LinkState::ProjectSelectionRequired {
                reason: ProjectSelectionReason::NoLoadedProject,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn multiple_loaded_projects_require_project_selection() {
        let mut harness = ScenarioHarness::new(ProvisioningScenario::multiple_loaded_projects());

        connect_ready_server(&mut harness).await;
        harness.read_project_state().await.unwrap();

        assert!(matches!(
            &harness.app().state().device_manager.active_flow,
            LinkState::ProjectSelectionRequired {
                reason: ProjectSelectionReason::MultipleLoadedProjects,
                projects,
                ..
            } if projects.len() == 2
        ));
    }

    #[tokio::test]
    async fn recovery_required_enters_recovery_flow() {
        let mut harness = ScenarioHarness::new(ProvisioningScenario::recovery_required());

        connect_ready_server(&mut harness).await;
        harness.read_project_state().await.unwrap();

        assert!(matches!(
            harness.app().state().device_manager.active_flow,
            LinkState::RecoveryRequired { .. }
        ));
    }

    #[tokio::test]
    async fn blank_target_requires_provisioning() {
        let mut harness =
            ScenarioHarness::new(ProvisioningScenario::blank_device_flash_available());

        connect_and_probe(&mut harness).await;

        assert!(matches!(
            harness.app().state().device_manager.active_flow,
            LinkState::ProvisioningRequired {
                reason: ProvisioningReason::DeviceBlank,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn flash_success_opens_server() {
        let mut harness = ScenarioHarness::new(ProvisioningScenario::flash_succeeds());

        connect_and_probe(&mut harness).await;
        harness.confirm_firmware_flash(None).await.unwrap();

        assert_flow_snapshot(&harness, |flow| matches!(flow, LinkState::Flashing { .. }));
        assert!(matches!(
            harness.app().state().device_manager.active_flow,
            LinkState::OpeningServer { .. }
        ));
    }

    #[tokio::test]
    async fn flash_unavailable_and_failure_raise_flash_issues() {
        let mut unavailable = ScenarioHarness::new(ProvisioningScenario::flash_unavailable());
        connect_and_probe(&mut unavailable).await;
        unavailable.confirm_firmware_flash(None).await.unwrap();

        let mut missing = ScenarioHarness::new(ProvisioningScenario::flash_artifact_missing());
        connect_and_probe(&mut missing).await;
        missing.confirm_firmware_flash(None).await.unwrap();

        let mut failed = ScenarioHarness::new(ProvisioningScenario::flash_fails());
        connect_and_probe(&mut failed).await;
        failed.confirm_firmware_flash(None).await.unwrap();

        assert_active_issue_kind(&unavailable, DeviceIssueKind::FlashFailed);
        assert_active_issue_kind(&missing, DeviceIssueKind::FirmwareArtifactMissing);
        assert_active_issue_kind(&failed, DeviceIssueKind::FlashFailed);
    }

    #[tokio::test]
    async fn post_flash_reconnect_failure_degrades_after_opening_server() {
        let mut harness = ScenarioHarness::new(ProvisioningScenario::flash_reconnect_fails());

        connect_and_probe(&mut harness).await;
        harness.confirm_firmware_flash(None).await.unwrap();

        assert_flow_snapshot(&harness, |flow| {
            matches!(flow, LinkState::OpeningServer { .. })
        });
        assert_active_issue_kind(&harness, DeviceIssueKind::ConnectionLost);
        assert!(matches!(
            harness.app().state().device_manager.active_flow,
            LinkState::Degraded { .. }
        ));
    }

    #[tokio::test]
    async fn post_flash_success_can_read_project_state() {
        let mut harness = ScenarioHarness::new(ProvisioningScenario::flash_succeeds());

        connect_and_probe(&mut harness).await;
        harness.confirm_firmware_flash(None).await.unwrap();
        harness.read_project_state().await.unwrap();

        assert!(matches!(
            harness.app().state().device_manager.active_flow,
            LinkState::Ready { .. }
        ));
        assert_flow_snapshot(&harness, |flow| {
            matches!(flow, LinkState::OpeningServer { .. })
        });
        assert_flow_snapshot(&harness, |flow| {
            matches!(flow, LinkState::ReadingProjectState { .. })
        });
    }

    #[tokio::test]
    async fn post_flash_project_state_failure_degrades() {
        let mut harness =
            ScenarioHarness::new(ProvisioningScenario::post_flash_project_state_fails());

        connect_and_probe(&mut harness).await;
        harness.confirm_firmware_flash(None).await.unwrap();
        harness.read_project_state().await.unwrap();

        assert_active_issue_kind(&harness, DeviceIssueKind::ServerTimeout);
        assert!(matches!(
            harness.app().state().device_manager.active_flow,
            LinkState::Degraded { .. }
        ));
    }

    #[tokio::test]
    async fn endpoint_open_failure_sets_link_failed_flow() {
        let mut harness = ScenarioHarness::new(ProvisioningScenario::endpoint_open_failure());

        harness.refresh_catalog().await.unwrap();
        harness.start_default_provider().await.unwrap();
        harness.connect_selected_endpoint().await.unwrap();

        assert_active_issue_kind(&harness, DeviceIssueKind::EndpointOpenFailed);
        assert!(matches!(
            harness.app().state().device_manager.active_flow,
            LinkState::LinkFailed { .. }
        ));
    }

    #[tokio::test]
    async fn server_timeout_sets_link_failed_flow() {
        let mut harness = ScenarioHarness::new(ProvisioningScenario::server_timeout());

        connect_and_probe(&mut harness).await;

        assert_active_issue_kind(&harness, DeviceIssueKind::ServerTimeout);
        assert!(matches!(
            harness.app().state().device_manager.active_flow,
            LinkState::LinkFailed { .. }
        ));
    }

    #[tokio::test]
    async fn incompatible_firmware_requires_provisioning_with_issue() {
        let mut harness = ScenarioHarness::new(ProvisioningScenario::incompatible_firmware());

        connect_and_probe(&mut harness).await;

        assert_active_issue_kind(&harness, DeviceIssueKind::IncompatibleFirmware);
        assert!(matches!(
            harness.app().state().device_manager.active_flow,
            LinkState::ProvisioningRequired {
                reason: ProvisioningReason::FirmwareIncompatible { .. },
                ..
            }
        ));
    }

    #[tokio::test]
    async fn project_deploy_and_load_failures_raise_project_issues() {
        let mut deploy = ScenarioHarness::new(ProvisioningScenario::project_deploy_fails());
        connect_ready_server(&mut deploy).await;
        deploy.load_demo_project().await.unwrap();

        let mut load = ScenarioHarness::new(ProvisioningScenario::project_load_fails());
        connect_ready_server(&mut load).await;
        load.load_demo_project().await.unwrap();

        assert_active_issue_kind(&deploy, DeviceIssueKind::ProjectDeployFailed);
        assert_active_issue_kind(&load, DeviceIssueKind::ProjectLoadFailed);
    }

    #[tokio::test]
    async fn connection_lost_degrades_flow() {
        let mut harness = ScenarioHarness::new(ProvisioningScenario::connection_lost());

        connect_ready_server(&mut harness).await;
        harness.refresh_status().await.unwrap();

        assert_active_issue_kind(&harness, DeviceIssueKind::ConnectionLost);
        assert!(matches!(
            harness.app().state().device_manager.active_flow,
            LinkState::Degraded { .. }
        ));
    }

    async fn connect_and_probe(harness: &mut ScenarioHarness) {
        connect_ready_server(harness).await;
        harness.probe_current_target().await.unwrap();
    }

    async fn connect_ready_server(harness: &mut ScenarioHarness) {
        harness.refresh_catalog().await.unwrap();
        harness.start_default_provider().await.unwrap();
        harness.connect_selected_endpoint().await.unwrap();
    }

    fn assert_active_issue_kind(harness: &ScenarioHarness, kind: DeviceIssueKind) {
        let issue = harness
            .app()
            .state()
            .device_manager
            .issues
            .last()
            .expect("scenario issue");
        assert_eq!(issue.kind, kind);
    }

    fn assert_flow_snapshot(harness: &ScenarioHarness, predicate: impl Fn(&LinkState) -> bool) {
        assert!(
            harness
                .snapshots()
                .iter()
                .any(|snapshot| predicate(&snapshot.flow)),
            "missing flow snapshot; snapshots: {:?}",
            harness.snapshots()
        );
    }
}
