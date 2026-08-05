//! The clock card's permanent face: the published time product, the plain
//! seconds readings, and the read-only listing of the phasors riding this
//! timebase (parent D10).
//!
//! Since the M2 break `bus:time` carries a `TimeProduct` handle, and
//! everything behind that handle — effective seconds, this tick's delta,
//! every phasor a consumer materialized — lives in the engine's timebase
//! store rather than in any slot. Nothing on the ordinary read surface can
//! see it, which is why a clock's card had no way to answer "what is
//! actually running right now?".
//!
//! **This is a debug listing, not a control panel.** There is no gesture
//! here at all: nothing creates, retunes, or deletes a phasor, because
//! phasors materialize on query and despawn on silence and both are the
//! consuming node's business. The one place a period IS editable is the
//! consuming shader's own period knob.
//!
//! Three states have to read differently, and the middle one is the trap:
//!
//! | state | reads |
//! |---|---|
//! | no probe has landed | "Reading the timebase…" |
//! | live, no phasors | "Nothing is riding this timebase." — NORMAL |
//! | live, rows | one row per integrator |
//!
//! An empty listing is not a failure and must never look like one: a
//! project whose shaders declare no phasor has none, and so does one whose
//! phasors have all gone idle.
//!
//! A **shared** row is the listing's one load-bearing fact: a phasor keyed
//! by a config channel is ONE integrator for every reader of that channel
//! (parent D3), so its period is not a private setting. It gets the bound
//! violet family — the shared thing here is bus wiring, which is exactly
//! what violet means in Studio ([[studio-bound-violet-convention]]).

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiClockFace as UiClockFaceData, UiPhasorRow, UiTimebaseState};

use crate::app::node::{NodeCardSection, ProducedProductView, ProducedValues};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ClockFace(
    face: UiClockFaceData,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    rsx! {
        NodeCardSection { label: "output", first: true,
            div { class: "tw:grid tw:min-w-0 tw:justify-items-center tw:gap-2 tw:p-2",
                ProducedProductView { product: face.product.clone(), on_action }
            }
            if !face.readings.is_empty() {
                div { class: "tw:min-w-0 tw:px-2 tw:pb-2",
                    ProducedValues { values: face.readings.clone(), on_action }
                }
            }
        }
        NodeCardSection { label: "phasors",
            div { class: "tw:grid tw:min-w-0 tw:gap-1 tw:px-3 tw:py-2",
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
                        for (index, row) in face.phasors.iter().enumerate() {
                            PhasorRow { key: "{index}-{row.origin}", row: row.clone() }
                        }
                    },
                }
            }
        }
    }
}

/// One integrator: who it belongs to, how long its cycle is, where in that
/// cycle it currently sits, and how many cycles it has completed.
///
/// The bar is the RAW `[0,1)` ramp — never a shaped reading. `waveform` and
/// `phase_offset` are per-consumer output shaping applied after the store,
/// so a shared integrator has exactly one phase and possibly several
/// differently-shaped readings of it; picking one reader's and calling it
/// the phasor's would be a lie the listing cannot detect.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PhasorRow(row: UiPhasorRow) -> Element {
    let origin_class = if row.shared {
        "tw:min-w-0 tw:truncate tw:font-mono tw:text-[11px] tw:font-semibold tw:text-status-bound-foreground"
    } else {
        "tw:min-w-0 tw:truncate tw:font-mono tw:text-[11px] tw:font-semibold tw:text-strong-foreground"
    };
    let fill_class = if row.shared {
        "tw:h-full tw:rounded-xs tw:bg-status-bound-foreground"
    } else {
        "tw:h-full tw:rounded-xs tw:bg-accent"
    };
    // The bar geometry follows the QUANTIZED phase for the same reason the
    // readout does: a row that only changes when its number changes keeps
    // the whole-DTO change gate quiet on a slow phasor.
    let percent = (row.phase.clamp(0.0, 1.0) * 100.0).round();

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-0.5",
            div { class: "tw:flex tw:min-w-0 tw:items-baseline tw:gap-1.5",
                span { class: origin_class, "{row.origin}" }
                if row.shared {
                    span {
                        class: "tw:flex-none tw:text-[9px] tw:font-bold tw:uppercase tw:tracking-wide tw:text-status-bound-foreground",
                        title: "One integrator for every reader of this channel — its period is shared",
                        "shared"
                    }
                }
                if let Some(detail) = &row.detail {
                    span { class: "tw:min-w-0 tw:truncate tw:text-[10px] tw:text-subtle-foreground", "{detail}" }
                }
                span { class: "tw:min-w-0 tw:flex-1" }
                span { class: "tw:flex-none tw:font-mono tw:text-[10px] tw:tabular-nums tw:text-muted-foreground",
                    "{row.period_display}"
                }
            }
            div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2",
                div { class: "tw:h-1 tw:min-w-0 tw:flex-1 tw:overflow-hidden tw:rounded-xs tw:bg-border-strong",
                    div { class: fill_class, style: "width: {percent}%" }
                }
                span { class: "tw:flex-none tw:font-mono tw:text-[10px] tw:tabular-nums tw:text-muted-foreground",
                    "\u{03c6} {row.phase_display}"
                }
                span {
                    class: "tw:flex-none tw:font-mono tw:text-[10px] tw:tabular-nums tw:text-subtle-foreground",
                    title: "Completed cycles since this integrator materialized",
                    "\u{00d7}{row.cycle}"
                }
            }
        }
    }
}
