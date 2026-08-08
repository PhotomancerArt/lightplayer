//! Stories for the fixture card face.
//!
//! The face is the thing being lit (LED sample-point preview) plus one
//! dominant horizontal brightness fader. Coverage: default and the
//! advanced drawer open.

use dioxus::prelude::*;
use lpa_studio_core::UiSpaceCellRole;
use lpa_studio_web_story_macros::story;

use crate::app::node::face_story_fixtures::{
    fixture_face_bound_output, fixture_face_limiting, fixture_face_policy,
    fixture_face_within_budget, fixture_node_view, fixture_node_view_with_face,
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

// -- the space section, consumer side (plan-B P4 / gate G1) ------------------

#[story(
    description = "The MIRROR (D13): the fixture's `space` section sits in the same slot the shader card gives its declaration — between output and settings, same rail, same row shape, rendered by the same component off the same DTO. What differs is the voice: a shader declares a space, a fixture states a policy, so this side reads `consumes: auto` and carries the one authored bit a shape cannot answer for itself — does strip order mean something (D3). `auto` expands into nothing else on purpose: a fixture with no opinion follows whatever each source declares, and the line under the primary says exactly that."
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
    description = "An authored policy: `consumes: policy` opens the `from 1D sources` cell — the same picker the shader's `default projection` cell opens, mirrored — with the `force` bit inline on the row it qualifies (spike §3). Unforced, this only fills a silence: a source that declares its own projection still wins, which is what the line beneath says."
)]
fn space_policy() -> Element {
    rsx! {
        FixtureCardCanvas {
            FixtureFace {
                face: fixture_face_policy(false),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "The same policy FORCED: this fixture's default now wins over a source's own declaration, and the who-wins line flips to say so. The one line is the compact form of the spike's precedence ladder — the rung that can still surprise the person reading this card, stated where the gesture that changes it lives."
)]
fn space_policy_forced() -> Element {
    rsx! {
        FixtureCardCanvas {
            FixtureFace {
                face: fixture_face_policy(true),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "The picker open on the CONSUMER side — one component, both sides of the binding (D16). The tiles, the glyphs, the merged outline and the select-and-close are identical to the shader card's; only the cell it writes differs (`consume.Policy.from_1d`). Note the absent `consumer decides` tile: a fixture that has opened a policy has to name one, so the deferring choice does not exist on this side."
)]
fn space_projection_picker_open() -> Element {
    rsx! {
        FixtureCardCanvas {
            FixtureFace {
                face: fixture_face_policy(false),
                space_picker_open_cell: UiSpaceCellRole::ConsumerFrom1d,
                on_action: move |_| {},
            }
        }
    }
}
