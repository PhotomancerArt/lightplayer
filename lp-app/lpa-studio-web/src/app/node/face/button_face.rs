//! The button card's permanent face: the press control (a skeuomorphic momentary button, knob-family).
//!
//! A button node's whole job is a transition someone's finger makes. Studio
//! needs to make that transition without the finger — to rehearse a show, or
//! to prove a binding before the hardware is even on the desk — so the face
//! is one control that behaves like the physical button it stands in for.
//!
//! The gesture is WINDOWED, and the window is what makes one control cover
//! both events the runtime distinguishes:
//!
//! - pointer-up inside [`PRESS_WINDOW_MS`] → `Click` (the minimal
//!   down-then-up pair — a tap);
//! - the window elapsing with the pointer still down → `Press`, re-sent
//!   every [`RENEWAL_MS`] while held, then `Release` on pointer-up.
//!
//! The renewal cadence and the [`HOLD_TTL_MS`] TTL are a pair: the runtime
//! auto-releases a hold whose renewals stop, so a closed tab, a crashed
//! renderer, or a card that unmounts mid-hold cannot leave a button stuck
//! down. That is why teardown does NOTHING here — the renewal task is
//! scope-owned and simply dies with the component; chasing a guaranteed
//! `Release` on unmount would be a worse mechanism than the one already
//! guarding the wire.
//!
//! Everything in this file is the TIMING half of the affordance. The ops
//! themselves — and which controller they route to — are built by
//! [`lpa_studio_core::UiButtonFace`], so a synthetic press cannot drift from
//! the op that carries it.

use core::sync::atomic::{AtomicU32, Ordering};

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use lpa_studio_core::{UiAction, UiButtonFace as UiButtonFaceData};

use crate::app::node::NodeCardSection;
use crate::app::node::slot_fields::capture_field_pointer;

/// How long the pointer must stay down before the gesture becomes a HOLD
/// instead of a tap.
const PRESS_WINDOW_MS: u32 = 300;

/// Renewal cadence for a sustained hold. Comfortably inside
/// [`HOLD_TTL_MS`], so an ordinary render hitch never drops the hold.
const RENEWAL_MS: u32 = 1000;

/// A hold's device-side auto-release. The safety net for every teardown
/// path: nothing this component does on unmount, and nothing it could do,
/// matters as long as the renewals stop.
const HOLD_TTL_MS: u32 = 5000;

/// Per-gesture press id. A hold is identified by this id on the wire, so
/// renewals of the SAME hold repeat it and a new gesture never adopts an
/// old one. Tab-local is the right scope: renewals only ever come from the
/// tab that started the hold, and a reload's stale hold self-clears on TTL.
static NEXT_PRESS_ID: AtomicU32 = AtomicU32::new(1);

fn next_press_id() -> u32 {
    NEXT_PRESS_ID.fetch_add(1, Ordering::Relaxed)
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ButtonFace(
    face: UiButtonFaceData,
    /// Mount already looking held (stories render the pressed state
    /// without a pointer or a timer).
    #[props(default = false)]
    held_initially: bool,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    // The live gesture's press id — `None` between gestures. Clearing it is
    // ALSO how the renewal task is told to stop: the task compares the id
    // it started with on every tick, so a release, a cancel, or a fresh
    // gesture all retire the previous loop without any extra channel.
    let mut gesture = use_signal(|| None::<u32>);
    // True once the window has elapsed: the control looks held and the
    // pointer-up will be a Release rather than a Click.
    let mut held = use_signal(|| held_initially);
    let wired = on_action.is_some();

    let identity = button_identity(&face);
    let press_face = face.clone();
    let up_face = face.clone();
    let cancel_face = face;

    rsx! {
        NodeCardSection { label: "controls", first: true,
            div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-3 tw:px-4 tw:py-3",
                button {
                    class: press_button_class(wired),
                    r#type: "button",
                    aria_pressed: "{held()}",
                    disabled: !wired,
                    title: "Press and hold for a real hold; tap for a click. Reaches the runtime exactly like the physical button.",
                    onpointerdown: move |event| {
                        let Some(handler) = on_action else {
                            return;
                        };
                        event.stop_propagation();
                        // Capture so the pointer-up lands here even if the
                        // finger slides off the control mid-hold.
                        capture_field_pointer(&event);
                        let press_id = next_press_id();
                        gesture.set(Some(press_id));
                        held.set(false);
                        let face = press_face.clone();
                        spawn(async move {
                            TimeoutFuture::new(PRESS_WINDOW_MS).await;
                            // Released inside the window (or superseded):
                            // this gesture was a tap, and the pointer-up
                            // already sent its Click.
                            if *gesture.peek() != Some(press_id) {
                                return;
                            }
                            held.set(true);
                            // Renewal loop. Scope-owned: an unmount drops
                            // it mid-hold and the TTL does the rest.
                            while *gesture.peek() == Some(press_id) {
                                handler.call(face.press_action(press_id, HOLD_TTL_MS));
                                TimeoutFuture::new(RENEWAL_MS).await;
                            }
                        });
                    },
                    onpointerup: move |event| {
                        event.stop_propagation();
                        let (Some(press_id), Some(handler)) = (*gesture.peek(), on_action) else {
                            return;
                        };
                        let was_held = *held.peek();
                        gesture.set(None);
                        held.set(false);
                        if was_held {
                            handler.call(up_face.release_action(press_id));
                        } else {
                            handler.call(up_face.click_action());
                        }
                    },
                    onpointercancel: move |_| {
                        // A cancelled gesture is not a tap, so no Click —
                        // but an established hold must still be let go.
                        let (Some(press_id), Some(handler)) = (*gesture.peek(), on_action) else {
                            return;
                        };
                        let was_held = *held.peek();
                        gesture.set(None);
                        held.set(false);
                        if was_held {
                            handler.call(cancel_face.release_action(press_id));
                        }
                    },
                    // The physical anatomy, knob-family: a housing ring, a
                    // radial-gradient cap that visibly travels down while the
                    // pointer is on it, and the live-blue ring only once the
                    // hold is real on the runtime. Depression is mechanical
                    // truth (your finger is on it); blue is runtime truth.
                    svg {
                        class: "tw:block",
                        width: "48",
                        height: "48",
                        view_box: "0 0 48 48",
                        defs {
                            radialGradient {
                                id: "lp-press-cap-gradient",
                                cx: "35%",
                                cy: "30%",
                                r: "80%",
                                stop {
                                    offset: "0%",
                                    stop_color: "var(--studio-color-surface-raised-strong)",
                                }
                                stop {
                                    offset: "100%",
                                    stop_color: "var(--studio-color-surface-raised)",
                                }
                            }
                        }
                        // Housing: the panel-mount ring the cap sits in.
                        circle {
                            cx: "24",
                            cy: "24",
                            r: "16",
                            fill: "none",
                            stroke: press_ring_stroke(held()),
                            stroke_width: if held() { "2" } else { "1.5" },
                        }
                        // Cap: down and slightly smaller while the pointer is
                        // on it — a momentary button, mid-travel.
                        circle {
                            cx: "24",
                            cy: if gesture().is_some() || held() { "24.8" } else { "24" },
                            r: if gesture().is_some() || held() { "11.75" } else { "12.5" },
                            fill: "url(#lp-press-cap-gradient)",
                            stroke: press_ring_stroke(held()),
                        }
                        // Rest-state highlight dies while pressed: the cap
                        // face tips out of the light.
                        if gesture().is_none() && !held() {
                            circle {
                                cx: "20.5",
                                cy: "20",
                                r: "6",
                                fill: "var(--studio-color-surface-raised-strong)",
                                opacity: "0.35",
                            }
                        }
                    }
                }
                span { class: if held() { "tw:text-[0.75rem] tw:font-medium tw:text-status-live-foreground" } else { "tw:text-[0.75rem] tw:font-medium tw:text-strong-foreground" },
                    "Press"
                }
                if let Some(identity) = identity {
                    span { class: "tw:min-w-0 tw:truncate tw:font-mono tw:text-[0.7rem] tw:text-dim-foreground",
                        "{identity}"
                    }
                }
            }
        }
    }
}

/// The button's hardware identity beside the control: endpoint, message id,
/// or both. `None` when the def rows have not projected yet — the control
/// is the face, the readout is garnish.
fn button_identity(face: &UiButtonFaceData) -> Option<String> {
    match (face.endpoint.as_deref(), face.id) {
        (Some(endpoint), Some(id)) => Some(format!("{endpoint} · id {id}")),
        (Some(endpoint), None) => Some(endpoint.to_string()),
        (None, Some(id)) => Some(format!("id {id}")),
        (None, None) => None,
    }
}

/// Ring/cap stroke. A held button takes the LIVE family (blue) — the same
/// family the playlist strip's ACTIVE placard wears, because both say the
/// same thing: something is happening in the runtime right now. Deliberately
/// not green (green is good/valid, never state) and not violet (violet is
/// bound/bus). At rest it sits in the neutral knob-body family.
fn press_ring_stroke(held: bool) -> &'static str {
    if held {
        "var(--studio-status-live-border)"
    } else {
        "var(--studio-color-border-strong)"
    }
}

/// The button element is bare chrome — the SVG is the whole visual — so the
/// class only carries interaction affordances.
fn press_button_class(wired: bool) -> String {
    let cursor = if wired {
        " tw:cursor-pointer"
    } else {
        " tw:opacity-60"
    };
    format!(
        "tw:inline-flex tw:flex-none tw:touch-none tw:select-none tw:appearance-none \
         tw:items-center tw:rounded-full tw:border-none tw:bg-transparent tw:p-0 \
         tw:outline-none tw:focus-visible:outline tw:focus-visible:outline-1 \
         tw:focus-visible:outline-border-strong{cursor}"
    )
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::{ProjectNodeAddress, UiButtonFace as UiButtonFaceData};

    use super::{
        HOLD_TTL_MS, PRESS_WINDOW_MS, RENEWAL_MS, button_identity, press_button_class,
        press_ring_stroke,
    };

    fn face(endpoint: Option<&str>, id: Option<u32>) -> UiButtonFaceData {
        UiButtonFaceData {
            node: ProjectNodeAddress::parse("/demo.project/panel.button").unwrap(),
            endpoint: endpoint.map(str::to_string),
            id,
        }
    }

    #[test]
    fn renewals_stay_well_inside_the_hold_ttl() {
        // The whole teardown story rests on this: if a renewal can be later
        // than the TTL, an ordinary hitch reads as a release.
        assert!(
            RENEWAL_MS * 2 < HOLD_TTL_MS,
            "a hold must survive a missed renewal"
        );
        assert!(
            PRESS_WINDOW_MS < RENEWAL_MS,
            "the hold must be established before the first renewal is due"
        );
    }

    #[test]
    fn identity_degrades_one_piece_at_a_time() {
        assert_eq!(
            button_identity(&face(Some("button:gpio:D9"), Some(3))).as_deref(),
            Some("button:gpio:D9 · id 3")
        );
        assert_eq!(
            button_identity(&face(Some("button:gpio:D9"), None)).as_deref(),
            Some("button:gpio:D9")
        );
        assert_eq!(
            button_identity(&face(None, Some(3))).as_deref(),
            Some("id 3")
        );
        assert_eq!(button_identity(&face(None, None)), None);
    }

    #[test]
    fn held_never_borrows_the_bound_or_good_families() {
        let held = press_ring_stroke(true);
        assert!(
            held.contains("status-live"),
            "held reads as a live runtime state"
        );
        assert!(
            !held.contains("status-bound"),
            "violet is the binding/bus convention, not a diagnostic one"
        );
        assert!(
            !held.contains("status-good"),
            "green means good/valid, never state"
        );
        assert!(
            press_ring_stroke(false).contains("border-strong"),
            "at rest the button sits in the neutral knob-body family"
        );
        assert!(press_button_class(true).contains("cursor-pointer"));
        assert!(!press_button_class(false).contains("cursor-pointer"));
    }
}
