//! The small fields the Bluetooth access surfaces share: a password with
//! Show/Hide, a Play/Edit choice, and a name.
//!
//! Plain text buttons, no icons: at 375 px a word says what a glyph only
//! hints at, and these sit in forms people fill once.

use dioxus::prelude::*;
use lpa_studio_core::AccessTier;

/// A password input with its own Show/Hide.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn PasswordField(
    value: Signal<String>,
    #[props(default = "Password".to_string())] label: String,
    /// Stories: start shown.
    #[props(default)]
    initially_shown: bool,
    #[props(default)] autofocus: bool,
) -> Element {
    let mut value = value;
    let mut shown = use_signal(|| initially_shown);
    rsx! {
        div { class: "tw:flex tw:min-w-0 tw:gap-1.5",
            input {
                class: FIELD_CLASS,
                r#type: if shown() { "text" } else { "password" },
                aria_label: "{label}",
                placeholder: "{label}",
                autocomplete: "off",
                autofocus,
                value: "{value}",
                oninput: move |event| value.set(event.value()),
            }
            button {
                class: TEXT_BUTTON_CLASS,
                r#type: "button",
                onclick: move |_| {
                    let was = shown();
                    shown.set(!was);
                },
                if shown() { "Hide" } else { "Show" }
            }
        }
    }
}

/// Play or Edit, as two pressed-or-not buttons.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn TierChoice(tier: Signal<AccessTier>) -> Element {
    let mut tier = tier;
    let pick = |candidate: AccessTier| move |_| tier.set(candidate);
    rsx! {
        div { class: "tw:flex tw:flex-none tw:overflow-hidden tw:rounded tw:border tw:border-border",
            role: "group",
            aria_label: "What this password can do",
            button {
                class: segment_class(tier() == AccessTier::Play),
                r#type: "button",
                aria_pressed: "{tier() == AccessTier::Play}",
                title: "Play: the piece's controls, nothing else",
                onclick: pick(AccessTier::Play),
                "Play"
            }
            button {
                class: segment_class(tier() == AccessTier::Edit),
                r#type: "button",
                aria_pressed: "{tier() == AccessTier::Edit}",
                title: "Edit: everything USB can do except firmware",
                onclick: pick(AccessTier::Edit),
                "Edit"
            }
        }
    }
}

/// A one-line text input (a password's name).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn NameField(value: Signal<String>, label: String) -> Element {
    let mut value = value;
    rsx! {
        input {
            class: FIELD_CLASS,
            r#type: "text",
            aria_label: "{label}",
            placeholder: "{label}",
            value: "{value}",
            oninput: move |event| value.set(event.value()),
        }
    }
}

/// One password's row: name, what it can do, and its Remove.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn SecretRow(
    label: String,
    tier: AccessTier,
    #[props(default)] disabled: bool,
    on_remove: EventHandler<()>,
) -> Element {
    rsx! {
        li { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2 tw:py-1",
            span { class: "tw:min-w-0 tw:flex-1 tw:truncate tw:text-sm tw:text-strong-foreground", title: "{label}",
                "{label}"
            }
            span { class: tier_badge_class(tier), "{lpa_studio_core::tier_word(tier)}" }
            button {
                class: TEXT_BUTTON_CLASS,
                r#type: "button",
                disabled,
                onclick: move |_| on_remove.call(()),
                "Remove"
            }
        }
    }
}

pub(crate) const FIELD_CLASS: &str = "tw:min-w-0 tw:flex-1 tw:rounded tw:border tw:border-border tw:bg-terminal tw:px-2 tw:py-1.5 tw:text-sm tw:text-strong-foreground";

pub(crate) const TEXT_BUTTON_CLASS: &str = "tw:flex-none tw:cursor-pointer tw:appearance-none tw:rounded tw:border-0 tw:bg-transparent tw:px-1.5 tw:py-1 tw:text-xs tw:font-semibold tw:text-muted-foreground tw:hover:text-strong-foreground tw:disabled:cursor-not-allowed tw:disabled:opacity-60 ux-focus-ring";

/// The quiet help text under a field.
pub(crate) const HELP_CLASS: &str =
    "tw:m-0 tw:text-xs tw:leading-relaxed tw:text-subtle-foreground";

fn segment_class(pressed: bool) -> &'static str {
    if pressed {
        "tw:cursor-pointer tw:appearance-none tw:border-0 tw:bg-white/10 tw:px-2.5 tw:py-1 tw:text-xs tw:font-bold tw:text-strong-foreground ux-focus-ring"
    } else {
        "tw:cursor-pointer tw:appearance-none tw:border-0 tw:bg-transparent tw:px-2.5 tw:py-1 tw:text-xs tw:font-semibold tw:text-muted-foreground tw:hover:text-strong-foreground ux-focus-ring"
    }
}

/// A tier's small badge: edit reads stronger than play.
pub(crate) fn tier_badge_class(tier: AccessTier) -> &'static str {
    match tier {
        AccessTier::Edit => {
            "tw:flex-none tw:rounded-sm tw:border tw:border-border-strong tw:px-1.5 tw:text-[10px] tw:font-bold tw:uppercase tw:tracking-wide tw:text-strong-foreground"
        }
        AccessTier::Play => {
            "tw:flex-none tw:rounded-sm tw:border tw:border-border tw:px-1.5 tw:text-[10px] tw:font-semibold tw:uppercase tw:tracking-wide tw:text-muted-foreground"
        }
    }
}
