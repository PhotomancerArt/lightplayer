//! Stories for the output card face (board diagram + one row per wire).
//!
//! Coverage is the four states the face has to be right in: the degenerate
//! single-wire node every format-2 output migrated into, a dome-shaped
//! five-wire node on the desk DOM-Z-102, the "no board known" fallback, and
//! the spread gesture's before/after — plus the pin picker open, since
//! picking a pin is the one gesture a still capture otherwise cannot show.

use dioxus::prelude::*;
use lpa_studio_core::{UiOutputChannelRow, UiOutputFace};
use lpa_studio_web_story_macros::story;

use crate::app::node::face_story_fixtures::{output_channel, output_face, output_node_view};
use crate::app::node::{NodePane, OutputFace};

/// The desk board: a WLED-class controller whose four fused data channels are
/// screw terminals on the rails.
const DESK_BOARD: &str = "domraem/dom-z-102";

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn OutputCardCanvas(children: Element) -> Element {
    rsx! {
        div { class: "tw:w-full tw:max-w-md", {children} }
    }
}

/// One wire on a QuinLED Dig-Uno, taking the whole 241-lamp buffer — the
/// shape every format-2 output migrated into.
fn single_channel_face() -> UiOutputFace {
    output_face(
        Some("quinled/dig-uno"),
        vec![output_channel(0, "LED1", None)],
        Some(241),
        Vec::new(),
    )
}

/// Five wires over a 1500-lamp dome on the desk board: four authored counts
/// and the highest-keyed wire taking the remainder. The upstream fixture
/// authored five paths, so the face has a real snapping grid.
fn dome_channels(counts: [Option<u32>; 5]) -> Vec<UiOutputChannelRow> {
    ["IO18", "IO16", "IO14", "IO2", "IO13"]
        .into_iter()
        .zip(counts)
        .enumerate()
        .map(|(key, (pin, count))| output_channel(key as u32, pin, count))
        .collect()
}

const DOME_SPANS: [u32; 5] = [0, 280, 610, 900, 1210];

fn dome_face(counts: [Option<u32>; 5], board: Option<&str>) -> UiOutputFace {
    output_face(
        board,
        dome_channels(counts),
        Some(1500),
        DOME_SPANS.to_vec(),
    )
}

#[story(
    description = "The degenerate case: one output node, one wire, no authored count — so the single channel takes the whole 241-lamp buffer ('rest'). The Dig-Uno's LED terminals live in the band above the board, and the one violet connection is real DTO data, not a sample. Everything the wire needs is on one line; the spread gesture is absent because one wire has nothing to spread across."
)]
fn default() -> Element {
    rsx! {
        OutputCardCanvas {
            NodePane {
                view: output_node_view(single_channel_face(), "1 wire · 241 lamps"),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "A dome on the desk DOM-Z-102: five wires from ONE output node, four with authored counts and the highest-keyed one taking the remainder. The diagram carries a violet connection per assigned channel (title 'ch<k>', lamp count beside it); each row reads its pin, its count, and — in the caption — the slice of the node's single control buffer it drives, so the split the engine performs is legible without opening a drawer."
)]
fn dome_five_channels() -> Element {
    rsx! {
        OutputCardCanvas {
            NodePane {
                view: output_node_view(
                    dome_face([Some(280), Some(330), Some(290), Some(310), None], Some(DESK_BOARD)),
                    "5 wires · 1500 lamps",
                ),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "No board known — a device provisioned outside Studio carries no board id, so there is nothing to draw a diagram of. The face does NOT degrade to the generic sections: every wire stays fully editable, and one quiet line says where the pin names are edited instead (free text in the advanced drawer). The pin cells drop their picker with the board; the channel chips drop violet, because nothing here is confirmed to land on a real pin."
)]
fn no_board() -> Element {
    rsx! {
        OutputCardCanvas {
            NodePane {
                view: output_node_view(
                    dome_face([Some(280), Some(330), Some(290), Some(310), None], None),
                    "5 wires · 1500 lamps",
                ),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Spread, BEFORE: the state someone lands in after wiring five strips by hand — 900 lamps on ch0 and 100 on the next three, with the last wire absorbing whatever is left. The gesture in the summary line reads 'fit counts to strips' because the upstream fixture published per-path spans; without them it reads 'divide lamps evenly', and its tooltip previews the exact resulting counts."
)]
fn spread_before() -> Element {
    rsx! {
        OutputCardCanvas {
            NodePane {
                view: output_node_view(
                    dome_face([Some(900), Some(100), Some(100), Some(100), None], Some(DESK_BOARD)),
                    "5 wires · 1500 lamps",
                ),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Spread, AFTER: the same node once the gesture ran. The even cut would be 300 per wire; the fixture's authored paths start at 0/280/610/900/1210, so each cut moved to the nearest path boundary and the wires end where the strips actually end. It is a SEQUENCE of ordinary count edits, not a new op — every wire undoes on its own, and the remainder wire was left count-less on purpose, so the node keeps one self-adjusting channel."
)]
fn spread_after() -> Element {
    rsx! {
        OutputCardCanvas {
            NodePane {
                view: output_node_view(
                    dome_face([Some(280), Some(330), Some(290), Some(310), None], Some(DESK_BOARD)),
                    "5 wires · 1500 lamps",
                ),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "The pin picker on channel 1: every output-eligible pin the board declares — rails and screw terminals alike — with the pin this wire is on marked 'current' and pins another channel already claims marked with that channel. Picking one writes the endpoint through the ordinary slot write (`ws281x:local:<label>`), keeping the wire's capability and target segments."
)]
fn pin_picker() -> Element {
    let face = dome_face(
        [Some(280), Some(330), Some(290), Some(310), None],
        Some(DESK_BOARD),
    );
    rsx! {
        OutputCardCanvas {
            div { class: "tw:rounded-md tw:border tw:border-border tw:bg-card",
                OutputFace { face, pin_picker_open: Some(1), on_action: move |_| {} }
            }
        }
    }
}

#[story(
    description = "An output with no wires yet: the face's empty state, whose one affordance is the add button at the BOTTOM of the list — where the new wire will appear, never in a header."
)]
fn no_wires() -> Element {
    let face = output_face(Some(DESK_BOARD), Vec::new(), Some(1500), Vec::new());
    rsx! {
        OutputCardCanvas {
            NodePane {
                view: output_node_view(face, "no wires"),
                on_action: move |_| {},
            }
        }
    }
}
