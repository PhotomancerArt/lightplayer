//! Stories for the clock card face: the published time product plus the
//! per-reading phasor trace cards (clock-face v2).
//!
//! Coverage is the states the face has to be right in:
//!
//! - **live** — a few mixed cards: private and shared, different waveforms
//!   and rates, since the violet shared border is the face's one
//!   load-bearing distinction;
//! - **shared** — every card on one shared channel, the violet treatment
//!   alone;
//! - **crowd** — a full house (8+ cards, a frozen 0/s, a long reader name)
//!   proving the grid wraps and truncates instead of breaking;
//! - **empty** — the runtime answered and NOTHING is riding this timebase.
//!   A normal state (a project whose shaders declare no phasor has none),
//!   which must not read as a failure;
//! - **unread** / **unknown** — no probe yet vs. a clock that resolves no
//!   timebase; different sentences on purpose.
//!
//! Captures freeze animation; the canvas paints its first frame
//! deterministically from the fixture's own phase (no time dependence in
//! frame zero — see `phasor_trace`).

use dioxus::prelude::*;
use lpa_studio_core::{UiTimebaseState, Waveform};
use lpa_studio_web_story_macros::story;

use crate::app::node::NodePane;
use crate::app::node::face_story_fixtures::{clock_face, clock_node_view, phasor_reading};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ClockCardCanvas(children: Element) -> Element {
    rsx! {
        div { class: "tw:w-full tw:max-w-md", {children} }
    }
}

#[story(
    description = "Live face: three trace cards, one per downstream reading. `plasma · phase` rides its own private integrator (ramp), the two `bus:speed` readers share ONE integrator — violet border and id — while each shapes the cycle its own way (sine vs square). Rates auto-denominate (2/s → 3/min → 15/hr); tiny muted seconds sit in the section header; the Delta row is gone."
)]
fn default() -> Element {
    rsx! {
        ClockCardCanvas {
            NodePane {
                view: clock_node_view(clock_face(
                    UiTimebaseState::Live,
                    vec![
                        phasor_reading(
                            "plasma · phase",
                            None,
                            false,
                            0.62,
                            17,
                            20.0,
                            Waveform::Ramp,
                            0.0,
                        ),
                        phasor_reading(
                            "aurora · wave",
                            Some("bus:speed in fyeah_sign"),
                            true,
                            0.18,
                            3,
                            2.0,
                            Waveform::Sine,
                            0.0,
                        ),
                        phasor_reading(
                            "strobe · gate",
                            Some("bus:speed in fyeah_sign"),
                            true,
                            0.18,
                            3,
                            2.0,
                            Waveform::Square,
                            0.5,
                        ),
                    ],
                )),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Every reading on one shared channel: the violet border + violet id carry 'shared' on their own — the traces stay black-and-white (bound-violet convention; sharing is bus wiring, not a color of the data)."
)]
fn shared() -> Element {
    rsx! {
        ClockCardCanvas {
            NodePane {
                view: clock_node_view(clock_face(
                    UiTimebaseState::Live,
                    vec![
                        phasor_reading(
                            "plasma · phase",
                            Some("bus:speed in fyeah_sign"),
                            true,
                            0.35,
                            9,
                            8.0,
                            Waveform::Ramp,
                            0.0,
                        ),
                        phasor_reading(
                            "aurora · wave",
                            Some("bus:speed in fyeah_sign"),
                            true,
                            0.35,
                            9,
                            8.0,
                            Waveform::Triangle,
                            0.25,
                        ),
                    ],
                )),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "A crowded clock: eight readings including a frozen 0/s flat-line and a long reader name that truncates. The grid wraps at ~112px columns; the cap upstream is 8 readings per integrator, so this is as full as one face gets per phasor."
)]
fn crowd() -> Element {
    let waves = [
        Waveform::Ramp,
        Waveform::Sine,
        Waveform::Triangle,
        Waveform::Square,
    ];
    let cards = (0..8)
        .map(|index| {
            let period = match index {
                0 => 0.0, // frozen: 0/s, flat line
                1 => 0.5,
                2 => 100.0,
                _ => 2.0 + index as f32 * 3.0,
            };
            let label = if index == 3 {
                "a-very-long-shader-node-name · phase_offset_input".to_string()
            } else {
                format!("shader-{index} · phase")
            };
            let shared = index % 3 == 0;
            phasor_reading(
                &label,
                shared.then_some("bus:speed in fyeah_sign"),
                shared,
                (index as f32 * 0.13) % 1.0,
                index * 5,
                period,
                waves[index as usize % waves.len()],
                0.0,
            )
        })
        .collect();
    rsx! {
        ClockCardCanvas {
            NodePane {
                view: clock_node_view(clock_face(UiTimebaseState::Live, cards)),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Nothing is riding the timebase. A NORMAL state, not a failure: a project whose shaders declare no phasor has none, and so does one whose phasors have all gone idle. The line says exactly that instead of showing an empty box."
)]
fn no_phasors() -> Element {
    rsx! {
        ClockCardCanvas {
            NodePane {
                view: clock_node_view(clock_face(UiTimebaseState::Live, Vec::new())),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "The card just mounted and no timebase probe has answered yet. Deliberately distinct from the empty listing above — 'no read landed' and 'nothing is running' are different sentences."
)]
fn unread() -> Element {
    rsx! {
        ClockCardCanvas {
            NodePane {
                view: clock_node_view(clock_face(UiTimebaseState::Unread, Vec::new())),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "The runtime resolves no timebase for this product — the node just left the tree, or has never produced. A structured answer, not an error card."
)]
fn unknown() -> Element {
    rsx! {
        ClockCardCanvas {
            NodePane {
                view: clock_node_view(clock_face(UiTimebaseState::Unknown, Vec::new())),
                on_action: move |_| {},
            }
        }
    }
}
