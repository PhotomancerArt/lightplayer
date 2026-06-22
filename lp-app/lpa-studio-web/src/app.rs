use dioxus::prelude::*;
use lpa_studio_ux::{
    ActionEnablement, ActionPriority, AvailableAction, LinkState, ProjectState, ServerState,
    StudioAction, StudioSnapshot, StudioUx, UxLogLevel,
};

const STYLE: &str = include_str!("style.css");

#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn App() -> Element {
    #[cfg(feature = "stories")]
    if crate::stories::story_book::should_show_story_book() {
        return rsx! {
            style { "{STYLE}" }
            crate::stories::story_book::StoryBook {}
        };
    }

    let model = use_signal(StudioWebModel::new);
    let snapshot = model.read().snapshot.clone();
    let actions = model.read().actions.clone();
    let running = model.read().running;
    let error = model.read().error.clone();
    let notices = model.read().notices.clone();
    let on_action = move |action: StudioAction| {
        spawn(async move {
            execute_action(model, action).await;
        });
    };

    rsx! {
        style { "{STYLE}" }
        StudioShell {
            snapshot,
            actions,
            running,
            error,
            notices,
            on_action,
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn StudioShell(
    snapshot: StudioSnapshot,
    actions: Vec<AvailableAction<StudioAction>>,
    running: bool,
    error: Option<String>,
    notices: Vec<String>,
    on_action: EventHandler<StudioAction>,
) -> Element {
    let has_error = error.is_some();
    rsx! {
        main { class: "ux-shell",
            header { class: "ux-header",
                div {
                    p { class: "ux-eyebrow", "LightPlayer Studio" }
                    h1 { "Simulator" }
                }
                div { class: status_class(running, has_error),
                    if running {
                        "Running"
                    } else if error.is_some() {
                        "Needs attention"
                    } else {
                        "Ready"
                    }
                }
            }

            if let Some(message) = error.as_ref() {
                section { class: "ux-alert ux-alert-error",
                    strong { "Action failed" }
                    p { "{message}" }
                }
            }

            if !notices.is_empty() {
                section { class: "ux-notices",
                    for notice in notices.iter() {
                        p { "{notice}" }
                    }
                }
            }

            section { class: "ux-layout",
                section { class: "ux-panel ux-panel-primary",
                    div { class: "ux-panel-heading",
                        p { "Link" }
                        h2 { "{link_title(&snapshot.link.state)}" }
                    }
                    p { class: "ux-panel-copy", "{link_detail(&snapshot.link.state)}" }
                    ActionList {
                        actions: actions.clone(),
                        running,
                        on_action,
                    }
                }

                section { class: "ux-panel",
                    div { class: "ux-panel-heading",
                        p { "Server" }
                        h2 { "{server_title(&snapshot.server.state)}" }
                    }
                    p { class: "ux-panel-copy", "{server_detail(&snapshot.server.state)}" }
                }

                section { class: "ux-panel",
                    div { class: "ux-panel-heading",
                        p { "Project" }
                        h2 { "{project_title(&snapshot.project.state)}" }
                    }
                    ProjectDetails { state: snapshot.project.state.clone() }
                }
            }

            section { class: "ux-log-panel",
                div { class: "ux-panel-heading",
                    p { "Runtime" }
                    h2 { "Recent activity" }
                }
                if snapshot.logs.is_empty() {
                    p { class: "ux-panel-copy", "No runtime messages yet." }
                } else {
                    ol { class: "ux-log-list",
                        for entry in snapshot.logs.iter().rev().take(8) {
                            li { class: log_class(entry.level),
                                span { "{log_level_label(entry.level)}" }
                                strong { "{entry.source}" }
                                p { "{entry.message}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ActionList(
    actions: Vec<AvailableAction<StudioAction>>,
    running: bool,
    on_action: EventHandler<StudioAction>,
) -> Element {
    rsx! {
        div { class: "ux-actions",
            if actions.is_empty() {
                p { class: "ux-panel-copy", "No actions are currently available." }
            } else {
                for action in actions.iter() {
                    {
                        let command = action.command.clone();
                        let disabled = running || !action.meta.enablement.is_enabled();
                        let class = action_class(action.meta.priority);
                        let disabled_reason = disabled_reason(&action.meta.enablement);
                        rsx! {
                            button {
                                class,
                                r#type: "button",
                                disabled,
                                title: "{action.meta.summary}",
                                onclick: move |_| on_action.call(command.clone()),
                                span { "{action.meta.label}" }
                            }
                            if let Some(reason) = disabled_reason {
                                p { class: "ux-disabled-reason", "{reason}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProjectDetails(state: ProjectState) -> Element {
    match state {
        ProjectState::Ready {
            project_id,
            handle_id,
            inventory,
        } => rsx! {
            dl { class: "ux-metrics",
                div {
                    dt { "Project" }
                    dd { "{project_id}" }
                }
                div {
                    dt { "Handle" }
                    dd { "{handle_id}" }
                }
                div {
                    dt { "Nodes" }
                    dd { "{inventory.node_count}" }
                }
                div {
                    dt { "Definitions" }
                    dd { "{inventory.definition_count}" }
                }
                div {
                    dt { "Assets" }
                    dd { "{inventory.asset_count}" }
                }
            }
        },
        other => rsx! {
            p { class: "ux-panel-copy", "{project_detail(&other)}" }
        },
    }
}

struct StudioWebModel {
    ux: Option<StudioUx>,
    snapshot: StudioSnapshot,
    actions: Vec<AvailableAction<StudioAction>>,
    running: bool,
    error: Option<String>,
    notices: Vec<String>,
}

impl StudioWebModel {
    fn new() -> Self {
        let ux = StudioUx::new();
        let snapshot = ux.snapshot();
        let actions = ux.actions();
        Self {
            ux: Some(ux),
            snapshot,
            actions,
            running: false,
            error: None,
            notices: Vec::new(),
        }
    }

    fn refresh_from_ux(&mut self) {
        if let Some(ux) = &self.ux {
            self.snapshot = ux.snapshot();
            self.actions = ux.actions();
        }
    }
}

async fn execute_action(mut model: Signal<StudioWebModel>, action: StudioAction) {
    let Some(mut ux) = ({
        let mut state = model.write();
        if state.running {
            return;
        }
        state.running = true;
        state.error = None;
        state.actions.clear();
        state.ux.take()
    }) else {
        model.write().error = Some("Studio UX is already busy.".to_string());
        return;
    };

    let result = ux.execute(action).await;
    let mut state = model.write();
    match result {
        Ok(outcome) => {
            state.notices = outcome
                .notices
                .into_iter()
                .map(|notice| notice.message)
                .collect();
        }
        Err(error) => {
            state.error = Some(error.to_string());
        }
    }
    state.ux = Some(ux);
    state.refresh_from_ux();
    state.running = false;
}

fn link_title(state: &LinkState) -> String {
    match state {
        LinkState::SelectingProvider { .. } => "Choose runtime".to_string(),
        LinkState::StartingSimulator { .. } => "Starting".to_string(),
        LinkState::Connected { device } => device.label.clone(),
        LinkState::Failed { .. } => "Link failed".to_string(),
    }
}

fn link_detail(state: &LinkState) -> String {
    match state {
        LinkState::SelectingProvider { providers } => providers
            .first()
            .map(|provider| provider.summary.clone())
            .unwrap_or_else(|| "No simulator providers are available.".to_string()),
        LinkState::StartingSimulator { progress } => progress
            .detail
            .clone()
            .unwrap_or_else(|| progress.label.clone()),
        LinkState::Connected { device } => {
            format!(
                "Connected through {} as {}.",
                device.provider_id, device.session_id
            )
        }
        LinkState::Failed { issue } => issue.message.clone(),
    }
}

fn server_title(state: &ServerState) -> &'static str {
    match state {
        ServerState::Disconnected => "Offline",
        ServerState::Connecting { .. } => "Connecting",
        ServerState::Connected { .. } => "Connected",
        ServerState::Failed { .. } => "Failed",
    }
}

fn server_detail(state: &ServerState) -> String {
    match state {
        ServerState::Disconnected => "Start the simulator to open the server protocol.".to_string(),
        ServerState::Connecting { progress } => progress.label.clone(),
        ServerState::Connected { protocol } => format!("Protocol: {protocol}"),
        ServerState::Failed { issue } => issue.message.clone(),
    }
}

fn project_title(state: &ProjectState) -> &'static str {
    match state {
        ProjectState::NotLoaded => "Not loaded",
        ProjectState::LoadingDemoProject { .. } => "Loading",
        ProjectState::Ready { .. } => "Ready",
        ProjectState::Failed { .. } => "Failed",
    }
}

fn project_detail(state: &ProjectState) -> String {
    match state {
        ProjectState::NotLoaded => {
            "Load the demo project after the simulator is connected.".to_string()
        }
        ProjectState::LoadingDemoProject { progress } => progress.label.clone(),
        ProjectState::Ready { .. } => "Project inventory is ready.".to_string(),
        ProjectState::Failed { issue } => issue.message.clone(),
    }
}

fn action_class(priority: ActionPriority) -> &'static str {
    match priority {
        ActionPriority::Primary => "ux-action ux-action-primary",
        ActionPriority::Secondary => "ux-action ux-action-secondary",
        ActionPriority::Tertiary => "ux-action ux-action-tertiary",
    }
}

fn disabled_reason(enablement: &ActionEnablement) -> Option<&str> {
    match enablement {
        ActionEnablement::Enabled => None,
        ActionEnablement::Disabled { reason } => Some(reason.as_str()),
    }
}

fn log_level_label(level: UxLogLevel) -> &'static str {
    match level {
        UxLogLevel::Debug => "debug",
        UxLogLevel::Info => "info",
        UxLogLevel::Warn => "warn",
        UxLogLevel::Error => "error",
    }
}

fn log_class(level: UxLogLevel) -> &'static str {
    match level {
        UxLogLevel::Debug => "ux-log ux-log-debug",
        UxLogLevel::Info => "ux-log ux-log-info",
        UxLogLevel::Warn => "ux-log ux-log-warn",
        UxLogLevel::Error => "ux-log ux-log-error",
    }
}

fn status_class(running: bool, has_error: bool) -> &'static str {
    if has_error {
        "ux-status ux-status-error"
    } else if running {
        "ux-status ux-status-running"
    } else {
        "ux-status"
    }
}
