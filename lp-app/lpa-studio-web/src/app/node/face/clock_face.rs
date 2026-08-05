//! The clock card's permanent face: the published time product and the
//! per-reading phasor trace cards riding this timebase (clock-face v2 —
//! the direction converged at the TimeProduct G2 gate).
//!
//! Since the M2 break `bus:time` carries a `TimeProduct` handle, and
//! everything behind that handle — effective seconds, this tick's delta,
//! every phasor a consumer materialized — lives in the engine's timebase
//! store rather than in any slot. The probe now reports each integrator's
//! downstream READINGS too, so the face can answer "what is riding this
//! clock" the way a consumer would: one little black-and-white scrolling
//! trace per reading, drawing the SHAPED value that consumer actually
//! reads. Sharing is the violet border + id (bound-violet convention); the
//! trace itself stays monochrome. Seconds shrank into the section header;
//! the Delta row is gone ("isn't useful at all" — G2).
//!
//! **This is a debug listing, not a control panel.** There is no gesture
//! here at all: nothing creates, retunes, or deletes a phasor, because
//! phasors materialize on query and despawn on silence and both are the
//! consuming node's business. The one place a period IS editable is the
//! consuming shader's own speed knob.
//!
//! Three states have to read differently, and the middle one is the trap:
//!
//! | state | reads |
//! |---|---|
//! | no probe has landed | "Reading the timebase…" |
//! | live, no phasors | "Nothing is riding this timebase." — NORMAL |
//! | live, cards | one trace card per reading |
//!
//! An empty listing is not a failure and must never look like one: a
//! project whose shaders declare no phasor has none, and so does one whose
//! phasors have all gone idle.
//!
//! Animation lives in [`super::phasor_trace`]: one rAF driver per face,
//! extrapolating phase between probes and re-anchoring on each probe.

use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiClockFace as UiClockFaceData, UiPhasorReading, UiTimebaseState};

use crate::app::node::{NodeCardSection, ProducedProductView};

use super::phasor_trace::PhasorTraceDriver;

/// Monotonic per-face id base for trace canvases (one driver per face; one
/// canvas per card, addressed `{base}-{index}`).
static NEXT_TRACE_FACE_ID: AtomicU64 = AtomicU64::new(0);

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ClockFace(
    face: UiClockFaceData,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let driver = use_hook(|| Rc::new(PhasorTraceDriver::new()));
    let base_id = use_hook(|| {
        let id = NEXT_TRACE_FACE_ID.fetch_add(1, Ordering::Relaxed);
        format!("phasor-trace-{id}")
    });

    // Reconcile the animation driver with this render's cards: live cards
    // animate, every other state stops the loop.
    let cards: &[UiPhasorReading] = if face.timebase == UiTimebaseState::Live {
        &face.phasors
    } else {
        &[]
    };
    {
        let base = base_id.clone();
        driver.sync(cards, move |index| format!("{base}-{index}"));
    }

    rsx! {
        NodeCardSection { label: "output", first: true,
            div { class: "tw:grid tw:min-w-0 tw:justify-items-center tw:gap-2 tw:p-2",
                // The hero is the SECONDS COUNTER — the number a scrub or
                // speed change visibly moves ("Time product" as a caption
                // did nothing; PR review). The Delta row stays dead.
                ProducedProductView {
                    product: face.product.clone(),
                    on_action,
                    time_seconds: face.seconds.clone(),
                }
            }
        }
        NodeCardSection { label: "phasors",
            div { class: "tw:grid tw:min-w-0 tw:gap-1.5 tw:px-3 tw:py-2",
                match face.timebase {
                    // No read yet: say so plainly rather than showing an
                    // empty listing, which would read as "nothing running".
                    UiTimebaseState::Unread => rsx! {
                        p { class: "tw:m-0 tw:text-xs tw:text-subtle-foreground",
                            "Reading the timebase\u{2026}"
                        }
                    },
                    // A structured answer, not an error: a card asking about
                    // a node that just left the tree is not a fault.
                    UiTimebaseState::Unknown => rsx! {
                        p { class: "tw:m-0 tw:text-xs tw:text-subtle-foreground",
                            "This clock is not producing a timebase."
                        }
                    },
                    UiTimebaseState::Live if face.phasors.is_empty() => rsx! {
                        p { class: "tw:m-0 tw:text-xs tw:text-subtle-foreground",
                            "Nothing is riding this timebase."
                        }
                    },
                    UiTimebaseState::Live => rsx! {
                        div { class: "tw:grid tw:min-w-0 tw:grid-cols-[repeat(auto-fill,minmax(112px,1fr))] tw:gap-2",
                            for (index, reading) in face.phasors.iter().enumerate() {
                                PhasorTraceCard {
                                    key: "{index}-{reading.label}",
                                    reading: reading.clone(),
                                    canvas_id: format!("{base_id}-{index}"),
                                    index,
                                    driver: driver.clone(),
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}

/// One downstream reading: a scrolling trace of the shaped value the
/// consumer actually reads (right edge = now), the reader's id, and the
/// auto-denominated rate + waveform + completed cycles beneath it.
///
/// Shared readings wear the violet border and id (the shared thing is bus
/// wiring — exactly what violet means in Studio); the trace stays
/// black-and-white on the terminal background, spike proportions.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PhasorTraceCard(
    reading: UiPhasorReading,
    canvas_id: String,
    index: usize,
    driver: Rc<PhasorTraceDriver>,
) -> Element {
    let card_class = if reading.shared {
        "tw:flex tw:min-w-0 tw:flex-col tw:overflow-hidden tw:rounded-md tw:border tw:border-status-bound-border tw:bg-terminal"
    } else {
        "tw:flex tw:min-w-0 tw:flex-col tw:overflow-hidden tw:rounded-md tw:border tw:border-border-muted tw:bg-terminal"
    };
    let id_class = if reading.shared {
        "tw:min-w-0 tw:truncate tw:font-mono tw:text-[10px] tw:font-semibold tw:text-status-bound-foreground"
    } else {
        "tw:min-w-0 tw:truncate tw:font-mono tw:text-[10px] tw:font-semibold tw:text-muted-foreground"
    };
    // The shared channel rides the tooltip; the border is the signal.
    let id_title = reading
        .detail
        .clone()
        .unwrap_or_else(|| reading.label.clone());

    rsx! {
        div { class: card_class,
            // The canvas inherits its stroke color from `color` (B/W trace
            // tone); painting is imperative from the face's rAF driver —
            // never through the vdom. `ux-box-sized-canvas` declares that
            // this canvas's backing store tracks its CSS box, which the
            // story-capture ready gate asserts before it shoots.
            //
            // The BOX is inline rather than tailwind classes, and that is
            // load-bearing: the driver sizes the backing store from this
            // element's box, and an unstyled canvas takes its box from the
            // `width`/`height` attributes the driver writes — which at
            // dpr > 1 feeds itself and grows the element every frame until
            // the stylesheet lands. Inline declarations apply on the first
            // layout, before the stylesheet the wasm bundle injects.
            canvas {
                id: "{canvas_id}",
                style: "display:block;width:100%;height:42px",
                class: "ux-box-sized-canvas tw:text-strong-foreground",
                onmounted: move |_| driver.canvas_mounted(index),
            }
            div { class: "tw:grid tw:min-w-0 tw:gap-px tw:px-1.5 tw:pb-1 tw:pt-0.5",
                span { class: id_class, title: "{id_title}", "{reading.label}" }
                div { class: "tw:flex tw:min-w-0 tw:items-baseline tw:gap-1.5",
                    span { class: "tw:flex-none tw:font-mono tw:text-[10px] tw:tabular-nums tw:text-subtle-foreground",
                        "{reading.rate_display}"
                    }
                    span { class: "tw:min-w-0 tw:truncate tw:font-mono tw:text-[9px] tw:text-dim-foreground",
                        "{reading.waveform}"
                    }
                    span {
                        class: "tw:ml-auto tw:flex-none tw:font-mono tw:text-[9px] tw:tabular-nums tw:text-dim-foreground",
                        title: "Completed cycles since this integrator materialized",
                        "\u{00d7}{reading.cycle}"
                    }
                }
            }
        }
    }
}
