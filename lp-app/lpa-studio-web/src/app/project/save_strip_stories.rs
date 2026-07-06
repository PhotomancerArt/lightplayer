//! Stories for the project save strip states.

use dioxus::prelude::*;
use lpa_studio_core::ProjectDirtyCounts;
use lpa_studio_web_story_macros::story;

use crate::app::project::ProjectSaveStrip;

#[story(description = "Save strip with no pending edits: buttons disabled, unchanged icon.")]
pub(crate) fn unchanged() -> Element {
    rsx! {
        ProjectSaveStrip {
            dirty: ProjectDirtyCounts::default(),
            overlay_revision: 0,
            edits_in_flight: 0,
            on_action: move |_| {},
        }
    }
}

#[story(
    description = "Save strip with pending persisted edits: count pill, enabled buttons, uncommitted icon."
)]
pub(crate) fn uncommitted() -> Element {
    rsx! {
        ProjectSaveStrip {
            dirty: ProjectDirtyCounts {
                persisted: 2,
                transient: 1,
            },
            overlay_revision: 7,
            edits_in_flight: 0,
            on_action: move |_| {},
        }
    }
}

#[story(
    description = "Save strip with only live (transient) edits: Save disabled, Revert to saved enabled."
)]
pub(crate) fn live_only() -> Element {
    rsx! {
        ProjectSaveStrip {
            dirty: ProjectDirtyCounts {
                persisted: 0,
                transient: 2,
            },
            overlay_revision: 4,
            edits_in_flight: 0,
            on_action: move |_| {},
        }
    }
}

#[story(
    description = "The placeholder detail popup: overlay revision plus per-kind sections — warning tint for unsaved, live tint for transient."
)]
pub(crate) fn detail_popup() -> Element {
    rsx! {
        div { class: "tw:flex tw:min-h-80 tw:justify-end",
            ProjectSaveStrip {
                dirty: ProjectDirtyCounts {
                    persisted: 2,
                    transient: 1,
                },
                overlay_revision: 7,
                edits_in_flight: 0,
                on_action: move |_| {},
                initially_open: true,
            }
        }
    }
}

#[story(description = "Save strip while an edit awaits its server ack: in-progress icon.")]
pub(crate) fn in_progress() -> Element {
    rsx! {
        ProjectSaveStrip {
            dirty: ProjectDirtyCounts {
                persisted: 1,
                transient: 0,
            },
            overlay_revision: 7,
            edits_in_flight: 1,
            on_action: move |_| {},
        }
    }
}
