use dioxus::prelude::*;
use lpa_studio_ux::{
    ActionMeta, ActionPriority, AvailableAction, ConnectedDeviceSummary, LinkAction, LinkSnapshot,
    LinkState, ProgressState, ProjectAction, ProjectInventorySummary, ProjectSnapshot,
    ProjectState, ProviderChoice, ServerSnapshot, ServerState, StudioAction, StudioSnapshot,
    UxIssue, UxLogEntry, UxLogLevel,
};

use crate::app::StudioShell;
use crate::stories::story::StoryDescriptor;

pub const STORIES: &[StoryDescriptor] = &[
    StoryDescriptor::new(
        "studio/simulator-idle",
        "Studio UX",
        "Simulator idle",
        "Initial Studio UX state before launching the browser simulator.",
    ),
    StoryDescriptor::new(
        "studio/simulator-starting",
        "Studio UX",
        "Simulator starting",
        "Progress state while the browser worker and server protocol are starting.",
    ),
    StoryDescriptor::new(
        "studio/simulator-ready",
        "Studio UX",
        "Simulator ready",
        "Connected simulator with the demo project action available.",
    ),
    StoryDescriptor::new(
        "studio/project-ready",
        "Studio UX",
        "Project ready",
        "Demo project loaded and summarized through the UX snapshot.",
    ),
    StoryDescriptor::new(
        "studio/error",
        "Studio UX",
        "Action error",
        "Failure state shown through the same shell and action surface.",
    ),
];

pub fn render_story(id: &str) -> Option<Element> {
    let (snapshot, actions, running, error, notices) = match id {
        "studio/simulator-idle" => (idle_snapshot(), start_actions(), false, None, Vec::new()),
        "studio/simulator-starting" => (starting_snapshot(), Vec::new(), true, None, Vec::new()),
        "studio/simulator-ready" => (
            simulator_ready_snapshot(),
            load_project_actions(),
            false,
            None,
            vec!["Simulator is running".to_string()],
        ),
        "studio/project-ready" => (
            project_ready_snapshot(),
            Vec::new(),
            false,
            None,
            vec!["Demo project loaded".to_string()],
        ),
        "studio/error" => (
            error_snapshot(),
            start_actions(),
            false,
            Some("browser worker boot timed out".to_string()),
            Vec::new(),
        ),
        _ => return None,
    };
    Some(rsx! {
        StudioShell {
            snapshot,
            actions,
            running,
            error,
            notices,
            on_action: move |_| {},
        }
    })
}

fn idle_snapshot() -> StudioSnapshot {
    StudioSnapshot::new(
        LinkSnapshot::new(LinkState::SelectingProvider {
            providers: vec![ProviderChoice::browser_worker()],
        }),
        ServerSnapshot::new(ServerState::Disconnected),
        ProjectSnapshot::new(ProjectState::NotLoaded),
        Vec::new(),
    )
}

fn starting_snapshot() -> StudioSnapshot {
    StudioSnapshot::new(
        LinkSnapshot::new(LinkState::StartingSimulator {
            progress: ProgressState::new("Starting simulator"),
        }),
        ServerSnapshot::new(ServerState::Connecting {
            progress: ProgressState::new("Opening server protocol"),
        }),
        ProjectSnapshot::new(ProjectState::NotLoaded),
        vec![UxLogEntry::new(
            UxLogLevel::Info,
            "lpa-link",
            "browser worker session created",
        )],
    )
}

fn simulator_ready_snapshot() -> StudioSnapshot {
    StudioSnapshot::new(
        connected_link_snapshot(),
        ServerSnapshot::new(ServerState::Connected {
            protocol: "fw-browser-post-message-v1".to_string(),
        }),
        ProjectSnapshot::new(ProjectState::NotLoaded),
        vec![
            UxLogEntry::new(UxLogLevel::Info, "fw-browser", "ready"),
            UxLogEntry::new(
                UxLogLevel::Info,
                "lpa-link",
                "browser worker session owns Worker lifecycle in lpa-link",
            ),
        ],
    )
}

fn project_ready_snapshot() -> StudioSnapshot {
    StudioSnapshot::new(
        connected_link_snapshot(),
        ServerSnapshot::new(ServerState::Connected {
            protocol: "fw-browser-post-message-v1".to_string(),
        }),
        ProjectSnapshot::new(ProjectState::Ready {
            project_id: "studio-demo".to_string(),
            handle_id: 1,
            inventory: ProjectInventorySummary {
                node_count: 4,
                definition_count: 3,
                asset_count: 1,
            },
        }),
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

fn error_snapshot() -> StudioSnapshot {
    StudioSnapshot::new(
        LinkSnapshot::new(LinkState::Failed {
            issue: UxIssue::new("browser worker boot timed out"),
        }),
        ServerSnapshot::new(ServerState::Failed {
            issue: UxIssue::new("server protocol was not opened"),
        }),
        ProjectSnapshot::new(ProjectState::NotLoaded),
        vec![UxLogEntry::new(
            UxLogLevel::Error,
            "lpa-studio-ux",
            "browser worker boot timed out",
        )],
    )
}

fn connected_link_snapshot() -> LinkSnapshot {
    let provider_id = ProviderChoice::browser_worker().id;
    LinkSnapshot::new(LinkState::Connected {
        device: ConnectedDeviceSummary::new(
            provider_id,
            "browser-worker-worker-1",
            "browser-worker-worker-1:1",
            "Browser firmware runtime",
        ),
    })
}

fn start_actions() -> Vec<AvailableAction<StudioAction>> {
    vec![AvailableAction::from_command(
        StudioAction::from(LinkAction::StartSimulator),
        ActionMeta::new(
            LinkAction::START_SIMULATOR,
            "Start simulator",
            "Launch a browser-local LightPlayer runtime.",
            ActionPriority::Primary,
        ),
    )]
}

fn load_project_actions() -> Vec<AvailableAction<StudioAction>> {
    vec![AvailableAction::from_command(
        StudioAction::from(ProjectAction::LoadDemoProject),
        ActionMeta::new(
            ProjectAction::LOAD_DEMO_PROJECT,
            "Load demo project",
            "Upload and run the built-in simulator project.",
            ActionPriority::Primary,
        ),
    )]
}
