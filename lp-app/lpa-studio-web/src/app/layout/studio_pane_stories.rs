//! Stories for the Studio pane layout component — the pane grammar's visual
//! spec, with neutral fixture content in every slot.

use dioxus::prelude::*;
use lpa_studio_core::{
    ControllerId, NodeRemoveOp, ProjectNodeAddress, ProjectOp, UiAction, UiPaneAction,
};
use lpa_studio_web_story_macros::story;

use crate::app::layout::{PaneChip, PaneChrome, PaneCollapse, PaneTone, StudioPane};
use crate::base::{DetailPopover, IconMenuTone, StudioIcon, StudioIconName};

#[story(
    description = "Every pane slot populated: collapse, primary affordance, title/kind, state chips, actions, trailing content, detail popup, body."
)]
pub(crate) fn all_slots() -> Element {
    rsx! {
        StudioPane {
            collapse: story_collapse(false),
            primary: story_primary(),
            title: "Playlist",
            kind: "Node".to_string(),
            chrome: PaneChrome {
                tone: PaneTone::Warning,
                selected: false,
                chips: vec![
                    PaneChip {
                        tone: PaneTone::Warning,
                        text: "3 unsaved".to_string(),
                        title: "Pending persisted edits".to_string(),
                    },
                    PaneChip {
                        tone: PaneTone::Live,
                        text: "1 live".to_string(),
                        title: "Touched live controls".to_string(),
                    },
                ],
            },
            actions: story_actions(),
            on_action: move |_| {},
            trailing: story_trailing(),
            detail: story_detail(),
            body: rsx! {
                p { class: "tw:m-0 tw:pt-3 tw:text-sm tw:text-muted-foreground",
                    "Neutral pane body content."
                }
            },
        }
    }
}

#[story(
    description = "Header-only pane (no collapse, no body): the project-header shape with a persistent state chip and contextual actions."
)]
pub(crate) fn header_only() -> Element {
    rsx! {
        StudioPane {
            title: "fyeah_sign.show",
            kind: "Project".to_string(),
            chrome: PaneChrome {
                tone: PaneTone::Neutral,
                selected: false,
                chips: vec![PaneChip {
                    tone: PaneTone::Neutral,
                    text: "unchanged".to_string(),
                    title: "No unsaved changes".to_string(),
                }],
            },
            actions: story_actions(),
            on_action: move |_| {},
            detail: story_detail(),
        }
    }
}

#[story(
    description = "Header icons by consequence level (D7, Q7), the same treatment ActionButton gives: Save is Routine and plain; Remove is Undoable and wears the error tint, one click; Revert to saved is Lasting and wears the error tint, and its first click ARMS it. Top: at rest. Bottom: Revert to saved armed — the icon keeps its width and changes its dress (the error fill, the quiet drain along its foot), and its tooltip and accessible name become what is lost. Blur or the 4 s window stands it down; there is no dialog."
)]
pub(crate) fn header_actions_by_level() -> Element {
    rsx! {
        div { class: "tw:grid tw:gap-4",
            StudioPane {
                title: "fyeah_sign.show",
                kind: "Project".to_string(),
                chrome: story_dirty_chrome(),
                actions: story_level_actions(),
                on_action: move |_| {},
            }
            StudioPane {
                title: "fyeah_sign.show",
                kind: "Project".to_string(),
                chrome: story_dirty_chrome(),
                actions: story_level_actions(),
                armed_preview: true,
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Collapsed pane: the collapse rail folds the pane to its header; selection outline, live header tint, live chip."
)]
pub(crate) fn collapsed() -> Element {
    rsx! {
        StudioPane {
            collapse: story_collapse(true),
            primary: story_primary(),
            title: "Playlist",
            chrome: PaneChrome {
                tone: PaneTone::Live,
                selected: true,
                chips: vec![PaneChip {
                    tone: PaneTone::Live,
                    text: "1 live".to_string(),
                    title: "Touched live controls".to_string(),
                }],
            },
            body: rsx! {
                p { class: "tw:m-0 tw:pt-3 tw:text-sm tw:text-muted-foreground",
                    "Hidden while collapsed."
                }
            },
        }
    }
}

fn story_collapse(collapsed: bool) -> PaneCollapse {
    PaneCollapse {
        collapsed,
        expand_label: "Expand pane".to_string(),
        collapse_label: "Collapse pane".to_string(),
        on_toggle: EventHandler::new(|()| {}),
    }
}

fn story_primary() -> Element {
    rsx! {
        span { class: "tw:inline-flex tw:h-8 tw:w-8 tw:shrink-0 tw:items-center tw:justify-center tw:rounded-full tw:border tw:border-status-neutral-border tw:bg-status-neutral-bg tw:text-status-neutral-foreground",
            StudioIcon {
                name: StudioIconName::NodeTreeItem,
                size: 16,
            }
        }
    }
}

fn story_trailing() -> Element {
    rsx! {
        button {
            class: "tw:min-h-full tw:border-0 tw:border-l tw:border-border-muted tw:bg-transparent tw:px-4 tw:text-xs tw:font-bold tw:text-muted-foreground",
            r#type: "button",
            "raw"
        }
    }
}

fn story_detail() -> Element {
    rsx! {
        DetailPopover {
            icon: StudioIconName::Info,
            label: "Pane details",
            tone: IconMenuTone::Neutral,
            div { class: "tw:grid tw:gap-1 tw:p-3",
                p { class: "tw:m-0 tw:text-xs tw:text-muted-foreground", "Detail popover content." }
            }
        }
    }
}

fn story_actions() -> Vec<UiPaneAction> {
    vec![
        UiPaneAction::new(
            "play",
            UiAction::from_op(ControllerId::new("story.pane"), ProjectOp::SaveOverlay),
        ),
        UiPaneAction::new(
            "test-tube",
            UiAction::from_op(ControllerId::new("story.pane"), ProjectOp::RevertAllEdits)
                .with_label("Revert to saved")
                .disabled("nothing to revert"),
        ),
    ]
}

fn story_dirty_chrome() -> PaneChrome {
    PaneChrome {
        tone: PaneTone::Warning,
        selected: false,
        chips: vec![PaneChip {
            tone: PaneTone::Warning,
            text: "3 unsaved".to_string(),
            title: "Pending persisted edits".to_string(),
        }],
    }
}

/// One header action per consequence level: Routine, Undoable, Lasting.
fn story_level_actions() -> Vec<UiPaneAction> {
    vec![
        UiPaneAction::new(
            "save",
            UiAction::from_op(ControllerId::new("story.pane"), ProjectOp::SaveOverlay),
        ),
        UiPaneAction::new(
            "remove",
            UiAction::from_op(
                ControllerId::new("story.pane"),
                NodeRemoveOp {
                    node: ProjectNodeAddress::parse("/fyeah_sign.show/playlist.playlist")
                        .expect("valid story node address"),
                },
            )
            .with_summary(
                "Remove Playlist from the project. You can revert from the save panel until you save.",
            ),
        ),
        UiPaneAction::new(
            "revert",
            UiAction::from_op(ControllerId::new("story.pane"), ProjectOp::RevertAllEdits)
                .with_label("Revert to saved"),
        ),
    ]
}
