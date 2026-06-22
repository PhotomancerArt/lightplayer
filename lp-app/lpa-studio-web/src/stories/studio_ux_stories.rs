use dioxus::prelude::*;
use lpa_studio_ux::{
    ConnectedDeviceSummary, EndpointChoice, LinkProviderKind, LinkState, LinkUx,
    LoadedProjectChoice, ProgressState, ProjectInventorySummary, ProjectState, ProjectUx,
    ProviderChoice, ServerState, ServerUx, StudioView, UxAction, UxIssue, UxLogEntry, UxLogLevel,
    UxPaneView,
};

use crate::components::{ActionStrip, StudioShell, UxPane};
use crate::stories::story::StoryDescriptor;

pub const STORIES: &[StoryDescriptor] = &[
    StoryDescriptor::new(
        "studio/actions/provider-actions",
        "Studio UX",
        "Provider actions",
        "Generic action strip for provider choices exposed by Link UX.",
    ),
    StoryDescriptor::new(
        "studio/panes/link",
        "Studio UX",
        "Link pane",
        "Link pane rendered directly from the Link UX view.",
    ),
    StoryDescriptor::new(
        "studio/panes/server",
        "Studio UX",
        "Server pane",
        "Server pane rendered directly from the Server UX view.",
    ),
    StoryDescriptor::new(
        "studio/panes/project",
        "Studio UX",
        "Project pane",
        "Project pane rendered directly from the Project UX view.",
    ),
    StoryDescriptor::new(
        "studio/panes/project-selection",
        "Studio UX",
        "Project selection",
        "Loaded project choices exposed as Project UX actions.",
    ),
    StoryDescriptor::new(
        "studio/simulator-idle",
        "Studio UX",
        "Simulator idle",
        "Initial Studio UX state before launching the browser simulator.",
    ),
    StoryDescriptor::new(
        "studio/simulator-endpoint",
        "Studio UX",
        "Simulator endpoint",
        "Endpoint choices returned by the selected lpa-link provider.",
    ),
    StoryDescriptor::new(
        "studio/simulator-starting",
        "Studio UX",
        "Simulator starting",
        "Progress state while the selected endpoint is opening.",
    ),
    StoryDescriptor::new(
        "studio/simulator-ready",
        "Studio UX",
        "Simulator ready",
        "Connected simulator after the UX layer auto-loads the demo project.",
    ),
    StoryDescriptor::new(
        "studio/server-disconnected-link-ready",
        "Studio UX",
        "Server disconnected",
        "Open link session with the server protocol detached and reconnect action available.",
    ),
    StoryDescriptor::new(
        "studio/project-ready",
        "Studio UX",
        "Project ready",
        "Demo project loaded and summarized through the UX view.",
    ),
    StoryDescriptor::new(
        "studio/error",
        "Studio UX",
        "Action error",
        "Failure state shown through the same shell and action surface.",
    ),
];

pub fn render_story(id: &str) -> Option<Element> {
    match id {
        "studio/actions/provider-actions" => {
            return Some(rsx! {
                section { class: "ux-panel ux-panel-primary",
                    div { class: "ux-panel-heading",
                        p { "Actions" }
                        h2 { "Provider choices" }
                    }
                    ActionStrip {
                        actions: start_actions(),
                        running: false,
                        on_action: move |_| {},
                    }
                }
            });
        }
        "studio/panes/link" => {
            let view = link_view(idle_link_state(), false);
            return Some(rsx! {
                UxPane {
                    view,
                    primary: true,
                    running: false,
                    on_action: move |_| {},
                }
            });
        }
        "studio/panes/server" => {
            let view = server_view(ServerState::Connected {
                protocol: "fw-browser-post-message-v1".to_string(),
            });
            return Some(rsx! {
                UxPane {
                    view,
                    primary: false,
                    running: false,
                    on_action: move |_| {},
                }
            });
        }
        "studio/panes/project" => {
            let view = project_view(ProjectState::NotLoaded, true);
            return Some(rsx! {
                UxPane {
                    view,
                    primary: false,
                    running: false,
                    on_action: move |_| {},
                }
            });
        }
        "studio/panes/project-selection" => {
            let view = project_view(project_selection_state(), true);
            return Some(rsx! {
                UxPane {
                    view,
                    primary: false,
                    running: false,
                    on_action: move |_| {},
                }
            });
        }
        _ => {}
    }

    let (view, running, error, notices) = match id {
        "studio/simulator-idle" => (idle_view(), false, None, Vec::new()),
        "studio/simulator-endpoint" => (endpoint_view(), false, None, Vec::new()),
        "studio/simulator-starting" => (starting_view(), true, None, Vec::new()),
        "studio/simulator-ready" => (
            simulator_ready_view(),
            false,
            None,
            vec![
                "Simulator is running".to_string(),
                "Demo project loaded".to_string(),
            ],
        ),
        "studio/server-disconnected-link-ready" => (
            server_disconnected_link_ready_view(),
            false,
            None,
            vec!["Server disconnected".to_string()],
        ),
        "studio/project-ready" => (
            project_ready_view(),
            false,
            None,
            vec!["Demo project loaded".to_string()],
        ),
        "studio/error" => (
            error_view(),
            false,
            Some("browser worker boot timed out".to_string()),
            Vec::new(),
        ),
        _ => return None,
    };
    Some(rsx! {
        StudioShell {
            view,
            running,
            error,
            notices,
            on_action: move |_| {},
        }
    })
}

fn idle_view() -> StudioView {
    studio_view(
        idle_link_state(),
        ServerState::Disconnected,
        ProjectState::NotLoaded,
        false,
        Vec::new(),
    )
}

fn starting_view() -> StudioView {
    studio_view(
        LinkState::Connecting {
            endpoint: EndpointChoice::browser_worker(),
            progress: ProgressState::new("Opening link session"),
        },
        ServerState::Connecting {
            progress: ProgressState::new("Opening server protocol"),
        },
        ProjectState::NotLoaded,
        false,
        vec![UxLogEntry::new(
            UxLogLevel::Info,
            "lpa-link",
            "browser worker session created",
        )],
    )
}

fn endpoint_view() -> StudioView {
    studio_view(
        LinkState::SelectingEndpoint {
            provider_id: LinkProviderKind::BrowserWorker,
            endpoints: vec![EndpointChoice::browser_worker()],
        },
        ServerState::Disconnected,
        ProjectState::NotLoaded,
        false,
        Vec::new(),
    )
}

fn simulator_ready_view() -> StudioView {
    studio_view(
        connected_link_state(),
        ServerState::Connected {
            protocol: "fw-browser-post-message-v1".to_string(),
        },
        project_ready_state(),
        true,
        vec![
            UxLogEntry::new(UxLogLevel::Info, "fw-browser", "ready"),
            UxLogEntry::new(
                UxLogLevel::Info,
                "lpa-link",
                "browser worker session owns Worker lifecycle in lpa-link",
            ),
            UxLogEntry::new(UxLogLevel::Info, "fw-browser", "project loaded"),
        ],
    )
}

fn project_ready_view() -> StudioView {
    studio_view(
        connected_link_state(),
        ServerState::Connected {
            protocol: "fw-browser-post-message-v1".to_string(),
        },
        project_ready_state(),
        true,
        vec![
            UxLogEntry::new(UxLogLevel::Info, "fw-browser", "project loaded"),
            UxLogEntry::new(
                UxLogLevel::Debug,
                "lp-server",
                "heartbeat frame=42 uptime_ms=700",
            ),
        ],
    )
}

fn server_disconnected_link_ready_view() -> StudioView {
    studio_view(
        connected_link_state(),
        ServerState::Disconnected,
        ProjectState::NotLoaded,
        false,
        vec![UxLogEntry::new(
            UxLogLevel::Info,
            "lpa-studio-ux",
            "server protocol detached; link session remains open",
        )],
    )
}

fn error_view() -> StudioView {
    studio_view(
        LinkState::Failed {
            issue: UxIssue::new("browser worker boot timed out"),
        },
        ServerState::Failed {
            issue: UxIssue::new("server protocol was not opened"),
        },
        ProjectState::NotLoaded,
        false,
        vec![UxLogEntry::new(
            UxLogLevel::Error,
            "lpa-studio-ux",
            "browser worker boot timed out",
        )],
    )
}

fn studio_view(
    link_state: LinkState,
    server_state: ServerState,
    project_state: ProjectState,
    server_connected: bool,
    logs: Vec<UxLogEntry>,
) -> StudioView {
    StudioView::new(
        vec![
            link_view(link_state, server_connected),
            server_view(server_state),
            project_view(project_state, server_connected),
        ],
        logs,
    )
}

fn link_view(state: LinkState, server_connected: bool) -> UxPaneView {
    let mut link = LinkUx::new();
    link.set_state(state);
    link.view(server_connected)
}

fn server_view(state: ServerState) -> UxPaneView {
    let mut server = ServerUx::new();
    server.set_state(state);
    server.view()
}

fn project_view(state: ProjectState, server_connected: bool) -> UxPaneView {
    let mut project = ProjectUx::new();
    let no_running_project = matches!(state, ProjectState::NotLoaded) && server_connected;
    project.set_state(state);
    if no_running_project {
        project.mark_no_running_project();
    }
    project.view(server_connected)
}

fn idle_link_state() -> LinkState {
    LinkState::SelectingProvider {
        providers: vec![
            ProviderChoice::browser_worker(),
            ProviderChoice {
                id: LinkProviderKind::BrowserSerialEsp32,
                label: "ESP32".to_string(),
                summary: "Connect to ESP32 hardware through browser Web Serial.".to_string(),
            },
        ],
    }
}

fn connected_link_state() -> LinkState {
    let provider_id = ProviderChoice::browser_worker().id;
    LinkState::Connected {
        device: ConnectedDeviceSummary::new(
            provider_id,
            "browser-worker-worker-1",
            "browser-worker-worker-1:1",
            "Browser firmware runtime",
        ),
    }
}

fn project_selection_state() -> ProjectState {
    ProjectState::SelectingLoadedProject {
        projects: vec![
            LoadedProjectChoice::new("/projects/ambient", 1),
            LoadedProjectChoice::new("/projects/palette-test", 2),
        ],
    }
}

fn project_ready_state() -> ProjectState {
    ProjectState::Ready {
        project_id: "studio-demo".to_string(),
        handle_id: 1,
        inventory: ProjectInventorySummary {
            node_count: 4,
            definition_count: 3,
            asset_count: 1,
        },
    }
}

fn start_actions() -> Vec<UxAction> {
    link_view(idle_link_state(), false).actions
}
