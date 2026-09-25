//! The small fields the Bluetooth access surfaces share: a password with
//! Show/Hide, a Play/Edit choice, a name, and an on/off switch.
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
    #[props(default = "Device password".to_string())] label: String,
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
        div { class: "tw:flex tw:flex-none tw:overflow-hidden tw:rounded tw:border tw:border-border-strong",
            role: "group",
            aria_label: "What this device password can do",
            button {
                class: segment_class(tier() == AccessTier::Play),
                r#type: "button",
                aria_pressed: "{tier() == AccessTier::Play}",
                title: "Play: the device's controls, nothing else",
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

/// A one-line text input (a password's name, this browser's name).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn NameField(
    value: Signal<String>,
    label: String,
    #[props(default)] autofocus: bool,
) -> Element {
    let mut value = value;
    rsx! {
        input {
            class: FIELD_CLASS,
            r#type: "text",
            aria_label: "{label}",
            placeholder: "{label}",
            autofocus,
            value: "{value}",
            oninput: move |event| value.set(event.value()),
        }
    }
}

/// An on/off switch (`role="switch"`): the Bluetooth row, "Anyone
/// nearby". `locked` draws it dimmed and inert; the row says why.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn Switch(
    on: bool,
    label: String,
    #[props(default)] locked: bool,
    on_toggle: EventHandler<bool>,
) -> Element {
    rsx! {
        button {
            class: "ux-switch ux-focus-ring",
            r#type: "button",
            role: "switch",
            aria_checked: "{on}",
            aria_label: "{label}",
            disabled: locked,
            onclick: move |_| on_toggle.call(!on),
        }
    }
}

pub(crate) const FIELD_CLASS: &str = "tw:min-w-0 tw:flex-1 tw:rounded tw:border tw:border-border-strong tw:bg-terminal tw:px-2.5 tw:py-2 tw:text-sm tw:text-strong-foreground";

pub(crate) const TEXT_BUTTON_CLASS: &str = "tw:flex-none tw:cursor-pointer tw:appearance-none tw:rounded tw:border-0 tw:bg-transparent tw:px-1.5 tw:py-1 tw:text-xs tw:font-semibold tw:text-muted-foreground tw:hover:text-strong-foreground tw:disabled:cursor-not-allowed tw:disabled:opacity-60 ux-focus-ring";

/// A quiet dotted-underline text link ("+ Add a password", "Rename").
pub(crate) const TEXT_LINK_CLASS: &str = "tw:flex-none tw:cursor-pointer tw:appearance-none tw:border-0 tw:bg-transparent tw:p-0 tw:text-xs tw:font-semibold tw:text-muted-foreground tw:underline tw:decoration-dotted tw:underline-offset-[3px] tw:hover:text-strong-foreground tw:disabled:cursor-not-allowed tw:disabled:opacity-60 ux-focus-ring";

/// The quiet help text under a field.
pub(crate) const HELP_CLASS: &str =
    "tw:m-0 tw:text-xs tw:leading-relaxed tw:text-subtle-foreground";

/// A small uppercase group heading inside a surface ("This browser").
pub(crate) const GROUP_HEAD_CLASS: &str =
    "tw:m-0 tw:text-[10px] tw:font-extrabold tw:uppercase tw:tracking-wider tw:text-dim-foreground";

fn segment_class(pressed: bool) -> &'static str {
    if pressed {
        "tw:cursor-pointer tw:appearance-none tw:border-0 tw:bg-white/10 tw:px-3 tw:py-1.5 tw:text-xs tw:font-bold tw:text-strong-foreground ux-focus-ring"
    } else {
        "tw:cursor-pointer tw:appearance-none tw:border-0 tw:bg-transparent tw:px-3 tw:py-1.5 tw:text-xs tw:font-semibold tw:text-muted-foreground tw:hover:text-strong-foreground ux-focus-ring"
    }
}

/// The PLAY tier chip on a play-only entry (edit entries show none: edit is
/// what a key normally is).
pub(crate) const PLAY_CHIP_CLASS: &str = "tw:flex-none tw:rounded-sm tw:border tw:border-status-neutral-border tw:px-1.5 tw:py-0.5 tw:font-mono tw:text-[9.5px] tw:font-bold tw:uppercase tw:leading-none tw:tracking-wider tw:text-status-neutral-foreground";
