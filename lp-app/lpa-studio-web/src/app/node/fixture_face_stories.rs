//! Stories for the fixture card face.
//!
//! The face is the thing being lit (LED sample-point preview) plus one
//! dominant horizontal brightness fader. Coverage: default and the
//! advanced drawer open.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::app::node::face_story_fixtures::{
    fixture_face_limiting, fixture_face_within_budget, fixture_node_view,
    fixture_node_view_with_face, fyeah_presentable_doc, map2d_fixture_face,
    map2d_fixture_face_editing,
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
    description = "Wiring view on a 16×16 snake panel: numbers-in-lamps + direction arrows, live colors off. The physical-wiring helper view."
)]
fn panel_wiring_view() -> Element {
    rsx! {
        FixtureCardCanvas {
            FixtureFace {
                face: map2d_fixture_face(&lpc_mapping::corpus::panel_16x16()),
                initial_map_view: MapViewOptions {
                    numbers: true,
                    arrows: true,
                    universes: false,
                    live: false,
                },
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Universe colors on the 16×16 panel: 256 lamps flow across the 170-lamp boundary mid-panel (U1 blue → U2 gold)."
)]
fn panel_universes_view() -> Element {
    rsx! {
        FixtureCardCanvas {
            FixtureFace {
                face: map2d_fixture_face(&lpc_mapping::corpus::panel_16x16()),
                initial_map_view: MapViewOptions {
                    numbers: false,
                    arrows: false,
                    universes: true,
                    live: false,
                },
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "The real sign import (SVG-derived paths + canvas framing) with inter-path chain arrows over live colors."
)]
fn sign_chain_view() -> Element {
    rsx! {
        FixtureCardCanvas {
            FixtureFace {
                face: map2d_fixture_face(&fyeah_presentable_doc()),
                initial_map_view: MapViewOptions {
                    numbers: false,
                    arrows: true,
                    universes: false,
                    live: true,
                },
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Multi-ring button (two concentric rings, one parametric object) with wiring numbers over live colors."
)]
fn button_rings_numbered() -> Element {
    rsx! {
        FixtureCardCanvas {
            FixtureFace {
                face: map2d_fixture_face(&lpc_mapping::corpus::basic_button()),
                initial_map_view: MapViewOptions {
                    numbers: true,
                    arrows: false,
                    universes: false,
                    live: true,
                },
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
