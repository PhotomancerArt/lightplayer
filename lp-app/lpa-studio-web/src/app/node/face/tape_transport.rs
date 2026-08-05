//! The clock card's OUTPUT hero: the tape transport instrument (plan
//! 2026-08-04-2355-clock-tape-hero, P3; design record
//! `spikes/clock-transport-hero/index.html`, gate rounds 1–2).
//!
//! One shared surface renders the transport: the scrolling tape strip
//! under a fixed playhead (painted by [`super::tape_driver`]), the calm
//! digits, the run/pause button, the log ׼–×8 speed fader with octave
//! detents, and the amber off-live chip. This phase renders the chrome
//! statically — every gesture (drag-scrub, fader, run/pause, tap-to-
//! return) lands in P4 through the standard slot-edit path.
//!
//! Spike verdicts binding here — with ONE live-build reversal: the zoom
//! is FIXED (a tape second is always the same pixels, so the speed
//! slider visibly changes how fast the strip streams; the spike's
//! speed-linked zoom is banked for the input-recorder reel). Still
//! binding: octave detents with ×1 pulling hardest; a fixed-width
//! readout that must NEVER reflow the fader; whole-second digits at rest
//! with tenths only mid-drag; amber = off-live (`status-attention`
//! family, class toggle on the box, not canvas).

use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use dioxus::prelude::*;
use lpa_studio_core::{LpValue, ProjectSlotAddress, UiAction, UiClockTransport};
use wasm_bindgen::JsCast;

use crate::app::node::slot_edit_actions::{panel_or_slot_action, slot_clear_action};
use crate::app::node::slot_fields::capture_field_pointer;
use crate::base::{InlineButton, InlineButtonTone, StudioIconName};

use super::tape_driver::TapeTransportDriver;

/// Minimum interval between drag dispatches (ms) — the KnobField throttle:
/// the geometry previews every pointer move, only the write stream is
/// capped, and pointerup flushes the final value (an unthrottled flood
/// re-arms the verdict-chase probe window).
const TAPE_DISPATCH_INTERVAL_MS: f64 = 50.0;

/// Base tape velocity at ×1, in css px per effective second (Q5).
pub(crate) const TAPE_BASE_PX_PER_SEC: f64 = 14.0;

/// The octave detent stops of the speed fader (round-2 verdict; a
/// quantize-to-0.1 mode was tried and rejected — no landmark stops; fine
/// speed lives on the phasor knobs).
pub(crate) const RATE_DETENTS: [f32; 6] = [0.25, 0.5, 1.0, 2.0, 4.0, 8.0];

/// The off-live chip appears past this offset magnitude, in seconds (the
/// amber box border tracks any non-zero offset; the chip waits for one
/// worth reading).
pub(crate) const OFFLIVE_CHIP_EPSILON_S: f32 = 0.05;

/// Monotonic per-face id base for the tape's imperatively-addressed
/// elements (canvas + digits), same idiom as the trace canvases.
static NEXT_TAPE_FACE_ID: AtomicU64 = AtomicU64::new(0);

/// FIXED zoom (Q5 reversed at the live build, 2026-08-05): a tape second
/// is always the same pixels, so a faster rate visibly STREAMS faster —
/// "I really expect the speed slider to make that tape move faster /
/// slower." The speed-linked zoom the spike converged on (constant pixel
/// velocity, rate changes seconds-per-pixel) is banked for the future
/// input-recorder reel, where packing more recorded time into the frame
/// is the point.
pub(crate) fn tape_px_per_sec() -> f64 {
    TAPE_BASE_PX_PER_SEC
}

/// Adaptive tick granularity, map-style: the first `[minor, major]` pair
/// where minors stay ≥ 8 css px apart AND major labels ≥ 44 css px apart.
const TAPE_TICK_LADDER: [(f64, f64); 7] = [
    (0.2, 1.0),
    (1.0, 5.0),
    (5.0, 30.0),
    (15.0, 60.0),
    (60.0, 300.0),
    (300.0, 1800.0),
    (900.0, 3600.0),
];

pub(crate) fn tape_tick_pair(pps_css: f64) -> (f64, f64) {
    for (minor, major) in TAPE_TICK_LADDER {
        if minor * pps_css >= 8.0 && major * pps_css >= 44.0 {
            return (minor, major);
        }
    }
    TAPE_TICK_LADDER[TAPE_TICK_LADDER.len() - 1]
}

/// Clock digits: `m:ss` (hours only when they exist: `h:mm:ss`), a real
/// minus sign ahead of negative time, tenths appended only mid-drag.
pub(crate) fn format_clock(seconds: f64, tenths: bool) -> String {
    let sign = if seconds < 0.0 { "\u{2212}" } else { "" };
    let t = seconds.abs();
    let hours = (t / 3600.0).floor() as i64;
    let minutes = ((t % 3600.0) / 60.0).floor() as i64;
    let secs = (t % 60.0).floor() as i64;
    let core = if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes}:{secs:02}")
    };
    if tenths {
        format!("{sign}{core}.{}", ((t % 1.0) * 10.0).floor() as i64)
    } else {
        format!("{sign}{core}")
    }
}

/// Where a rate sits on the fader track, as a 0–1 fraction of the log
/// ׼–×8 span.
pub(crate) fn rate_frac(rate: f32) -> f32 {
    ((rate.max(1e-6).log2() + 2.0) / 5.0).clamp(0.0, 1.0)
}

/// The readout's number: integers bare (`×1`), fractional rates with the
/// leading zero dropped (`×.25`), one decimal above ×1 (`×1.5`) — the
/// spike's `fmtRate`, fixed-width friendly.
pub(crate) fn format_rate(rate: f32) -> String {
    if rate >= 1.0 {
        if rate.fract() == 0.0 {
            format!("{}", rate as i64)
        } else {
            format!("{rate:.1}")
        }
    } else {
        let rounded = (rate * 100.0).round() / 100.0;
        format!("{rounded}").trim_start_matches('0').to_string()
    }
}

/// Whether the rate is seated exactly on an octave detent (the readout
/// wears the accent when it is).
pub(crate) fn on_detent(rate: f32) -> bool {
    RATE_DETENTS.contains(&rate)
}

/// The rate at a 0–1 fraction of the fader's log ׼–×8 track.
pub(crate) fn frac_rate(frac: f32) -> f32 {
    2.0_f32.powf(frac.clamp(0.0, 1.0) * 5.0 - 2.0)
}

/// Magnetic octave detents (round-2 verdict): a rate within the snap
/// radius of a stop — measured in log-frac so the pull feels equal at
/// both ends — seats exactly on it. ×1 pulls hardest (0.05 vs 0.028).
pub(crate) fn apply_detents(rate: f32) -> f32 {
    let rate = rate.clamp(0.25, 8.0);
    for detent in RATE_DETENTS {
        let radius = if detent == 1.0 { 0.05 } else { 0.028 };
        if (rate / detent).log2().abs() < radius * 5.0 {
            return detent;
        }
    }
    rate
}

/// The next detent an arrow key steps to: strictly the adjacent stop in
/// the pressed direction, from wherever the rate sits (a between-stops
/// rate steps onto the grid first).
pub(crate) fn adjacent_detent(rate: f32, up: bool) -> f32 {
    if up {
        RATE_DETENTS
            .into_iter()
            .find(|detent| *detent > rate)
            .unwrap_or(8.0)
    } else {
        RATE_DETENTS
            .into_iter()
            .rev()
            .find(|detent| *detent < rate)
            .unwrap_or(0.25)
    }
}

/// The scrub value a horizontal tape drag reaches: pixels convert to
/// seconds at the tape's fixed scale (px = s is the whole feel contract),
/// and dragging the tape rightward moves time backwards — the strip goes
/// where the finger goes.
pub(crate) fn scrub_drag_value(anchor_scrub: f32, dx_px: f64) -> f32 {
    anchor_scrub - (dx_px / tape_px_per_sec()) as f32
}

/// The `(address, handler)` pair a transport gesture needs to dispatch,
/// present only when the row was editable (the face builder withholds the
/// address of a read-only row — `editable_row_address`) and the conduit is
/// wired. `field_wiring`'s contract with the DTO's own editability
/// encoding.
fn transport_wiring(
    address: &Option<ProjectSlotAddress>,
    on_action: Option<EventHandler<UiAction>>,
) -> Option<(ProjectSlotAddress, EventHandler<UiAction>)> {
    Some((address.clone()?, on_action?))
}

/// The fader-track fraction under a pointer event: the pointer's x inside
/// the track's padding box, thumb half-width (8 px) inset at both ends,
/// clamped so a drag past the track pins to the domain edge. `None`
/// outside a real browser event.
fn fader_frac_from_event(event: &Event<PointerData>) -> Option<f32> {
    use dioxus::web::WebEventExt;

    let web_event = event.data().try_as_web_event()?;
    let target = web_event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::Element>().ok())?;
    let rect = target.get_bounding_client_rect();
    let width = rect.width() - 16.0;
    if width <= 0.0 {
        return None;
    }
    let frac = ((f64::from(web_event.client_x()) - rect.left() - 8.0) / width).clamp(0.0, 1.0);
    Some(frac as f32)
}

/// The tape transport instrument, one per clock face. Every gesture —
/// drag-scrub, fader, run/pause, tap-to-return — dispatches through the
/// standard slot-edit path (`panel_or_slot_action` with no panel target
/// yet, so P6's panel exposure is a data change): the actor coalesces the
/// flood per address, and the edit buffer's staged echo keeps the DTO
/// stable under the finger (P2's e2e contract).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn TapeTransport(
    transport: UiClockTransport,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let driver = use_hook(|| {
        let id = NEXT_TAPE_FACE_ID.fetch_add(1, Ordering::Relaxed);
        Rc::new(TapeTransportDriver::new(
            format!("tape-transport-{id}"),
            format!("tape-transport-digits-{id}"),
        ))
    });
    driver.sync(&transport);

    let scrub_wired = transport_wiring(&transport.scrub_address, on_action);
    let rate_wired = transport_wiring(&transport.rate_address, on_action);
    let running_wired = transport_wiring(&transport.running_address, on_action);

    let offlive = transport.scrub_offset_seconds != 0.0;
    let chip_visible = transport.scrub_offset_seconds.abs() > OFFLIVE_CHIP_EPSILON_S;
    let running = transport.running;
    let rate = transport.rate;
    let staged_scrub = transport.scrub_offset_seconds;

    // Scrub drag anchor: pointer x and staged scrub at pointerdown; None
    // while idle. The strip's preview lives in the driver (it repaints
    // per pointer move without a vdom pass); the throttle caps the write
    // stream only.
    let mut scrub_drag = use_signal(|| None::<(f64, f32)>);
    let scrub_last_sent = use_signal(|| 0.0_f64);
    // Fader drag-local echo (the knob preview pattern): the thumb and
    // readout render the value under the hand; display truth catches up
    // through the staged echo.
    let mut rate_preview = use_signal(|| None::<f32>);
    let rate_last_sent = use_signal(|| 0.0_f64);
    let mut fader_dragging = use_signal(|| false);

    let shown_rate = rate_preview().unwrap_or(rate);

    // Amber = off-live, a class toggle on the box (never canvas): the
    // border is the ambient signal, the chip is the actionable one.
    let box_class = if offlive {
        "tw:overflow-hidden tw:rounded-md tw:border tw:border-status-attention-border tw:bg-terminal"
    } else {
        "tw:overflow-hidden tw:rounded-md tw:border tw:border-border-muted tw:bg-terminal"
    };
    let canvas_class = if scrub_wired.is_none() {
        "tw:block tw:h-[62px] tw:w-full tw:text-strong-foreground"
    } else if scrub_drag().is_some() {
        "tw:block tw:h-[62px] tw:w-full tw:cursor-grabbing tw:touch-none tw:text-strong-foreground"
    } else {
        "tw:block tw:h-[62px] tw:w-full tw:cursor-grab tw:touch-none tw:text-strong-foreground"
    };
    // The transport is Debug territory: a control with an ACTIVE session
    // override wears the debug family's orange tint (no hazard stripes —
    // "a bit strong" per gate feedback; the tint + Clear carry it).
    let running_changed = transport.running_override.is_some();
    let rate_changed = transport.rate_override.is_some();
    // `button { font: inherit }` in the base sheet beats layered tw
    // utilities — the font is set explicitly here (wiring-UI lesson).
    // Changed-tint outranks the run-state accent: an overridden control
    // announces the override first (the glyph still says which state).
    let run_class = if running_changed {
        "tw:inline-flex tw:h-7 tw:min-w-[34px] tw:flex-none tw:cursor-pointer tw:items-center tw:justify-center tw:rounded-[7px] tw:border tw:border-status-attention-border tw:bg-card-raised tw:px-2.5 tw:font-sans tw:text-xs tw:font-semibold tw:text-status-attention-foreground tw:disabled:cursor-default"
    } else if running {
        "tw:inline-flex tw:h-7 tw:min-w-[34px] tw:flex-none tw:cursor-pointer tw:items-center tw:justify-center tw:rounded-[7px] tw:border tw:border-border-strong tw:bg-card-raised tw:px-2.5 tw:font-sans tw:text-xs tw:font-semibold tw:text-accent tw:disabled:cursor-default"
    } else {
        "tw:inline-flex tw:h-7 tw:min-w-[34px] tw:flex-none tw:cursor-pointer tw:items-center tw:justify-center tw:rounded-[7px] tw:border tw:border-border-strong tw:bg-card-raised tw:px-2.5 tw:font-sans tw:text-xs tw:font-semibold tw:text-muted-foreground tw:hover:text-strong-foreground tw:disabled:cursor-default"
    };
    let track_class = if rate_wired.is_none() {
        "tw:relative tw:h-[22px] tw:w-[190px] tw:flex-none tw:rounded-md tw:border tw:border-border-muted tw:bg-track"
    } else {
        "tw:relative tw:h-[22px] tw:w-[190px] tw:flex-none tw:cursor-ew-resize tw:touch-none tw:rounded-md tw:border tw:border-border-muted tw:bg-track tw:outline-none tw:focus-visible:outline tw:focus-visible:outline-1 tw:focus-visible:outline-border-strong"
    };
    let readout_value_class = if rate_changed {
        "tw:font-semibold tw:text-status-attention-foreground"
    } else if on_detent(shown_rate) {
        "tw:font-semibold tw:text-accent"
    } else {
        "tw:font-semibold tw:text-strong-foreground"
    };
    let thumb_class = if rate_changed {
        "tw:pointer-events-none tw:absolute tw:inset-y-0.5 tw:w-4 tw:rounded tw:border tw:border-status-attention-border tw:bg-card-raised tw:after:absolute tw:after:inset-y-[3px] tw:after:left-1/2 tw:after:w-px tw:after:bg-status-attention-foreground tw:after:content-['']"
    } else {
        "tw:pointer-events-none tw:absolute tw:inset-y-0.5 tw:w-4 tw:rounded tw:border tw:border-border-strong tw:bg-card-raised tw:after:absolute tw:after:inset-y-[3px] tw:after:left-1/2 tw:after:w-px tw:after:bg-accent tw:after:content-['']"
    };
    // Every active override's Clear target, for the one clear affordance
    // (D7 vocabulary: debug overrides CLEAR — never revert/reset).
    let override_targets: Vec<_> = [
        transport.running_override.clone(),
        transport.rate_override.clone(),
        transport.scrub_override.clone(),
    ]
    .into_iter()
    .flatten()
    .collect();
    let thumb_style = format!(
        "left: calc((100% - 18px) * {} + 1px);",
        rate_frac(shown_rate)
    );
    let chip_sign = if transport.scrub_offset_seconds < 0.0 {
        "\u{2212}"
    } else {
        "+"
    };
    let chip_offset = transport.scrub_offset_seconds.abs();
    // Captured ONCE: the driver owns this span's text after mount, and a
    // Dioxus patch racing the driver's imperative writes would flash the
    // stale anchor value (the driver's change-cache would then skip the
    // correction until the next whole-second flip). A constant initial
    // render means the vdom never patches it again.
    let initial_digits = use_hook(|| format_clock(f64::from(transport.seconds), false));

    let mounted_driver = driver.clone();
    let scrub_down_wired = scrub_wired.clone();
    let scrub_move_wired = scrub_wired.clone();
    let scrub_up_wired = scrub_wired.clone();
    let scrub_move_driver = driver.clone();
    let scrub_up_driver = driver.clone();
    let scrub_cancel_driver = driver.clone();
    let rate_move_wired = rate_wired.clone();
    let rate_up_wired = rate_wired.clone();
    let rate_dbl_wired = rate_wired.clone();
    let rate_key_wired = rate_wired.clone();
    let rate_down_wired = rate_wired.clone();
    let run_wired = running_wired.clone();
    let chip_wired = scrub_wired.clone();
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-2",
            div { class: box_class,
                // Painted imperatively by the face's rAF driver — never
                // through the vdom (see `tape_driver`). The canvas is the
                // scrub surface: horizontal drag, px = seconds at the
                // current zoom.
                canvas {
                    id: "{driver.canvas_id()}",
                    class: canvas_class,
                    onmounted: move |_| mounted_driver.canvas_mounted(),
                    onpointerdown: move |event| {
                        if scrub_down_wired.is_none() {
                            return;
                        }
                        capture_field_pointer(&event);
                        scrub_drag
                            .set(Some((event.data().client_coordinates().x, staged_scrub)));
                    },
                    onpointermove: move |event| {
                        let mut last_sent = scrub_last_sent;
                        let Some((anchor_x, anchor_scrub)) = scrub_drag() else {
                            return;
                        };
                        if event.data().held_buttons().is_empty() {
                            // Missed release (no pointer capture): stop.
                            scrub_drag.set(None);
                            scrub_move_driver.set_scrub_drag(None);
                            return;
                        }
                        let Some((address, handler)) = scrub_move_wired.clone() else {
                            return;
                        };
                        let dx = event.data().client_coordinates().x - anchor_x;
                        let next = scrub_drag_value(anchor_scrub, dx);
                        scrub_move_driver.set_scrub_drag(Some(next));
                        let now = js_sys::Date::now();
                        if now - last_sent() < TAPE_DISPATCH_INTERVAL_MS {
                            return;
                        }
                        last_sent.set(now);
                        handler
                            .call(panel_or_slot_action(&None, address, LpValue::F32(next)));
                    },
                    onpointerup: move |_| {
                        scrub_drag.set(None);
                        // Flush the final position: the throttle may have
                        // swallowed the last few moves, and the release
                        // must land exactly.
                        if let (Some(next), Some((address, handler))) = (
                            scrub_up_driver.scrub_preview(),
                            scrub_up_wired.clone(),
                        ) {
                            handler
                                .call(
                                    panel_or_slot_action(&None, address, LpValue::F32(next)),
                                );
                        }
                        scrub_up_driver.set_scrub_drag(None);
                    },
                    onpointercancel: move |_| {
                        scrub_drag.set(None);
                        scrub_cancel_driver.set_scrub_drag(None);
                    },
                }
            }
            div { class: "tw:flex tw:min-w-0 tw:flex-wrap tw:items-center tw:gap-2.5",
                button {
                    r#type: "button",
                    class: run_class,
                    disabled: running_wired.is_none(),
                    title: if running { "Pause the clock" } else { "Run the clock" },
                    onclick: move |_| {
                        if let Some((address, handler)) = run_wired.clone() {
                            handler
                                .call(
                                    panel_or_slot_action(
                                        &None,
                                        address,
                                        LpValue::Bool(!running),
                                    ),
                                );
                        }
                    },
                    span { class: "tw:text-xs tw:leading-none",
                        if running { "\u{275a}\u{275a}" } else { "\u{25b6}" }
                    }
                }
                // The driver rewrites this span's text every displayed
                // second (whole seconds at rest, tenths mid-drag) — the
                // initial render is the DTO's own anchor, deterministic.
                span {
                    id: "{driver.digits_id()}",
                    class: "tw:font-mono tw:text-lg tw:font-semibold tw:leading-none tw:tracking-[0.01em] tw:tabular-nums tw:text-strong-foreground",
                    "{initial_digits}"
                }
                span { class: "tw:ml-auto tw:inline-flex tw:flex-none tw:items-center tw:gap-2",
                    span { class: "tw:text-[9px] tw:uppercase tw:tracking-[0.1em] tw:text-dim-foreground",
                        "speed"
                    }
                    span {
                        class: track_class,
                        title: "drag \u{00b7} double-click = \u{00d7}1",
                        role: "slider",
                        tabindex: if rate_wired.is_some() { "0" } else { "-1" },
                        aria_valuemin: "0.25",
                        aria_valuemax: "8",
                        aria_valuenow: "{shown_rate}",
                        onpointerdown: move |event| {
                            if rate_down_wired.is_none() {
                                return;
                            }
                            capture_field_pointer(&event);
                            fader_dragging.set(true);
                            // Jump-to-click, detents applied — the spike's
                            // pointerdown behavior.
                            let mut last_sent = rate_last_sent;
                            if let (Some(frac), Some((address, handler))) = (
                                fader_frac_from_event(&event),
                                rate_down_wired.clone(),
                            ) {
                                let next = apply_detents(frac_rate(frac));
                                rate_preview.set(Some(next));
                                last_sent.set(js_sys::Date::now());
                                handler
                                    .call(
                                        panel_or_slot_action(&None, address, LpValue::F32(next)),
                                    );
                            }
                        },
                        onpointermove: move |event| {
                            let mut last_sent = rate_last_sent;
                            if !fader_dragging() {
                                return;
                            }
                            if event.data().held_buttons().is_empty() {
                                fader_dragging.set(false);
                                rate_preview.set(None);
                                return;
                            }
                            let (Some(frac), Some((address, handler))) = (
                                fader_frac_from_event(&event),
                                rate_move_wired.clone(),
                            ) else {
                                return;
                            };
                            let next = apply_detents(frac_rate(frac));
                            rate_preview.set(Some(next));
                            let now = js_sys::Date::now();
                            if now - last_sent() < TAPE_DISPATCH_INTERVAL_MS {
                                return;
                            }
                            last_sent.set(now);
                            handler
                                .call(panel_or_slot_action(&None, address, LpValue::F32(next)));
                        },
                        onpointerup: move |_| {
                            fader_dragging.set(false);
                            // Flush the seated value exactly.
                            if let (Some(next), Some((address, handler))) = (
                                rate_preview(),
                                rate_up_wired.clone(),
                            ) {
                                handler
                                    .call(
                                        panel_or_slot_action(&None, address, LpValue::F32(next)),
                                    );
                            }
                            rate_preview.set(None);
                        },
                        onpointercancel: move |_| {
                            fader_dragging.set(false);
                            rate_preview.set(None);
                        },
                        ondoubleclick: move |_| {
                            if let Some((address, handler)) = rate_dbl_wired.clone() {
                                handler
                                    .call(
                                        panel_or_slot_action(&None, address, LpValue::F32(1.0)),
                                    );
                            }
                        },
                        onkeydown: move |event| {
                            let Some((address, handler)) = rate_key_wired.clone() else {
                                return;
                            };
                            // Arrows step detent to detent (the stops ARE
                            // the keyboard grid); Home/End are the ends.
                            let next = match event.key() {
                                Key::ArrowUp | Key::ArrowRight => {
                                    adjacent_detent(shown_rate, true)
                                }
                                Key::ArrowDown | Key::ArrowLeft => {
                                    adjacent_detent(shown_rate, false)
                                }
                                Key::Home => 0.25,
                                Key::End => 8.0,
                                _ => return,
                            };
                            event.prevent_default();
                            handler
                                .call(panel_or_slot_action(&None, address, LpValue::F32(next)));
                        },
                        // Detent ticks on the track, ×1 emphasized. The
                        // children stay pointer-transparent so the track
                        // is always the gesture target (its rect is the
                        // fraction domain).
                        for detent in RATE_DETENTS {
                            span {
                                key: "{detent}",
                                class: if detent == 1.0 { "tw:pointer-events-none tw:absolute tw:bottom-0.5 tw:top-[12px] tw:w-px tw:bg-subtle-foreground" } else { "tw:pointer-events-none tw:absolute tw:bottom-0.5 tw:top-[15px] tw:w-px tw:bg-border-strong" },
                                style: format!(
                                    "left: calc({:.1}% + {:.1}px);",
                                    rate_frac(detent) * 100.0,
                                    (1.0 - rate_frac(detent)) * 16.0 - 8.0,
                                ),
                            }
                        }
                        span { class: thumb_class, style: thumb_style }
                    }
                    // Fixed width: a changing readout must NEVER reflow
                    // the fader (round-2 gate feedback).
                    span { class: "tw:w-9 tw:flex-none tw:text-left tw:font-mono tw:text-[11px] tw:tabular-nums tw:text-muted-foreground",
                        "\u{00d7}"
                        span { class: readout_value_class, {format_rate(shown_rate)} }
                    }
                }
                if chip_visible {
                    button {
                        r#type: "button",
                        class: "tw:inline-flex tw:flex-none tw:cursor-pointer tw:items-center tw:gap-1 tw:whitespace-nowrap tw:rounded-md tw:border tw:border-status-attention-border tw:bg-status-attention-bg tw:px-2 tw:py-1 tw:font-mono tw:text-[10px] tw:font-normal tw:text-status-attention-foreground tw:disabled:cursor-default",
                        disabled: chip_wired.is_none(),
                        title: "scrubbed off-live \u{2014} tap to return",
                        onclick: move |_| {
                            if let Some((address, handler)) = chip_wired.clone() {
                                handler
                                    .call(
                                        panel_or_slot_action(&None, address, LpValue::F32(0.0)),
                                    );
                            }
                        },
                        span { class: "tw:font-semibold", "{chip_sign}{chip_offset:.1} s" }
                        span { " \u{00b7} live" }
                    }
                }
                if !override_targets.is_empty() && on_action.is_some() {
                    InlineButton {
                        label: "Clear transport overrides",
                        icon: StudioIconName::Revert,
                        text: "clear",
                        tone: InlineButtonTone::Attention,
                        title: "Clear this transport's debug overrides \u{2014} session only",
                        on_press: move |_| {
                            if let Some(handler) = on_action {
                                for address in override_targets.clone() {
                                    handler.call(slot_clear_action(address));
                                }
                            }
                        },
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Calm digits: m:ss under an hour, h:mm:ss above, a real minus sign,
    /// tenths only when asked (mid-drag).
    #[test]
    fn clock_digits_format_calm_and_signed() {
        assert_eq!(format_clock(0.0, false), "0:00");
        assert_eq!(format_clock(447.0, false), "7:27");
        assert_eq!(format_clock(59.999, false), "0:59");
        assert_eq!(format_clock(60.0, false), "1:00");
        assert_eq!(format_clock(3600.0, false), "1:00:00");
        assert_eq!(format_clock(3.0 * 3600.0 + 47.0 * 60.0, false), "3:47:00");
        assert_eq!(format_clock(-12.4, false), "\u{2212}0:12");
        assert_eq!(format_clock(447.35, true), "7:27.3");
        assert_eq!(format_clock(-12.46, true), "\u{2212}0:12.4");
    }

    /// FIXED zoom (Q5 reversed): a tape second is always the same pixels,
    /// so rate changes how fast the strip STREAMS, never its scale.
    #[test]
    fn zoom_is_fixed_so_rate_changes_velocity() {
        assert_eq!(tape_px_per_sec(), 14.0);
    }

    /// The ladder picks the first pair keeping minors ≥ 8 css px and major
    /// labels ≥ 44 css px. At the tape's fixed scale that is always
    /// 1 s / 5 s; the ladder itself stays correct for any future zoom
    /// (the input-recorder reel inherits it).
    #[test]
    fn tick_ladder_adapts_to_zoom() {
        assert_eq!(tape_tick_pair(tape_px_per_sec()), (1.0, 5.0));
        assert_eq!(tape_tick_pair(1.75), (5.0, 30.0));
        assert_eq!(tape_tick_pair(56.0), (0.2, 1.0));
        // Absurdly zoomed out: the coarsest pair is the floor.
        assert_eq!(tape_tick_pair(0.01), (900.0, 3600.0));
    }

    /// The fader is log ׼–×8: detents land at even fractions and the
    /// ends clamp.
    #[test]
    fn rate_fraction_is_log_over_the_span() {
        assert_eq!(rate_frac(0.25), 0.0);
        assert_eq!(rate_frac(1.0), 0.4);
        assert_eq!(rate_frac(8.0), 1.0);
        assert!((rate_frac(2.0) - 0.6).abs() < 1e-6);
        assert_eq!(rate_frac(16.0), 1.0);
    }

    /// Readout numbers stay fixed-width friendly: integers bare, sub-×1
    /// drops the leading zero, one decimal above ×1.
    #[test]
    fn rate_readout_matches_the_spike() {
        assert_eq!(format_rate(1.0), "1");
        assert_eq!(format_rate(8.0), "8");
        assert_eq!(format_rate(0.25), ".25");
        assert_eq!(format_rate(0.5), ".5");
        assert_eq!(format_rate(1.5), "1.5");
        assert!(on_detent(1.0));
        assert!(!on_detent(1.3));
    }

    /// The log fader mapping round-trips, and the octave detents are
    /// magnetic with ×1 pulling hardest (0.05 log-frac vs 0.028).
    #[test]
    fn detents_snap_and_one_pulls_hardest() {
        // Fraction ↔ rate round trip along the log track.
        for rate in [0.25_f32, 0.5, 1.0, 2.0, 4.0, 8.0] {
            let back = frac_rate(rate_frac(rate));
            assert!((back - rate).abs() < rate * 1e-5, "{rate} -> {back}");
        }
        // Every detent snaps to itself.
        for detent in RATE_DETENTS {
            assert_eq!(apply_detents(detent), detent);
        }
        // Near ×1 inside its wide radius: log2(1.15) ≈ 0.20 < 0.25 snaps…
        assert_eq!(apply_detents(1.15), 1.0);
        // …but the same log distance from ×2 stays free (radius 0.14).
        assert_eq!(apply_detents(2.3), 2.3);
        // Just inside ×2's narrow radius does snap.
        assert_eq!(apply_detents(2.1), 2.0);
        // Between stops, outside every radius: untouched.
        assert_eq!(apply_detents(1.5), 1.5);
        // The gesture domain clamps to the fader's span.
        assert_eq!(apply_detents(0.1), 0.25);
        assert_eq!(apply_detents(20.0), 8.0);
    }

    /// Arrow keys walk the detent grid: adjacent stop in the pressed
    /// direction, off-grid rates step onto the grid, ends clamp.
    #[test]
    fn arrows_step_detent_to_detent() {
        assert_eq!(adjacent_detent(1.0, true), 2.0);
        assert_eq!(adjacent_detent(1.0, false), 0.5);
        assert_eq!(adjacent_detent(1.5, true), 2.0);
        assert_eq!(adjacent_detent(1.5, false), 1.0);
        assert_eq!(adjacent_detent(8.0, true), 8.0);
        assert_eq!(adjacent_detent(0.25, false), 0.25);
    }

    /// Drag-scrub is px = seconds at the tape's fixed scale, tape moving
    /// with the finger (rightward drag = backwards in time) — the same
    /// conversion at every rate now that zoom is fixed.
    #[test]
    fn scrub_drag_converts_pixels_at_the_fixed_scale() {
        // 14 px/s — a 14 px rightward drag is exactly −1 s.
        assert_eq!(scrub_drag_value(0.0, 14.0), -1.0);
        // Leftward drag scrubs forward, on top of the anchor.
        assert_eq!(scrub_drag_value(-2.0, -28.0), 0.0);
    }
}
