use dioxus::prelude::*;
use lp_studio_core::{
    DeviceAccessStatus, DeviceFlowState, DeviceIssue, ProgressState, ProjectSelectionReason,
    ProviderAvailability, ProviderCardState, ProviderIntent, ProvisioningReason, RecoveryAction,
    RecoveryReason, StudioDiagnostic, StudioDiagnosticSeverity, StudioLogEntry, StudioState,
};
use lpa_link::{LinkEndpointId, LinkProviderId};

#[component]
pub fn DevicePanel(
    state: StudioState,
    running: bool,
    on_refresh_catalog: EventHandler<MouseEvent>,
    on_start_provider: EventHandler<LinkProviderId>,
    on_confirm_firmware_flash: EventHandler<(LinkEndpointId, Option<String>)>,
    on_load_starter_project: EventHandler<MouseEvent>,
) -> Element {
    let providers = state.device_manager.providers.providers.clone();
    let selected_provider_id = state
        .device_manager
        .providers
        .selected_provider_id()
        .cloned();
    let issues = state.device_manager.issues.clone();
    let diagnostics = state
        .diagnostics
        .iter()
        .rev()
        .take(3)
        .cloned()
        .collect::<Vec<_>>();
    let logs = state.logs.iter().rev().take(5).cloned().collect::<Vec<_>>();
    let access = state
        .device_access
        .as_ref()
        .map(|access| access_status_label(&access.status))
        .unwrap_or_else(|| "not requested".to_string());

    rsx! {
        section { class: "panel device-panel",
            div { class: "panel-heading",
                h2 { "Device Manager" }
                div { class: "button-row",
                    button {
                        class: "secondary-button",
                        disabled: running,
                        onclick: move |event| on_refresh_catalog.call(event),
                        "Refresh"
                    }
                }
            }

            div { class: "device-section provider-section",
                h3 { "Providers" }
                if providers.is_empty() {
                    div { class: "empty-state", "No providers reported yet." }
                } else {
                    div { class: "provider-grid",
                        for provider in providers {
                            {
                                let selected = selected_provider_id
                                    .as_ref()
                                    .map(|selected| *selected == provider.provider_id)
                                    .unwrap_or(false);
                                rsx! {
                            ProviderCard {
                                provider,
                                selected,
                                running,
                                on_start_provider,
                            }
                                }
                            }
                        }
                    }
                }
            }

            div { class: "device-section",
                h3 { "Flow" }
                FlowStateView {
                    flow: state.device_manager.active_flow.clone(),
                    running,
                    on_confirm_firmware_flash,
                    on_load_starter_project,
                }
            }

            div { class: "device-section device-summary",
                h3 { "Session" }
                dl {
                    dt { "Access" }
                    dd { "{access}" }
                    dt { "Device" }
                    dd {
                        {
                            state.device_manager.current_device.as_ref()
                                .map(|device| device.device_id.as_str().to_string())
                                .unwrap_or_else(|| "none".to_string())
                        }
                    }
                    dt { "Endpoint" }
                    dd {
                        {
                            state.device_manager.current_device.as_ref()
                                .map(|device| device.endpoint_id.as_str().to_string())
                                .unwrap_or_else(|| "none".to_string())
                        }
                    }
                    dt { "Session" }
                    dd {
                        {
                            state.device_manager.current_device.as_ref()
                                .map(|device| device.session_id.as_str().to_string())
                                .unwrap_or_else(|| "none".to_string())
                        }
                    }
                }
            }

            if !issues.is_empty() {
                div { class: "device-section issue-list",
                    h3 { "Issues" }
                    for issue in issues {
                        IssueView { issue }
                    }
                }
            }

            if !diagnostics.is_empty() {
                div { class: "device-section diagnostic-list",
                    h3 { "Diagnostics" }
                    for diagnostic in diagnostics {
                        DiagnosticView { diagnostic }
                    }
                }
            }

            div { class: "device-section hardware-log",
                h3 { "Hardware Log" }
                if logs.is_empty() {
                    div { class: "empty-state", "No device log entries." }
                } else {
                    ol { class: "hardware-log-list",
                        for entry in logs {
                            LogLine { entry }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ProviderCard(
    provider: ProviderCardState,
    selected: bool,
    running: bool,
    on_start_provider: EventHandler<LinkProviderId>,
) -> Element {
    let can_start = provider.availability.can_start();
    let class = if selected {
        "provider-card is-selected"
    } else {
        "provider-card"
    };
    let provider_id = provider.provider_id.clone();
    let availability = availability_label(&provider.availability);
    let intent = intent_label(&provider.intent);
    let endpoint_count = provider.endpoints.len();
    let button_label = if running && selected {
        "Running"
    } else {
        "Use"
    };

    rsx! {
        article { class,
            div { class: "provider-card-header",
                h4 { "{provider.label}" }
                span { class: "mini-count", "{endpoint_count}" }
            }
            p { "{intent}" }
            div { class: "provider-card-meta", "{availability}" }
            button {
                disabled: running || !can_start,
                onclick: move |_| on_start_provider.call(provider_id.clone()),
                "{button_label}"
            }
        }
    }
}

#[component]
fn FlowStateView(
    flow: DeviceFlowState,
    running: bool,
    on_confirm_firmware_flash: EventHandler<(LinkEndpointId, Option<String>)>,
    on_load_starter_project: EventHandler<MouseEvent>,
) -> Element {
    let summary = flow_summary(&flow);
    rsx! {
        div { class: "flow-card",
            div { class: "flow-card-main",
                span { class: "flow-kicker", "{summary.stage}" }
                strong { "{summary.title}" }
                p { "{summary.detail}" }
            }
            if let Some(progress) = summary.progress {
                ProgressView { progress }
            }
            match flow {
                DeviceFlowState::ProjectSelectionRequired { reason, projects, .. } => rsx! {
                    ProjectSelectionView {
                        reason,
                        projects,
                        running,
                        on_load_starter_project,
                    }
                },
                DeviceFlowState::RecoveryRequired { reason, .. } => rsx! {
                    div { class: "recovery-box",
                        strong { "Recovery" }
                        p { "{recovery_reason_label(&reason)}" }
                    }
                },
                DeviceFlowState::ProvisioningRequired { endpoint_id, reason } => rsx! {
                    div { class: "recovery-box",
                        strong { "Provisioning" }
                        p { "{provisioning_reason_label(&reason)}" }
                        button {
                            disabled: running,
                            onclick: move |_| on_confirm_firmware_flash.call((endpoint_id.clone(), None)),
                            "Flash firmware"
                        }
                    }
                },
                DeviceFlowState::FlashConfirm { endpoint_id, firmware_id } => rsx! {
                    div { class: "recovery-box",
                        strong { "Flash firmware" }
                        p { "{flash_confirmation_label(firmware_id.as_deref())}" }
                        button {
                            disabled: running,
                            onclick: move |_| {
                                on_confirm_firmware_flash.call((endpoint_id.clone(), firmware_id.clone()))
                            },
                            "Confirm flash"
                        }
                    }
                },
                _ => rsx! {},
            }
        }
    }
}

#[component]
fn ProjectSelectionView(
    reason: ProjectSelectionReason,
    projects: Vec<lp_studio_core::ProjectChoice>,
    running: bool,
    on_load_starter_project: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        div { class: "project-selection-box",
            div {
                strong { "{project_selection_reason_label(&reason)}" }
                if !projects.is_empty() {
                    ul { class: "choice-list",
                        for project in projects {
                            li { "{project.project_id} ({project.server_path})" }
                        }
                    }
                }
            }
            button {
                disabled: running,
                onclick: move |event| on_load_starter_project.call(event),
                "Load starter"
            }
        }
    }
}

#[component]
fn ProgressView(progress: ProgressState) -> Element {
    let percent = progress.percent.unwrap_or(0);
    let steps = progress
        .total_steps
        .map(|total| format!("{} / {total}", progress.completed_steps))
        .unwrap_or_else(|| progress.completed_steps.to_string());
    rsx! {
        div { class: "progress-box",
            div { class: "progress-label",
                span { "{progress.label}" }
                span { "{steps}" }
            }
            div { class: "progress-track",
                div {
                    class: "progress-fill",
                    style: "--progress: {percent}%;",
                }
            }
        }
    }
}

#[component]
fn IssueView(issue: DeviceIssue) -> Element {
    rsx! {
        article { class: "issue-card",
            strong { "{issue.message}" }
            if !issue.recovery_actions.is_empty() {
                div { class: "recovery-actions",
                    for action in issue.recovery_actions {
                        span { class: "mini-count", "{recovery_action_label(&action)}" }
                    }
                }
            }
        }
    }
}

#[component]
fn DiagnosticView(diagnostic: StudioDiagnostic) -> Element {
    rsx! {
        article { class: "diagnostic-card",
            strong { "{diagnostic_severity_label(&diagnostic.severity)}" }
            p { "{diagnostic.message}" }
        }
    }
}

#[component]
fn LogLine(entry: StudioLogEntry) -> Element {
    rsx! {
        li {
            span { class: "log-level", "{entry.level:?}" }
            span { class: "log-target", "{entry.target}" }
            span { "{entry.message}" }
        }
    }
}

fn diagnostic_severity_label(severity: &StudioDiagnosticSeverity) -> &'static str {
    match severity {
        StudioDiagnosticSeverity::Info => "Info",
        StudioDiagnosticSeverity::Warning => "Warning",
        StudioDiagnosticSeverity::Error => "Error",
    }
}

struct FlowSummary {
    stage: &'static str,
    title: String,
    detail: String,
    progress: Option<ProgressState>,
}

fn flow_summary(flow: &DeviceFlowState) -> FlowSummary {
    match flow {
        DeviceFlowState::Empty | DeviceFlowState::ChoosingProvider => summary(
            "Choose",
            "Pick a runtime",
            "Simulator and hardware providers appear here when available.",
        ),
        DeviceFlowState::ProviderSelected { provider_id } => {
            summary("Selected", provider_id.as_str(), "Provider selected.")
        }
        DeviceFlowState::RequestingAccess { provider_id } => summary(
            "Access",
            provider_id.as_str(),
            "Waiting for provider access.",
        ),
        DeviceFlowState::AccessFailed { issue, .. } => {
            summary("Access", "Access failed", &issue.message)
        }
        DeviceFlowState::EndpointGranted { endpoint_id, .. } => {
            summary("Endpoint", endpoint_id.as_str(), "Endpoint granted.")
        }
        DeviceFlowState::OpeningLink { endpoint_id } => {
            summary("Link", endpoint_id.as_str(), "Opening link session.")
        }
        DeviceFlowState::LinkFailed { issue, .. } => summary("Link", "Link failed", &issue.message),
        DeviceFlowState::ProbingTarget { endpoint_id } => {
            summary("Probe", endpoint_id.as_str(), "Identifying target.")
        }
        DeviceFlowState::ProvisioningRequired { reason, .. } => summary(
            "Provision",
            "Provisioning required",
            &provisioning_reason_label(reason),
        ),
        DeviceFlowState::FlashConfirm { endpoint_id, .. } => summary(
            "Flash",
            endpoint_id.as_str(),
            "Waiting for flash confirmation.",
        ),
        DeviceFlowState::Flashing {
            endpoint_id,
            progress,
        } => summary_with_progress(
            "Flash",
            endpoint_id.as_str(),
            "Flashing firmware.",
            progress,
        ),
        DeviceFlowState::OpeningServer { endpoint_id } => {
            summary("Server", endpoint_id.as_str(), "Opening server protocol.")
        }
        DeviceFlowState::ServerReady { session_id } => {
            summary("Server", session_id.as_str(), "Server link ready.")
        }
        DeviceFlowState::ReadingProjectState { session_id } => {
            summary("Project", session_id.as_str(), "Reading project state.")
        }
        DeviceFlowState::ProjectSelectionRequired {
            reason, projects, ..
        } => summary(
            "Project",
            project_selection_reason_label(reason),
            &format!("{} candidate project(s).", projects.len()),
        ),
        DeviceFlowState::RecoveryRequired { reason, .. } => summary(
            "Recovery",
            "Recovery required",
            &recovery_reason_label(reason),
        ),
        DeviceFlowState::DeployingProject {
            project_id,
            progress,
        } => summary_with_progress("Project", project_id, "Deploying project.", progress),
        DeviceFlowState::Ready { project_id } => summary("Ready", project_id, "Project attached."),
        DeviceFlowState::Degraded { issue } => summary("Issue", "Degraded", &issue.message),
        DeviceFlowState::Disconnected { reason } => summary(
            "Offline",
            "Disconnected",
            reason.as_deref().unwrap_or("No active device session."),
        ),
    }
}

fn summary(stage: &'static str, title: &str, detail: &str) -> FlowSummary {
    FlowSummary {
        stage,
        title: title.to_string(),
        detail: detail.to_string(),
        progress: None,
    }
}

fn summary_with_progress(
    stage: &'static str,
    title: &str,
    detail: &str,
    progress: &Option<ProgressState>,
) -> FlowSummary {
    FlowSummary {
        stage,
        title: title.to_string(),
        detail: detail.to_string(),
        progress: progress.clone(),
    }
}

fn access_status_label(status: &DeviceAccessStatus) -> String {
    match status {
        DeviceAccessStatus::Unknown => "unknown".to_string(),
        DeviceAccessStatus::Unsupported { reason } => format!("unsupported: {reason}"),
        DeviceAccessStatus::PermissionRequired => "permission required".to_string(),
        DeviceAccessStatus::PermissionCanceled { reason } => format!("canceled: {reason}"),
        DeviceAccessStatus::PermissionDenied { reason } => format!("denied: {reason}"),
        DeviceAccessStatus::Granted => "granted".to_string(),
    }
}

fn availability_label(availability: &ProviderAvailability) -> String {
    match availability {
        ProviderAvailability::Available => "available".to_string(),
        ProviderAvailability::AvailableWithPermission => "permission required".to_string(),
        ProviderAvailability::Unavailable { reason, .. } => reason.clone(),
        ProviderAvailability::HiddenInThisBuild => "hidden in this build".to_string(),
    }
}

fn intent_label(intent: &ProviderIntent) -> String {
    match intent {
        ProviderIntent::SimulateInBrowser => "Browser simulator".to_string(),
        ProviderIntent::ConnectUsbEsp32 => "USB ESP32".to_string(),
        ProviderIntent::RunHostRuntime => "Host runtime".to_string(),
        ProviderIntent::ConnectHostSerialEsp32 => "Host serial ESP32".to_string(),
        ProviderIntent::ConnectRemoteServer => "Remote server".to_string(),
        ProviderIntent::Other { label } => label.clone(),
    }
}

fn project_selection_reason_label(reason: &ProjectSelectionReason) -> &'static str {
    match reason {
        ProjectSelectionReason::NoLoadedProject => "No project loaded",
        ProjectSelectionReason::MultipleLoadedProjects => "Choose a project",
    }
}

fn provisioning_reason_label(reason: &ProvisioningReason) -> String {
    match reason {
        ProvisioningReason::DeviceBlank => "Blank device.".to_string(),
        ProvisioningReason::BootloaderMode => "Device is in bootloader mode.".to_string(),
        ProvisioningReason::FirmwareMissing => "LightPlayer firmware is missing.".to_string(),
        ProvisioningReason::FirmwareIncompatible { version } => version
            .as_ref()
            .map(|version| format!("Firmware {version} is incompatible."))
            .unwrap_or_else(|| "Firmware is incompatible.".to_string()),
        ProvisioningReason::ServerUnavailable => "Server is unavailable.".to_string(),
        ProvisioningReason::UserRequested => "Provisioning requested.".to_string(),
        ProvisioningReason::Other { message } => message.clone(),
    }
}

fn flash_confirmation_label(firmware_id: Option<&str>) -> String {
    firmware_id
        .map(|firmware_id| {
            format!("Ready to write {firmware_id}. Existing device data may be lost.")
        })
        .unwrap_or_else(|| {
            "Ready to write LightPlayer firmware. Existing device data may be lost.".to_string()
        })
}

fn recovery_reason_label(reason: &RecoveryReason) -> String {
    match reason {
        RecoveryReason::SafeMode { message } => message
            .clone()
            .unwrap_or_else(|| "Server is in safe mode.".to_string()),
        RecoveryReason::ProjectCrash {
            project_id,
            message,
        } => {
            let project = project_id.as_deref().unwrap_or("previous project");
            message
                .clone()
                .unwrap_or_else(|| format!("{project} crashed on the previous run."))
        }
        RecoveryReason::BootLoopDetected { message } => message
            .clone()
            .unwrap_or_else(|| "Boot loop detected.".to_string()),
        RecoveryReason::FirmwarePanic { message } => message
            .clone()
            .unwrap_or_else(|| "Firmware panic recorded.".to_string()),
    }
}

fn recovery_action_label(action: &RecoveryAction) -> String {
    match action {
        RecoveryAction::Retry => "retry".to_string(),
        RecoveryAction::ChooseSimulator => "simulator".to_string(),
        RecoveryAction::ChooseProvider { provider_id } => provider_id.as_str().to_string(),
        RecoveryAction::UseCompatibleBrowser => "compatible browser".to_string(),
        RecoveryAction::Reconnect => "reconnect".to_string(),
        RecoveryAction::FlashFirmware { .. } => "flash firmware".to_string(),
        RecoveryAction::ResetDevice => "reset".to_string(),
        RecoveryAction::Disconnect => "disconnect".to_string(),
        RecoveryAction::OpenHelp { topic } => topic.clone(),
        RecoveryAction::Ignore => "ignore".to_string(),
    }
}
