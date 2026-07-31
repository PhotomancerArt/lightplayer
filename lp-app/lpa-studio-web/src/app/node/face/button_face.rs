//! The button card's permanent face: the simulate-press control.
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
        NodeCardSection { label: "simulate", first: true,
            div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-3 tw:px-4 tw:py-3",
                button {
                    class: press_button_class(held(), wired),
                    r#type: "button",
                    disabled: !wired,
                    title: "Press and hold to send a real hold; tap for a click. Synthetic events reach the runtime exactly like the physical button.",
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
                    if held() { "Holding…" } else { "Simulate press" }
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

/// Control chrome. Held takes the LIVE family (blue) — the same family the
/// playlist strip's ACTIVE placard wears, because both say the same thing:
/// something is happening in the runtime right now. Deliberately not green
/// (green is good/valid, never state) and not violet (violet is bound/bus).
fn press_button_class(held: bool, wired: bool) -> String {
    let surface = if held {
        "tw:border-status-live-border tw:bg-status-live-bg tw:text-status-live-foreground"
    } else {
        "tw:border-border-strong tw:bg-card-subtle tw:text-strong-foreground tw:hover:bg-card-muted"
    };
    let cursor = if wired {
        " tw:cursor-pointer"
    } else {
        " tw:opacity-60"
    };
    format!(
        "tw:inline-flex tw:flex-none tw:touch-none tw:select-none tw:appearance-none \
         tw:items-center tw:rounded-sm tw:border tw:px-3 tw:py-1.5 tw:text-[0.8rem] \
         tw:font-medium tw:transition-colors tw:motion-reduce:transition-none \
         {surface}{cursor}"
    )
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::{ProjectNodeAddress, UiButtonFace as UiButtonFaceData};

    use super::{HOLD_TTL_MS, PRESS_WINDOW_MS, RENEWAL_MS, button_identity, press_button_class};

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
        let held = press_button_class(true, true);
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
        assert!(press_button_class(false, true).contains("cursor-pointer"));
        assert!(!press_button_class(false, false).contains("cursor-pointer"));
    }
}
