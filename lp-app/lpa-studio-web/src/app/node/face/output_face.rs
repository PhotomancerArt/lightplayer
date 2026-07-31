//! The output card's permanent face: the test-pattern toggle.
//!
//! The first question anyone asks of an LED output is not "what is it
//! showing" but "is it wired, addressed, and alive at all". The toggle
//! answers that directly: while it is on, the runtime drives every pixel
//! full white instead of the graph's frames, so a dark strip is a wiring
//! fault rather than an empty show.
//!
//! Sustained the same way a button hold is: the pattern carries
//! [`PATTERN_TTL_MS`] and is re-sent every [`RENEWAL_MS`] while on, so a
//! closed tab or an unmounted card restores normal output within a couple
//! of seconds without this component doing anything on teardown. The
//! renewal task is scope-owned and dies with the component.
//!
//! Visual: the on state takes the ATTENTION family, deliberately not violet
//! (violet is the bound/bus convention) and not green (green means
//! good/valid, never state). Attention is the honest reading — the output
//! is being overridden, which is a temporary abnormal condition someone
//! needs to remember to undo.

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use lpa_studio_core::{UiAction, UiOutputFace as UiOutputFaceData, UiProductKind};

use crate::app::node::produced_product_view::ProductPreview;
use crate::app::node::{NodeCardSection, map_view::MapViewOptions};

/// Renewal cadence while the pattern is on.
const RENEWAL_MS: u32 = 1000;

/// The pattern's device-side auto-expiry. Short on purpose: an abandoned
/// override should not outlive the tab that set it by more than a blink.
const PATTERN_TTL_MS: u32 = 2000;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn OutputFace(
    face: UiOutputFaceData,
    /// Mount with the toggle already on (stories render the overridden
    /// state without running a renewal loop).
    #[props(default = false)]
    pattern_initially_on: bool,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let mut on = use_signal(|| pattern_initially_on);
    // Retires the previous renewal loop when the toggle is flipped: a loop
    // runs only while it still owns the current epoch, so a fast off/on
    // cannot leave two loops renewing the same pattern.
    let mut epoch = use_signal(|| 0_u32);
    let wired = on_action.is_some();

    let preview = face.preview.clone();
    let endpoint = face.endpoint.clone();
    let toggle_face = face;

    rsx! {
        NodeCardSection { label: "test", first: true,
            div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-3 tw:px-4 tw:py-3",
                button {
                    class: pattern_button_class(on(), wired),
                    r#type: "button",
                    role: "switch",
                    aria_checked: "{on()}",
                    disabled: !wired,
                    title: "Drive every pixel white instead of the graph — the wiring check. Auto-clears a couple of seconds after this tab stops asking.",
                    onclick: move |event| {
                        event.stop_propagation();
                        let Some(handler) = on_action else {
                            return;
                        };
                        let next = !*on.peek();
                        let mine = *epoch.peek() + 1;
                        epoch.set(mine);
                        // Optimistic: the toggle follows the click, and a
                        // refusal (never-rendered output) arrives as the
                        // op layer's warning notice while the TTL puts the
                        // runtime back on its own.
                        on.set(next);
                        if !next {
                            handler.call(toggle_face.clear_action());
                            return;
                        }
                        let face = toggle_face.clone();
                        spawn(async move {
                            while *epoch.peek() == mine && *on.peek() {
                                handler.call(face.test_pattern_action(PATTERN_TTL_MS));
                                TimeoutFuture::new(RENEWAL_MS).await;
                            }
                        });
                    },
                    if on() { "Test pattern on" } else { "Test pattern" }
                }
                if let Some(endpoint) = endpoint {
                    span { class: "tw:min-w-0 tw:truncate tw:font-mono tw:text-[0.7rem] tw:text-dim-foreground",
                        "{endpoint}"
                    }
                }
            }
            if let Some(preview) = preview {
                ProductPreview {
                    kind: UiProductKind::Control,
                    preview: preview.preview.clone(),
                    tracking: preview.tracking,
                    frame: preview.frame,
                    focus_action: None,
                    on_action,
                    map_view: MapViewOptions::default(),
                }
            }
        }
    }
}

/// Toggle chrome. On takes the ATTENTION family: the output is overridden,
/// which is a temporary abnormal condition, not a fault (error/warning) and
/// not a good state (green). Violet stays reserved for bound/bus.
fn pattern_button_class(on: bool, wired: bool) -> String {
    let surface = if on {
        "tw:border-status-attention-border tw:bg-status-attention-bg tw:text-status-attention-foreground"
    } else {
        "tw:border-border-strong tw:bg-card-subtle tw:text-strong-foreground tw:hover:bg-card-muted"
    };
    let cursor = if wired {
        " tw:cursor-pointer"
    } else {
        " tw:opacity-60"
    };
    format!(
        "tw:inline-flex tw:flex-none tw:select-none tw:appearance-none tw:items-center \
         tw:rounded-sm tw:border tw:px-3 tw:py-1.5 tw:text-[0.8rem] tw:font-medium \
         tw:transition-colors tw:motion-reduce:transition-none {surface}{cursor}"
    )
}

#[cfg(test)]
mod tests {
    use super::{PATTERN_TTL_MS, RENEWAL_MS, pattern_button_class};

    #[test]
    fn the_pattern_outlives_a_missed_renewal_but_not_the_tab() {
        assert!(
            RENEWAL_MS < PATTERN_TTL_MS,
            "a renewal must land before the pattern expires"
        );
        assert!(
            PATTERN_TTL_MS <= 2 * RENEWAL_MS + RENEWAL_MS,
            "an abandoned override must clear within a blink, not a minute"
        );
    }

    #[test]
    fn on_reads_as_a_diagnostic_override_not_a_binding_or_a_blessing() {
        let on = pattern_button_class(true, true);
        assert!(on.contains("status-attention"));
        assert!(
            !on.contains("status-bound"),
            "violet is the binding/bus convention"
        );
        assert!(
            !on.contains("status-good"),
            "green means good/valid, never state"
        );
        assert!(pattern_button_class(false, true).contains("cursor-pointer"));
        assert!(!pattern_button_class(false, false).contains("cursor-pointer"));
    }
}
