use core::any::Any;

use lpa_studio_core::core::view::activity_view::{UiActivityStep, UiActivityStepState};
use lpa_studio_core::core::view::steps_view::{UiStepState, UiStepView};
use lpa_studio_core::{
    ActionClass, ActionConfirmation, ActionMeta, ActionPriority, ControllerId, ControllerOp,
    PROJECT_ACTION_DEADLINE, UiAction, UiActivityView, UiIssue, UiLogEntry, UiLogLevel,
    UiLogOrigin, UiLogSource, UiMetric, UiPaneView, UiProgress, UiStatus, UiStepsView,
    UiTerminalLine, UiViewContent,
};

/// Timestamp shared by the core story log fixtures (deterministic stories).
const STORY_LOG_TIMESTAMP: f64 = 1_720_000_000.0;

pub(crate) fn story_actions() -> Vec<UiAction> {
    vec![
        story_action(StoryOp::Primary),
        story_action(StoryOp::Secondary),
        story_action(StoryOp::Tertiary),
    ]
}

pub(crate) fn disabled_action() -> UiAction {
    story_action(StoryOp::Secondary).disabled("Connect a device before running this action.")
}

pub(crate) fn confirmation_action() -> UiAction {
    story_action(StoryOp::Primary)
        .with_label("Erase device")
        .with_summary("Erase the connected device flash.")
        .with_confirmation(ActionConfirmation::new(
            "Erase device?",
            "This removes the current firmware and project data from the connected device.",
            "Erase",
        ))
}

pub(crate) fn story_metrics() -> Vec<UiMetric> {
    vec![
        UiMetric::new("Runtime", "ESP32-C6"),
        UiMetric::new("Project", "studio-demo"),
        UiMetric::new("FPS", "936"),
        UiMetric::new("Memory", "207k free"),
    ]
}

/// Every level and origin, with and without source detail, at fixed
/// timestamps stepping across a minute boundary (so the rendered `HH:MM:SS`
/// column shows visible variation while staying deterministic).
pub(crate) fn story_logs() -> Vec<UiLogEntry> {
    vec![
        UiLogEntry::new(
            STORY_LOG_TIMESTAMP,
            UiLogLevel::Info,
            UiLogOrigin::Studio,
            "Simulator is running",
        ),
        UiLogEntry::new(
            STORY_LOG_TIMESTAMP + 1.0,
            UiLogLevel::Trace,
            UiLogSource::with_detail(UiLogOrigin::Link, "browser-serial"),
            "read 512 bytes",
        ),
        UiLogEntry::new(
            STORY_LOG_TIMESTAMP + 2.0,
            UiLogLevel::Debug,
            UiLogOrigin::Server,
            "heartbeat frame=42",
        ),
        UiLogEntry::new(
            STORY_LOG_TIMESTAMP + 3.0,
            UiLogLevel::Warn,
            UiLogOrigin::Link,
            "firmware flashing is available",
        ),
        UiLogEntry::new(
            STORY_LOG_TIMESTAMP + 63.0,
            UiLogLevel::Info,
            UiLogSource::with_detail(UiLogOrigin::Device, "fw_core::project::project_loader"),
            "project loaded in 84 ms",
        ),
        UiLogEntry::new(
            STORY_LOG_TIMESTAMP + 64.0,
            UiLogLevel::Error,
            UiLogOrigin::Studio,
            "project sync failed",
        ),
    ]
}

pub(crate) fn story_terminal_lines() -> Vec<UiTerminalLine> {
    vec![
        UiTerminalLine::new("[lpa-link] Connected to ESP32 bootloader"),
        UiTerminalLine::new("[lpa-link] Writing app image at 0x10000"),
        UiTerminalLine::new("[lpa-link] Progress 42%"),
    ]
}

pub(crate) fn story_issue() -> UiIssue {
    UiIssue::new("Project sync failed")
        .with_detail("The device timed out while Studio was reading project shape data.")
}

pub(crate) fn story_activity() -> UiActivityView {
    UiActivityView::new("Flashing firmware")
        .with_detail("Keep the device connected while Studio writes the image.")
        .with_progress(UiProgress::determinate("Writing firmware", 42))
        .with_steps(vec![
            UiActivityStep::new("connect", "Connect bootloader")
                .with_state(UiActivityStepState::Complete),
            UiActivityStep::new("erase", "Erase flash").with_state(UiActivityStepState::Complete),
            UiActivityStep::new("write", "Write firmware")
                .with_state(UiActivityStepState::Active)
                .with_detail("app image at 0x10000"),
            UiActivityStep::new("verify", "Verify image"),
        ])
        .with_terminal(story_terminal_lines())
}

pub(crate) fn story_steps() -> UiStepsView {
    UiStepsView::new(vec![
        UiStepView::new("connection", "Select connection", UiStepState::Complete)
            .with_body(UiViewContent::text("Simulator provider selected.")),
        UiStepView::new("device", "Connect device", UiStepState::Active)
            .with_body(UiViewContent::Progress(UiProgress::indeterminate(
                "Opening link session",
            )))
            .with_actions(vec![disabled_action()]),
        UiStepView::new("project", "Open project", UiStepState::Pending)
            .with_body(UiViewContent::text("Connect LightPlayer first.")),
        UiStepView::new("sync", "Sync project", UiStepState::NeedsAttention)
            .with_body(UiViewContent::Issue(story_issue()))
            .with_actions(vec![story_action(StoryOp::Retry)]),
    ])
    .with_terminal(story_terminal_lines())
}

pub(crate) fn story_pane() -> UiPaneView {
    UiPaneView::new(
        ControllerId::new("story|core|pane"),
        "Device",
        UiStatus::working("Connecting"),
        UiViewContent::Stack(Box::new(story_steps())),
        vec![confirmation_action()],
    )
}

pub(crate) fn story_action(op: StoryOp) -> UiAction {
    UiAction::from_op(ControllerId::new("story|core"), op)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum StoryOp {
    Primary,
    Secondary,
    Tertiary,
    Retry,
}

impl ControllerOp for StoryOp {
    fn default_action_meta(&self) -> ActionMeta {
        match self {
            Self::Primary => ActionMeta::new(
                "Start simulator",
                "Start the browser-local simulator.",
                ActionPriority::Primary,
            )
            .with_icon("play"),
            Self::Secondary => ActionMeta::new(
                "Refresh",
                "Refresh the current Studio state.",
                ActionPriority::Secondary,
            )
            .with_icon("refresh"),
            Self::Tertiary => ActionMeta::new(
                "Disconnect",
                "Disconnect the current session.",
                ActionPriority::Tertiary,
            )
            .with_icon("disconnect"),
            Self::Retry => ActionMeta::new(
                "Retry sync",
                "Retry the failed project sync.",
                ActionPriority::Primary,
            ),
        }
    }

    fn action_class(&self) -> ActionClass {
        // Story fixtures are display-only and never dispatched through the sync
        // engine; a plain foreground class with the standard project-action
        // deadline satisfies the compile-forced contract.
        ActionClass::Foreground {
            deadline: PROJECT_ACTION_DEADLINE,
        }
    }

    fn clone_box(&self) -> Box<dyn ControllerOp> {
        Box::new(self.clone())
    }

    fn eq_op(&self, other: &dyn ControllerOp) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}
