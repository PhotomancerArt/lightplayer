//! Stories for the fixture card face.
//!
//! The face is the thing being lit (LED sample-point preview) plus one
//! dominant horizontal brightness fader. Coverage: default and the
//! advanced drawer open.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::app::node::face_story_fixtures::{
    fixture_face, fixture_face_bound_output, fixture_face_limiting, fixture_face_override,
    fixture_face_with_space, fixture_face_within_budget, fixture_node_view,
    fixture_node_view_with_face, fixture_space_section, fixture_space_section_wire_reversed,
    fyeah_presentable_doc, map2d_fixture_face, map2d_fixture_face_editing,
};
use crate::app::node::map_view::MapViewOptions;
use crate::app::node::{FixtureFace, NodePane};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn FixtureCardCanvas(children: Element) -> Element {
    rsx! {
        div { class: "tw:w-full tw:max-w-md", {children} }
    }
}

#[story(
    description = "Fixture card: ring lamp preview (what the LEDs receive) with the dominant brightness fader below; advanced drawer collapsed."
)]
fn default() -> Element {
    rsx! {
        FixtureCardCanvas {
            NodePane {
                view: fixture_node_view(),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Limiting opted out (budget 0): the only way to get no readout, since an unstated budget now falls back to the 1000 mA default guard. For someone whose supply is genuinely larger than any default."
)]
fn power_opted_out() -> Element {
    rsx! {
        FixtureCardCanvas {
            NodePane {
                view: fixture_node_view(),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Inside budget: one quiet line reading estimated draw against the declared supply — a setup number, useful before anything goes wrong. 'Estimated' is literal; no preset here has met a meter."
)]
fn power_within_budget() -> Element {
    rsx! {
        FixtureCardCanvas {
            NodePane {
                view: fixture_node_view_with_face(fixture_face_within_budget()),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Actively limiting: demand is over budget so output is scaled to stay inside it. Coloured 'attention', not 'warning' — shedding current to honour a declared budget is the feature working, not a fault."
)]
fn power_limiting() -> Element {
    rsx! {
        FixtureCardCanvas {
            NodePane {
                view: fixture_node_view_with_face(fixture_face_limiting()),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "The output's own header: name, the violet publish chip when the control output is wired to a bus channel, and the 'i' detail affordance. The custom lamp hero replaced the boxed product pane, and this chrome came back with it — before, a fixture's output was the one produced product you could not inspect or see the link status of."
)]
fn output_header_bound() -> Element {
    rsx! {
        FixtureCardCanvas {
            NodePane {
                view: fixture_node_view_with_face(fixture_face_bound_output()),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "The output header's detail popover open: type info plus the Output aspect's routing rows (published channel, who reads it, revision) — the same popover every slot surface opens, reached from the hero's header."
)]
fn output_detail_open() -> Element {
    rsx! {
        FixtureCardCanvas {
            FixtureFace {
                face: fixture_face_bound_output(),
                output_detail_initially_open: true,
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Advanced drawer open: mapping/input/driver/channel slot rows (bound input row included) under the face."
)]
fn advanced_open() -> Element {
    // Disclosure is core-owned: seed the DTO's card UI state.
    let mut view = fixture_node_view();
    view.card_ui.advanced_open = true;
    rsx! {
        FixtureCardCanvas {
            NodePane {
                view,
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "A 16×16 snake panel lit from the live frame: 256 lamps on one canvas, no chrome. What view mode is for — looking at the thing, not at its wiring."
)]
fn panel_display_view() -> Element {
    rsx! {
        FixtureCardCanvas {
            FixtureFace {
                face: map2d_fixture_face(&lpc_mapping::corpus::panel_16x16()),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "The same panel with live colors off: neutral lamps, so the layout still reads with no feed behind it (an untracked output, a story, the gallery)."
)]
fn panel_unlit_view() -> Element {
    rsx! {
        FixtureCardCanvas {
            FixtureFace {
                face: map2d_fixture_face(&lpc_mapping::corpus::panel_16x16()),
                initial_map_view: MapViewOptions {
                    live: false,
                    ..MapViewOptions::default()
                },
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "The real sign import (SVG-derived paths + canvas framing) under live colors — irregular lamp spacing at the renderer's per-lamp radius."
)]
fn sign_display_view() -> Element {
    rsx! {
        FixtureCardCanvas {
            FixtureFace {
                face: map2d_fixture_face(&fyeah_presentable_doc()),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Multi-ring button (two concentric rings, one parametric object) under live colors: the small-radius end of the renderer, where lamps sit at the 5px floor."
)]
fn button_rings_view() -> Element {
    rsx! {
        FixtureCardCanvas {
            FixtureFace {
                face: map2d_fixture_face(&lpc_mapping::corpus::basic_button()),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "One home, edit mode: the output section flipped into the in-place mapping editor (asset-pipeline synced), pencil toggle active."
)]
fn mapping_edit_mode() -> Element {
    rsx! {
        FixtureCardCanvas {
            FixtureFace {
                face: map2d_fixture_face_editing(&fyeah_presentable_doc()),
                edit_initially_open: true,
                on_action: move |_| {},
            }
        }
    }
}

// -- the Shape declaration moment (plan-B P5 / gate G2) ----------------------

#[story(
    description = "The Shape declaration moment (D13): a FRESHLY CREATED fixture renders its dimensionality drawer in guided clothing — 'What shape is this fixture?' over four preset tiles (Strip / Matrix / Mapped shape / 3D-soon-disabled) and a skip link. The trigger is card-UI state set by the create/paste paths, so existing fixtures never see it. Each tile is a batch of the SAME slot ops the compact section and advanced drawer send (strip-order bit, mapping, render size) — no parallel write path; the Mapped tile opens the in-place mapping editor (the map IS the shape). No strip-order follow-up question: that bit is the dropdown's first choice, one state away."
)]
fn shape_moment_guided() -> Element {
    let doc = fyeah_presentable_doc();
    rsx! {
        FixtureCardCanvas {
            FixtureFace {
                face: map2d_fixture_face_editing(&doc),
                shape_guided: true,
                node: Some("/fyeah_sign.show/halo.fixture".to_string()),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "The guided moment on a fixture with NO map2d document (an undeclared paste of older clipboard content): the Mapped-shape tile is honestly disabled — its tooltip points at the advanced drawer — while Strip and Matrix stay live. The moment never invents a mapping; it only writes the slots that exist."
)]
fn shape_moment_no_map() -> Element {
    rsx! {
        FixtureCardCanvas {
            FixtureFace {
                face: fixture_face(),
                shape_guided: true,
                node: Some("/fyeah_sign.show/halo.fixture".to_string()),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "What the STRIP preset leaves behind: the guided clothing gone, the dimensionality drawer back in compact form reading `along the wire` — the preset wrote strip_order=true, mapping=Unset, and a 1-row render area (the declared-strip idiom the engine's 2D-membership check reads), all through the ordinary slot ops. The same section, one declaration later."
)]
fn shape_moment_strip_result() -> Element {
    rsx! {
        FixtureCardCanvas {
            FixtureFace {
                face: fixture_face_with_space(fixture_space_section()),
                space_initially_open: true,
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "What the MATRIX preset leaves behind: strip_order=false (wire order is plumbing), mapping=Unset, a 16×16 render area — 2D membership from authored intent. The dropdown reads `follow the source`: 1D sources project the way they declare, exactly what a panel wants."
)]
fn shape_moment_matrix_result() -> Element {
    rsx! {
        FixtureCardCanvas {
            FixtureFace {
                face: fixture_face_override("Auto"),
                space_initially_open: true,
                on_action: move |_| {},
            }
        }
    }
}

// -- the space section, consumer side (plan-B P4 / gate G1) ------------------

#[story(
    description = "The card's DEFAULT posture: the dimensionality drawer rides below settings, collapsed to one summary row — `along the wire`, because a fresh fixture's strip order means something (D3's scarf default) and that bit now IS the dropdown's first choice (strip-order unification). Expanding the drawer reveals ONE dropdown."
)]
fn space_auto() -> Element {
    rsx! {
        FixtureCardCanvas {
            NodePane {
                view: fixture_node_view(),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "The drawer OPEN in its default state: one dropdown — `show 1D sources by: along the wire` — with the wire's forward/reversed direction row under it (the wire-reversed addendum). The old strip-order checkbox is GONE: it silently gated the dropdown (a set bit means the projection never fires), so its semantics became the first choice of the same control. This is the scarf case made visible."
)]
fn space_along_wire() -> Element {
    rsx! {
        FixtureCardCanvas {
            FixtureFace {
                face: fixture_face_with_space(fixture_space_section()),
                space_initially_open: true,
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Along the wire, REVERSED: the direction row's second segment flips `wire_reversed` — lamp k reads strip position N-1-k — and the collapsed summary wears the back arrow (`along the wire ←`). An ordinary bool SetValue behind a direction segment; the interim home until per-range reversed lands with the patching work."
)]
fn space_along_wire_reversed() -> Element {
    rsx! {
        FixtureCardCanvas {
            FixtureFace {
                face: fixture_face_with_space(fixture_space_section_wire_reversed()),
                space_initially_open: true,
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "The dropdown on `follow the source`: strip order cleared, 1D sources project the way they declare. No `force` checkbox anywhere: with one control, following is an entry and an explicit projection pick IS the override."
)]
fn space_policy() -> Element {
    rsx! {
        FixtureCardCanvas {
            FixtureFace {
                face: fixture_face_override("Auto"),
                space_initially_open: true,
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "An authored OVERRIDE: the same dropdown now names `radial`, the hint flips to `This fixture overrides what 1D sources declare.`, and the collapsed summary would read `1D sources: radial (override)`. Under the hood the pick dispatched ensure-Policy → ensure-from_1d.Radial → force=true — the same ops the drawer rows send, batched into one gesture."
)]
fn space_policy_forced() -> Element {
    rsx! {
        FixtureCardCanvas {
            FixtureFace {
                face: fixture_face_override("Radial"),
                space_initially_open: true,
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "The choice tiles INLINE on the CONSUMER side — one component, both sides of the binding (D16), no popover anywhere in the section (the inline-tiles ruling). Six always-visible tiles: `along the wire` (serpentine: wire order, the map doesn't apply), `follow the source` (dashed: the answer lives on the source), and the four projections. Selected = accent border + wash + check badge."
)]
fn space_choices_inline() -> Element {
    rsx! {
        FixtureCardCanvas {
            FixtureFace {
                face: fixture_face_override("Auto"),
                space_initially_open: true,
                on_action: move |_| {},
            }
        }
    }
}
