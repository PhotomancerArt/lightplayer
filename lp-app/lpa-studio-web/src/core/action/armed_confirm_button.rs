//! The studio's two-tap confirm, for controls that are not a `UiAction`:
//! the first tap ARMS (red fill, "Remove", the quiet 4 s drain), the second
//! within the window acts, and blur or the window's end stands it down.
//!
//! [`ActionButton`](super::ActionButton) runs the same machine for actions
//! whose confirmation is marked inline; the arming itself lives here, in
//! [`use_armed_confirm`], so the two can never drift. The armed dress is
//! the same CSS (`.ux-armed-chip` / `.ux-armed`, style.css).
//!
//! # The icon-only variant
//!
//! "Who has access" puts a trash can on every row (the spike's gate: X
//! means close, a can means delete). At rest it is just the Trash2 icon; armed
//! it reads "Remove". Both labels share one grid cell, so the button is as
//! wide as "Remove" from the start and arming never moves the row.
//!
//! These buttons carry `ux-armed-local`: arming one dims its own row
//! (`.ux-armed-row`), not the device card the list sits in — a card-wide
//! red border is for removing the card's own device.

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;

use crate::base::{StudioIcon, StudioIconName};

/// How long a two-tap confirmation stays ARMED before it stands down on
/// its own. Long enough to read the changed label, short enough that a
/// forgotten armed button cannot ambush a later stray click.
///
/// Must match `--ux-armed-win` on `.ux-armed-chip` (style.css) — the drain
/// track animates over that var, and constants can't cross the Rust/CSS
/// boundary, so these two comments pin them together.
pub(crate) const ARMED_CONFIRM_WINDOW_MS: u32 = 4_000;

/// One control's armed state and its tap handler.
#[derive(Clone, Copy)]
pub(crate) struct ArmedConfirm {
    armed: Signal<bool>,
    generation: Signal<u64>,
}

impl ArmedConfirm {
    pub(crate) fn is_armed(&self) -> bool {
        (self.armed)()
    }

    /// A tap: arm when at rest (and start the window), or report `true` —
    /// act now — when already armed.
    pub(crate) fn tap(&mut self) -> bool {
        if (self.armed)() {
            self.armed.set(false);
            return true;
        }
        self.armed.set(true);
        let generation = (self.generation)() + 1;
        self.generation.set(generation);
        let mut armed = self.armed;
        let current = self.generation;
        spawn(async move {
            TimeoutFuture::new(ARMED_CONFIRM_WINDOW_MS).await;
            if current() == generation {
                armed.set(false);
            }
        });
        false
    }

    /// Stand down (blur).
    pub(crate) fn disarm(&mut self) {
        self.armed.set(false);
    }
}

/// The armed machine for one control. `preview` starts it armed (stories).
pub(crate) fn use_armed_confirm(preview: bool) -> ArmedConfirm {
    ArmedConfirm {
        armed: use_signal(|| preview),
        generation: use_signal(|| 0u64),
    }
}

/// A two-tap button that is not a `UiAction`: icon-only at rest (with
/// `label: None`) or a text verb.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ArmedConfirmButton(
    /// The resting verb; `None` draws only the icon (the trash can).
    #[props(default)]
    label: Option<String>,
    #[props(default)] icon: Option<StudioIconName>,
    /// What it reads while armed ("Remove", "Confirm reset").
    armed_label: String,
    /// The tooltip / accessible name at rest ("Remove friends").
    title: String,
    #[props(default)] disabled: bool,
    /// Stories only: start armed.
    #[props(default)]
    armed_preview: bool,
    on_confirm: EventHandler<()>,
) -> Element {
    let mut confirm = use_armed_confirm(armed_preview);
    let armed = confirm.is_armed();
    let icon_only = label.is_none();
    let base = if icon_only {
        ICON_BUTTON_CLASS
    } else {
        TEXT_BUTTON_CLASS
    };
    let class = if armed {
        format!("{base} ux-armed-chip ux-armed-local ux-armed")
    } else {
        format!("{base} ux-armed-chip ux-armed-local")
    };
    rsx! {
        button {
            class,
            r#type: "button",
            disabled,
            title: "{title}",
            aria_label: if armed { armed_label.clone() } else { title.clone() },
            onblur: move |_| confirm.disarm(),
            onclick: move |_| {
                if confirm.tap() {
                    on_confirm.call(());
                }
            },
            span { class: "ux-armed-labels",
                span { class: "ux-armed-label-rest",
                    if let Some(icon) = icon {
                        StudioIcon { name: icon, size: 15 }
                    }
                    if let Some(label) = label.clone() {
                        "{label}"
                    }
                }
                span { class: "ux-armed-label-armed", aria_hidden: "true", "{armed_label}" }
            }
        }
    }
}

/// The trash can: 30px tall, no chrome at rest, the error tint on hover.
const ICON_BUTTON_CLASS: &str = "tw:inline-flex tw:h-[30px] tw:min-w-[30px] tw:flex-none tw:cursor-pointer tw:appearance-none tw:items-center tw:justify-center tw:rounded tw:border tw:border-transparent tw:bg-transparent tw:px-1.5 tw:text-xs tw:font-bold tw:text-subtle-foreground tw:transition-colors tw:hover:border-status-error-border tw:hover:text-status-error-foreground tw:disabled:cursor-not-allowed tw:disabled:opacity-60 ux-focus-ring";

/// A text verb in the danger tone ("Reset account key…").
const TEXT_BUTTON_CLASS: &str = "tw:inline-flex tw:flex-none tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-1.5 tw:rounded tw:border tw:border-transparent tw:bg-transparent tw:px-1.5 tw:py-1 tw:text-xs tw:font-semibold tw:text-status-error-foreground tw:transition-colors tw:hover:border-status-error-border tw:disabled:cursor-not-allowed tw:disabled:opacity-60 ux-focus-ring";
