//! Stories for the shader card face (permanent face + drawers).
//!
//! Full `NodePane` compositions exercising the `face: Some` branch —
//! preview hero, knob row, condensed agent chat, and the code/advanced
//! drawers. Coverage: idle, bound-knob, chat-streaming, code drawer open,
//! advanced drawer open.

use dioxus::prelude::*;
use lpa_studio_core::{
    UiAgentStatus, UiCellProjection, UiProjectionOrigin, UiSpaceCellRole, UiVisualSpace,
};
use lpa_studio_web_story_macros::story;

use crate::app::node::face_story_fixtures::{
    period_knob, shader_face, shader_face_bound_output, shader_face_one_d,
    shader_face_stacked_preview, shader_node_view, shader_node_view_with_face,
    shader_space_section_mismatch,
};
use crate::app::node::{NodePane, PanelControl, ShaderFace};
use crate::base::Platform;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ShaderCardCanvas(children: Element) -> Element {
    rsx! {
        div { class: "tw:w-full tw:max-w-md", {children} }
    }
}

#[story(
    description = "Idle shader card AT REST: preview hero, knob row (blue live-edited scale label, mirror toggle), and three collapsed lids — agent, code, advanced. The agent section is collapsed by default now (G1 R-F): a shader card stacks a lot, and the chat is a thing you go to on purpose, so it announces itself with a labeled summary row rather than by occupying the card."
)]
fn idle() -> Element {
    rsx! {
        ShaderCardCanvas {
            NodePane {
                view: shader_node_view(false, UiAgentStatus::Idle),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Bound knob on the face: speed rides a bus binding — violet arc, ring, and name on the knob itself."
)]
fn bound_knob() -> Element {
    rsx! {
        ShaderCardCanvas {
            NodePane {
                view: shader_node_view(true, UiAgentStatus::Idle),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "The visual output's own header above the hero: name, the violet publish chip when the render is wired to a bus channel, and the 'i' detail affordance. The full-bleed hero replaced the boxed product pane, and this chrome came back with it."
)]
fn output_header_bound() -> Element {
    rsx! {
        ShaderCardCanvas {
            NodePane {
                view: shader_node_view_with_face(shader_face_bound_output(UiAgentStatus::Idle)),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "The output header's detail popover open: type info plus the Output aspect's routing rows (published channel, who reads it, revision) — the same popover every slot surface opens."
)]
fn output_detail_open() -> Element {
    rsx! {
        ShaderCardCanvas {
            ShaderFace {
                face: shader_face_bound_output(UiAgentStatus::Idle),
                node: "/fyeah_sign.show/aurora.shader".to_string(),
                output_detail_initially_open: true,
                on_action: move |_| {},
            }
        }
    }
}

/// The two states a speed knob has (P7 item 5, re-voiced at G2): the
/// readout is the auto-denominated rate ("3/min" — bigger IS faster, the
/// drag axis inverts to match), and the gesture still writes a whole
/// `PhasorConfig`, never a bare float.
#[story(
    description = "Speed knobs. Left: slot-local — the knob edits consumed[phase].phasor.some and belongs to this card alone. Right: channel-driven — an authored config channel makes it violet, puts it on the module panel, and every reader of that channel rides the one integrator it retunes."
)]
fn phasor_period() -> Element {
    rsx! {
        div { class: "tw:flex tw:items-start tw:gap-8 tw:p-4",
            PanelControl {
                control: period_knob("Speed", 20.0, false),
                on_action: move |_| {},
            }
            PanelControl {
                control: period_knob("Speed", 100.0, true),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Mid-run agent chat on the face, EXPANDED: streaming cursor and Stop button in the condensed chat while the preview and knobs stay put. Expanding is a per-card gesture that persists (`NodeCardUiState.agent_collapsed`), so this is what the section looks like once you have opened it."
)]
fn chat_streaming() -> Element {
    let mut view = shader_node_view(true, UiAgentStatus::Streaming);
    view.card_ui.agent_collapsed = false;
    rsx! {
        ShaderCardCanvas {
            NodePane {
                view,
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "The agent section EXPANDED at idle — the state one click from the default. Read against `idle`: collapsing costs nothing but the composer, and the collapsed row still names the section and carries its status summary, which is what makes the new default safe."
)]
fn agent_expanded() -> Element {
    let mut view = shader_node_view(true, UiAgentStatus::Idle);
    view.card_ui.agent_collapsed = false;
    rsx! {
        ShaderCardCanvas {
            NodePane {
                view,
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Code drawer open: the inline GLSL editor expands under the face — opening code never hides the preview or knobs."
)]
fn code_drawer_open() -> Element {
    // Disclosure is core-owned now: stories seed the DTO's card UI state
    // (`NodeCardUiState`), not component props.
    let mut view = shader_node_view(true, UiAgentStatus::Idle);
    view.card_ui.code_open = true;
    rsx! {
        ShaderCardCanvas {
            NodePane {
                view,
                on_action: move |_| {},
                face_platform: Platform::Mac,
            }
        }
    }
}

#[story(
    description = "Advanced drawer open: today's slot rows (bound speed row included) behind the last lid."
)]
fn advanced_open() -> Element {
    let mut view = shader_node_view(true, UiAgentStatus::Idle);
    view.card_ui.advanced_open = true;
    rsx! {
        ShaderCardCanvas {
            NodePane {
                view,
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Agent section collapsed to its summary row — the DEFAULT resting state (G1 R-F): sparkles icon + status-aware summary (turn count and cost estimate); expanding restores the chat with any half-typed draft intact."
)]
fn agent_collapsed() -> Element {
    let mut view = shader_node_view(true, UiAgentStatus::Idle);
    view.card_ui.agent_collapsed = true;
    rsx! {
        ShaderCardCanvas {
            NodePane {
                view,
                on_action: move |_| {},
            }
        }
    }
}

// -- the space section (dimensionality plan-B P4 / gate G1) ------------------

/// The whole card, so the section can be judged at the rank it claims:
/// between the product and the settings, with a rail label of its own.
#[story(
    description = "A 2D shader's `space` section at rest — the declaration lifted out of the advanced drawer and onto the card at the same rank as output and settings (D13). The primary reads as a segmented pair (a two-state choice is squared blocks, never a dropdown), the header carries the matching `2D` badge, and the one answer cell below it is a STATEMENT rather than a picker: a 2D producer filling a 1D request has exactly one answer today (the centre scanline), and a dropdown over one option invites a gesture with nothing to change."
)]
fn space_two_d() -> Element {
    rsx! {
        ShaderCardCanvas {
            NodePane {
                view: shader_node_view(false, UiAgentStatus::Idle),
                on_action: move |_| {},
            }
        }
    }
}

/// One story per answer: the four projections are the design decision, and
/// a single screenshot of "radial" cannot be read against the others.
#[story(
    description = "A 1D shader declaring how it fills 2D space, one card per answer. `consumer decides` defers (the fixture's own default applies); the other four state an opinion that a fixture has to FORCE to overrule — which is what the line under the cells says. Read left to right: the same card, the same row, four different answers, each with its projection's own glyph on the closed field."
)]
fn space_one_d_answers() -> Element {
    rsx! {
        div { class: "tw:grid tw:gap-4 tw:p-2 tw:lg:grid-cols-2",
            for answer in ["Default", "Extrude", "Radial", "Mirror"] {
                div { key: "{answer}", class: "tw:w-full tw:max-w-md",
                    ShaderFace {
                        face: shader_face_one_d(answer),
                        node: "/fyeah_sign.show/comet.shader".to_string(),
                        on_action: move |_| {},
                    }
                }
            }
        }
    }
}

#[story(
    description = "The projection picker OPEN (D16): opening the enum field opens a tile grid inside it — one merged outline welds field and panel, so it reads as diving into the control rather than a menu appearing near it. Each tile draws what that answer DOES to a strip (extrude stretches it down, radial pushes it out from the centre, angular sweeps it around, mirror folds it at the centre); the tiles are schematic rather than live probes, which is exactly the question G1 rules on. A pick dispatches `EnsurePresent space.OneD.in_2d.<Variant>` and closes — a selection is a completed gesture."
)]
fn space_projection_picker_open() -> Element {
    rsx! {
        ShaderCardCanvas {
            ShaderFace {
                face: shader_face_one_d("Radial"),
                node: "/fyeah_sign.show/comet.shader".to_string(),
                space_picker_open_cell: UiSpaceCellRole::ProducerIn2d,
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "D1 on the card instead of in a compile log: the project declares 1D and the GLSL defines `render_2d`. The compiler refuses that outright — the declaration IS the entry contract — so the card names BOTH sides and points at the two places either could be changed, and the segmented primary wears the error family because it is the thing being objected to."
)]
fn space_mismatch() -> Element {
    let mut face = shader_face(false, UiAgentStatus::Idle);
    face.space = Some(shader_space_section_mismatch());
    rsx! {
        ShaderCardCanvas {
            ShaderFace {
                face,
                node: "/fyeah_sign.show/comet.shader".to_string(),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "D15's preview checkboxes, both on: the 1D band is what the shader actually renders, the square below it is the same product projected into 2D, and the caption under each names the space, the projection, and WHERE that projection came from. `(declared)` is the shader's own opinion; the sibling story shows the two other origins. One box always stays on — the last one refuses its own click rather than emptying the hero."
)]
fn preview_spaces_stacked() -> Element {
    rsx! {
        ShaderCardCanvas {
            ShaderFace {
                face: shader_face_stacked_preview(
                    UiCellProjection::Radial,
                    UiProjectionOrigin::Declared,
                ),
                node: "/fyeah_sign.show/comet.shader".to_string(),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "The same stacked hero under the other two origins (D11's honesty rule): `(consumer default)` is the fixture filling a silence the shader left, `(forced)` is the fixture overruling an opinion the shader did state. A projection nobody authored must never read like one somebody did — which is the whole reason the caption carries an origin at all."
)]
fn preview_space_origins() -> Element {
    rsx! {
        div { class: "tw:grid tw:gap-4 tw:p-2 tw:lg:grid-cols-2",
            for (key , origin) in [
                ("consumer-default", UiProjectionOrigin::ConsumerDefault),
                ("forced", UiProjectionOrigin::Forced),
            ]
            {
                div { key: "{key}", class: "tw:w-full tw:max-w-md",
                    ShaderFace {
                        face: shader_face_stacked_preview(UiCellProjection::Extrude, origin),
                        node: "/fyeah_sign.show/comet.shader".to_string(),
                        on_action: move |_| {},
                    }
                }
            }
        }
    }
}

#[story(
    description = "1D only — the D15 default for a strip-native shader, and the card an author of fire2012 or comet sees before touching anything. The hero is the strip as a readable band rather than the 32:1 hairline its probe geometry literally is; the caption says `native · 1D`, because nothing was projected to get here."
)]
fn preview_space_one_d_only() -> Element {
    let mut face = shader_face_one_d("Extrude");
    face.preview
        .spaces
        .retain(|view| view.space == UiVisualSpace::OneD);
    rsx! {
        ShaderCardCanvas {
            ShaderFace {
                face,
                node: "/fyeah_sign.show/comet.shader".to_string(),
                on_action: move |_| {},
            }
        }
    }
}
