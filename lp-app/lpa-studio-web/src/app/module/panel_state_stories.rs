//! Stories for the panel's three control states and its reset gestures
//! (`docs/design/panel.md` P2/P-Q2).
//!
//! `docs/design/panel.md` P-Q2 asks for confirmation that
//! Read-following-automation, Read-at-default, and Engaged are three
//! *visibly distinct* states. These stories put them next to each other so
//! that is a judgement about pixels rather than about prose.
//!
//! The shipped treatment: **gold** (`status-engaged`, minted at the M4 P6
//! gate) for engaged. Not violet — bound means *wired*, engaged means
//! *captured* (P6). Not green — green is valid-only. Not the blue live
//! family — that is a transient unsaved edit. Not attention-orange — that
//! stays device/roster health.

use dioxus::prelude::*;
use lpa_studio_core::PlayState;
use lpa_studio_web_story_macros::story;

use super::module_fixtures::{
    PLASMA_1_SCOPE, PanelWalk, ROOT_SCOPE, TRANSPORT_CHANNELS, held_root_face, palette_panel,
    root_module_node_view, three_state_panel, transport_panel,
};
use super::{ModulePanel, PanelGesture};
use crate::app::node::face_story_fixtures::clock_transport;

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
    description = "The three panel states across all three widget families, with NO sublabels — a control is a widget, a label, and a value. Read-at-default = quiet accent, subtle label. Read-following = violet at the LIVE value. Engaged = gold arc/fill/ring plus the per-control reset glyph. Everything the old captions said now lives behind the label (see control-detail)."
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
    description = "Reset granularity (P2 clear): per control — the gold revert glyph beside the label, present ONLY while engaged — per nested group, in its box border beside the name, and per module, upper right, counting everything under the scope. An untouched panel shows no destructive control at all, so the glyph's presence is itself part of the state signal."
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
    description = "Walkable Read → Latch → Clear (P2). Drag any knob: the first touch materializes its panel writer and the control turns engaged gold and captures the channel; the reset glyph or the panel-level reset drops the writer and the control falls back to following the project. Latch, not Touch — letting go changes nothing."
)]
fn latch_walkthrough() -> Element {
    // Start from the pristine Read face, so the FIRST touch is the thing
    // being felt — and so a clear has somewhere honest to land.
    let mut walk = use_signal(|| PanelWalk::new(root_module_node_view()));

    rsx! {
        PanelCanvas {
            ModulePanel {
                panel: walk().face().panel.clone(),
                auto_save: walk().face().auto_save,
                on_panel: move |gesture: PanelGesture| {
                    walk.with_mut(|walk| walk.apply_gesture(&gesture));
                },
                on_action: move |action| {
                    walk.with_mut(|walk| walk.apply_action(&action));
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
    description = "Palette swatches on a module panel, in the three panel states (M4 P3). The closed face is mode-adaptive: a HELD palette is one full-width strip reading `5 stops`; a CYCLE is its member set as equal segments of one band, reading `↻ 4 · 20 s` — the P6 gate's Step voice, plain seconds. The chevron says a chooser lives behind the band (it arrives in P4; the band is deliberately inert here). Colors are the existing families — violet following, engaged gold held with its reset glyph — and no live member ring, because a panel control has no phase reading in hand."
)]
fn palette_swatches() -> Element {
    rsx! {
        PanelCanvas {
            ModulePanel {
                panel: palette_panel(),
                on_panel: move |_| {},
                on_action: move |_| {},
            }
        }
    }
}

// -- the grouped Transport control (P8) -----------------------------------

#[story(
    description = "The clock's Transport as ONE grouped control on the module panel — the panel a phone opens onto, and the expected primary speed control. The whole tape instrument is the faceplate (streaming strip, digits, run/pause, log ׼–×8 fader with octave detents), taking the panel row's full width so the narrow controls wrap below it rather than squeezing it. It is one control with THREE wires: the fader writes clock.rate, run/pause writes clock.play_state, a strip drag writes clock.scrub. Judge it at sm first — that is the phone."
)]
fn transport() -> Element {
    rsx! {
        PanelCanvas {
            ModulePanel {
                panel: transport_panel(
                    clock_transport(447.0, PlayState::Playing, 1.0, 0.0),
                    TRANSPORT_CHANNELS,
                ),
                on_panel: move |_| {},
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Paused on the panel: the strip and digits hold still, the run button shows ▶ unlit, and nothing else about the control moves — pausing is calm, not a mode change. The setpoint went out as a state NOUN on clock.play_state (\"paused\"), so a consumer that reads it late still lands where the user asked."
)]
fn transport_paused() -> Element {
    rsx! {
        PanelCanvas {
            ModulePanel {
                panel: transport_panel(
                    clock_transport(447.0, PlayState::Paused, 1.0, 0.0),
                    TRANSPORT_CHANNELS,
                ),
                on_panel: move |_| {},
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Scrubbed off-live at ×2: the tape box border goes amber and the offset reads on the amber line under the digits, tap-to-return. The three dimensions moved independently — the fader is seated on the ×2 detent while the scrub sits −12.4 s back — because they are three channels, not one packed record."
)]
fn transport_off_live() -> Element {
    rsx! {
        PanelCanvas {
            ModulePanel {
                panel: transport_panel(
                    clock_transport(447.0, PlayState::Playing, 2.0, -12.4),
                    TRANSPORT_CHANNELS,
                ),
                on_panel: move |_| {},
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "PARTIAL wiring: the rate leaf has an authored binding onto a custom `speed` channel, which suppresses only its own default — its siblings keep clock.play_state and clock.scrub. Two things to check: the faceplate still renders WHOLE (rendering is a shape fact; wiring never subtracts a dimension from it), and the group re-anchors onto `speed`, since the group's identity is the rate leaf's EFFECTIVE channel."
)]
fn transport_partial_wiring() -> Element {
    let mut channels = TRANSPORT_CHANNELS;
    channels[0].1 = "speed";
    rsx! {
        PanelCanvas {
            ModulePanel {
                panel: transport_panel(
                    clock_transport(447.0, PlayState::Playing, 1.5, 0.0),
                    channels,
                ),
                on_panel: move |_| {},
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Engaged gold vs bound violet, one channel, one range, side by side — the direct comparison behind P-Q2. The left knob is wired and following its writer; the right one has been captured and holds. Nothing on this panel is green."
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
