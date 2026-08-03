//! Stories for the panel's three control states and its reset gestures
//! (M2 UX spike, gate G2 question 5).
//!
//! `docs/design/panel.md` P-Q2 asks for confirmation that
//! Read-following-automation, Read-at-default, and Engaged are three
//! *visibly distinct* states. These stories put them next to each other so
//! that is a judgement about pixels rather than about prose.
//!
//! The spike's proposal: **amber** (`status-attention`) for engaged. Not
//! violet — bound means *wired*, engaged means *captured* (P6). Not green
//! — green is valid-only. Not the blue live family — that is a transient
//! unsaved edit. A dedicated `status-engaged` token family is the eventual
//! home; what is under test is whether amber reads as "held".

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use super::module_fixtures::{
    PLASMA_1_SCOPE, PanelSpike, ROOT_SCOPE, held_root_face, root_module_node_view,
    three_state_panel,
};
use super::{ModulePanel, PanelGesture};

/// Panel stories render on a card surface so the states are judged against
/// the background they will actually sit on.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PanelCanvas(children: Element) -> Element {
    rsx! {
        div { class: "tw:w-full tw:max-w-[640px] tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card",
            {children}
        }
    }
}

#[story(
    description = "The three panel states across all three widget families, with NO sublabels — a control is a widget, a label, and a value. Read-at-default = quiet accent, subtle label. Read-following = violet at the LIVE value. Engaged = amber arc/fill/ring plus the per-control reset glyph. Everything the old captions said now lives behind the label (see control-detail)."
)]
fn three_states() -> Element {
    rsx! {
        PanelCanvas {
            ModulePanel {
                panel: three_state_panel(),
                on_panel: move |_| {},
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Reset granularity (P2 clear): per control — the amber revert glyph beside the label, present ONLY while engaged — per nested group, in its box border beside the name, and per module, upper right, counting everything under the scope. An untouched panel shows no destructive control at all, so the glyph's presence is itself part of the state signal."
)]
fn reset_gestures() -> Element {
    rsx! {
        PanelCanvas {
            ModulePanel {
                panel: held_root_face().panel,
                auto_save: Some(true),
                on_panel: move |_| {},
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Auto-save off (P11): the small switch sits in the panel chrome upper right beside the reset glyph, on the module that owns the scope — panel state is per project folder (.lp/state.json), not an app setting. Off means held values are lost on restart, which is the opposite of the scarf requirement (P10)."
)]
fn auto_save_off() -> Element {
    let mut face = held_root_face();
    face.auto_save = Some(false);
    rsx! {
        PanelCanvas {
            ModulePanel {
                panel: face.panel,
                auto_save: face.auto_save,
                on_panel: move |_| {},
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Walkable Read → Latch → Clear (P2). Drag any knob: the first touch materializes its panel writer and the control turns amber and captures the channel; the reset glyph or the panel-level reset drops the writer and the control falls back to following the project. Latch, not Touch — letting go changes nothing."
)]
fn latch_walkthrough() -> Element {
    // Start from the pristine Read face, so the FIRST touch is the thing
    // being felt — and so a clear has somewhere honest to land.
    let mut spike = use_signal(|| PanelSpike::new(root_module_node_view()));

    rsx! {
        PanelCanvas {
            ModulePanel {
                panel: spike().face().panel.clone(),
                auto_save: spike().face().auto_save,
                on_panel: move |gesture: PanelGesture| {
                    spike.with_mut(|spike| spike.apply_gesture(&gesture));
                },
                on_action: move |action| {
                    spike.with_mut(|spike| spike.apply_action(&action));
                },
            }
        }
    }
}

#[story(
    description = "Where the sublabels went: a HELD control's detail popup, opened from its label. The control's own outline merges into the aspect card — the same contiguous-outline gesture the node face's panel controls use — and the card carries the state ('Held'), what the held value displaced ('Was: authored 200'), and the (scope, channel) identity. This is the whole cost of taking the captions off the face."
)]
fn control_detail() -> Element {
    let mut panel = held_root_face().panel;
    panel.groups.clear();
    rsx! {
        // Room around the control for the popup to open into.
        div { class: "tw:h-[420px] tw:w-full tw:max-w-[860px] tw:pl-48 tw:pr-8",
            PanelCanvas {
                ModulePanel {
                    panel,
                    detail_open_channel: Some("brightness".to_string()),
                    on_panel: move |_| {},
                    on_action: move |_| {},
                }
            }
        }
    }
}

#[story(
    description = "A nested group's heading popup: the scope path and the '2 controls · 1 held' tally that used to sit on the group header, now behind its name — same popup language as a control. The box's border carries only the instance name, which is what tells two copies of one effect apart."
)]
fn group_detail() -> Element {
    let mut panel = held_root_face().panel;
    panel.controls.clear();
    rsx! {
        div { class: "tw:h-[420px] tw:w-full tw:max-w-[860px] tw:pl-48 tw:pr-8",
            PanelCanvas {
                ModulePanel {
                    panel,
                    detail_open_group: Some(PLASMA_1_SCOPE.to_string()),
                    on_panel: move |_| {},
                    on_action: move |_| {},
                }
            }
        }
    }
}

#[story(
    description = "Engaged amber vs bound violet, one channel, one range, side by side — the direct comparison behind P-Q2. The left knob is wired and following its writer; the right one has been captured and holds. Nothing on this panel is green."
)]
fn engaged_vs_bound() -> Element {
    let mut panel = three_state_panel();
    panel.label = "hue".to_string();
    panel.scope = ROOT_SCOPE.to_string();
    // Keep only the two knobs under comparison.
    panel.controls.retain(|control| {
        matches!(control.control.label.as_str(), "following" | "engaged")
            && matches!(
                control.control.widget,
                lpa_studio_core::UiPanelWidget::Knob { .. }
            )
    });
    rsx! {
        PanelCanvas {
            ModulePanel {
                panel,
                on_panel: move |_| {},
                on_action: move |_| {},
            }
        }
    }
}
