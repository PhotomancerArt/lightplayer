//! Stories for loaded-project workspace states.

use dioxus::prelude::*;
use lpa_studio_core::{
    ControllerId, DirtySummary, ProjectController, ProjectOp, ProjectSyncPhase, UiAction,
    UiLogLevel, UiPaneAction,
};
use lpa_studio_web_story_macros::story;

use crate::app::project::ProjectSidebar;
use crate::app::story_fixtures::{
    device_project_empty_view, device_project_selection_view, project_editor_fixture,
    project_ready_state, project_ready_view, project_sync_failed_view, project_syncing_view,
    project_view, shell_story, studio_log,
};
use crate::core::PaneView;

#[story]
pub(crate) fn project_pane() -> Element {
    let view = project_view(project_ready_state(), true);
    rsx! {
        PaneView {
            view,
            primary: false,
            running: false,
            on_action: move |_| {},
        }
    }
}

#[story]
pub(crate) fn device_project_empty() -> Element {
    let view = device_project_empty_view();
    rsx! {
        PaneView {
            view,
            primary: true,
            running: false,
            on_action: move |_| {},
        }
    }
}

#[story]
pub(crate) fn device_project_selection() -> Element {
    let view = device_project_selection_view();
    rsx! {
        PaneView {
            view,
            primary: true,
            running: false,
            on_action: move |_| {},
        }
    }
}

#[story(
    description = "Sidebar with a dirty tree: project header uncommitted with Save/Revert icons; a dirty grandchild's counts bubble — unsaved yellow tint on the focused shader, live blue tint on the palette, aggregate badge on the root."
)]
pub(crate) fn sidebar_dirty_tree() -> Element {
    let mut view = project_editor_fixture(ProjectSyncPhase::Ready);
    let unsaved = DirtySummary {
        persisted: 2,
        transient: 0,
        failed: 0,
    };
    let live = DirtySummary {
        persisted: 0,
        transient: 1,
        failed: 0,
    };
    if let Some(root) = view.tree.roots.first_mut() {
        root.dirty = unsaved.merge(live);
        root.children[1].dirty = unsaved;
        root.children[2].dirty = live;
    }
    view.dirty = unsaved.merge(live);
    view.edits_in_flight = 0;
    view.header_actions = vec![
        UiPaneAction::new(
            "save",
            UiAction::from_op(
                ControllerId::new(ProjectController::NODE_ID),
                ProjectOp::SaveOverlay,
            ),
        ),
        UiPaneAction::new(
            "revert",
            UiAction::from_op(
                ControllerId::new(ProjectController::NODE_ID),
                ProjectOp::RevertAllEdits,
            )
            .with_label("Revert to saved"),
        ),
    ];

    rsx! {
        div { class: "tw:max-w-[320px]",
            ProjectSidebar {
                view,
                running: false,
                on_action: move |_| {},
            }
        }
    }
}

#[story]
pub(crate) fn project_ready() -> Element {
    shell_story(
        project_ready_view(),
        false,
        vec![studio_log(UiLogLevel::Info, "Demo project loaded")],
    )
}

#[story]
pub(crate) fn project_syncing() -> Element {
    shell_story(
        project_syncing_view(),
        true,
        vec![studio_log(UiLogLevel::Info, "Reading project shapes")],
    )
}

#[story]
pub(crate) fn project_sync_failed() -> Element {
    shell_story(
        project_sync_failed_view(),
        false,
        vec![studio_log(
            UiLogLevel::Error,
            "project sync failed: protocol timeout",
        )],
    )
}
