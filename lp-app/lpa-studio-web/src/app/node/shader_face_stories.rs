//! Stories for the shader card face (permanent face + drawers).
//!
//! Full `NodePane` compositions exercising the `face: Some` branch —
//! preview hero, knob row, condensed agent chat, and the code/advanced
//! drawers. Coverage: idle, bound-knob, chat-streaming, code drawer open,
//! advanced drawer open.

use dioxus::prelude::*;
use lpa_studio_core::{
    NodeCardUiState, UiAgentStatus, UiCellProjection, UiNodeFace, UiProjectionOrigin,
    UiSpaceCellRole, UiVisualSpace,
};
use lpa_studio_web_story_macros::story;

use crate::app::node::face_story_fixtures::{
    period_knob, shader_face, shader_face_bound_output, shader_face_one_d,
    shader_face_stacked_preview, shader_node_view, shader_node_view_with_face, shader_sections,
    shader_space_section_mismatch,
};
use crate::app::node::{NodeFaceBody, NodePane, PanelControl, ShaderFace};
use crate::base::Platform;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ShaderCardCanvas(children: Element) -> Element {
    rsx! {
        div { class: "tw:w-full tw:max-w-md", {children} }
    }
}

/// A shader face WITH its drawer stack, card-framed — what the space
/// stories mount now that the dimensionality drawer lives in
/// [`NodeFaceBody`]'s drawer run (G1b ruling 1) rather than on
/// [`ShaderFace`] itself. The padding re-adds what the body's full-bleed
/// negative margins reclaim from the real pane.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ShaderBodyCard(
    face: lpa_studio_core::UiShaderFace,
    #[props(default = false)] space_open: bool,
    #[props(default = None)] space_picker_open_cell: Option<UiSpaceCellRole>,
) -> Element {
    let card_ui = NodeCardUiState {
        space_open,
        ..NodeCardUiState::default()
    };
    rsx! {
        div { class: "tw:overflow-hidden tw:rounded-sm tw:border tw:border-border-strong tw:bg-card tw:px-4 tw:pb-4",
            NodeFaceBody {
                face: UiNodeFace::Shader(face),
                node: "/fyeah_sign.show/comet.shader".to_string(),
                card_ui,
                sections: shader_sections(),
                space_picker_open_cell,
                on_action: move |_| {},
            }
        }
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

/// The whole card, so the drawer can be judged where it now lives: in the
/// drawer stack between `code` and `advanced`, collapsed to its summary
/// row.
#[story(
    description = "The card's DEFAULT posture after G1b ruling 1: the `dimensionality` drawer rides the drawer stack BETWEEN `code` and `advanced` — the declaration is authoring that belongs next to the code — collapsed to a summary row (`2D · in 1D: centre scanline`). Its open state is core-owned (`NodeCardUiState.space_open`), like its sibling drawers."
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

/// One story per answer: the projections are the design decision, and a
/// single screenshot of "radial" cannot be read against the others.
#[story(
    description = "A 1D shader's drawer OPEN in its new home between `code` and `advanced` (G1b ruling 1), one card per answer. The declaration leads as a full-width tab pair (G1: 'almost like tabs'), and the answer row reads `show in 2D by`. `consumer decides` is gone (G1): the silent state is `extrude · default` — the projection silence actually resolves to — and the other picks state an opinion a fixture can still override, which is what the line beneath says."
)]
fn space_one_d_answers() -> Element {
    rsx! {
        div { class: "tw:grid tw:gap-4 tw:p-2 tw:lg:grid-cols-2",
            for answer in ["Default", "Extrude", "Radial", "Mirror"] {
                div { key: "{answer}", class: "tw:w-full tw:max-w-md",
                    ShaderBodyCard { face: shader_face_one_d(answer), space_open: true }
                }
            }
        }
    }
}

#[story(
    description = "The projection picker OPEN (D16, tiles ratified at G1): one merged outline welds field and panel, each tile draws what that answer DOES to a strip. Two G1 fixes visible: the angular tile is now a conic sweep of pie sectors (the ray spokes read as an asterisk), and the leading tile is `extrude · default` rather than a deferring blank. A pick dispatches `EnsurePresent space.OneD.in_2d.<Variant>` and closes — a selection is a completed gesture."
)]
fn space_projection_picker_open() -> Element {
    rsx! {
        ShaderCardCanvas {
            ShaderBodyCard {
                face: shader_face_one_d("Radial"),
                space_picker_open_cell: UiSpaceCellRole::ProducerIn2d,
            }
        }
    }
}

/// G1b decision-matrix candidate A: the space controls next to the code,
/// inside one renamed authoring section.
#[story(
    description = "G1b CANDIDATE A — 'they feel like authoring that should go next to the code': the space controls folded into one `definition` section together with the GLSL, instead of a drawer of their own (candidate B, every other space story). Yona is explicitly not 100% sold either way; this story exists so the two homes can be judged side by side. The code block here is a mock — judging the placement needs the shape of code, not a live editor."
)]
fn space_in_definition_variant() -> Element {
    let section = crate::app::node::face_story_fixtures::shader_space_section_one_d("Radial");
    rsx! {
        ShaderCardCanvas {
            div { class: "tw:overflow-hidden tw:rounded-sm tw:border tw:border-border-strong tw:bg-card",
                crate::app::node::NodeCardSection { label: "definition", first: true,
                    div { class: "tw:grid tw:min-w-0 tw:gap-0",
                        pre { class: "tw:m-0 tw:overflow-x-auto tw:px-4 tw:py-3 tw:font-mono tw:text-[11px] tw:leading-snug tw:text-soft-foreground",
                            "vec4 render_1d(float pos) {{\n    float w = phase(pos * reach);\n    return palette(w);\n}}"
                        }
                        div { class: "tw:border-t tw:border-border-strong",
                            crate::app::node::SpaceSection { section, on_action: move |_| {} }
                        }
                    }
                }
            }
        }
    }
}

/// G1b ruling 4's two-section design, in the flesh: shape, then
/// direction. (The old `space_projection_candidates` story — the Y-twin
/// drawings with no model behind them — retired with the ruling: the
/// directional vocabulary is real now.)
#[story(
    description = "The direction row — RULING 4's two-section design (2026-08-09): the top section is the general SHAPE (the picker keeps its four tiles plus the default), and when the active shape is directional a `direction` row appears under the projection field — a 4-way segmented control (→ ← ↓ ↑) in the same squared-blocks discrete language as the 2D|1D tabs. Here: mirror ↓ — the strip folds along the rows; the field face, the glyph, and the collapsed summary all wear the arrow. A pick dispatches `EnsurePresent <cell>.<Shape>.direction.<D>`, the generic enum-row gesture at the flattened payload row's real address. Radial/angular show no row."
)]
fn space_direction_row() -> Element {
    let mut face = shader_face_one_d("Mirror");
    face.space = Some(
        crate::app::node::face_story_fixtures::shader_space_section_one_d_directed(
            "Mirror", "Down",
        ),
    );
    rsx! {
        ShaderCardCanvas {
            ShaderBodyCard { face, space_open: true }
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
            // No `space_open`: the D1 mismatch itself forces the drawer
            // open — an error folded away is an error hidden.
            ShaderBodyCard { face }
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
                        face: shader_face_stacked_preview(
                            UiCellProjection::Extrude(lpa_studio_core::UiProjectionDirection::Right),
                            origin,
                        ),
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
