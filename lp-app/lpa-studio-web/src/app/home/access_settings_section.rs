//! The Devices page's access settings (spike §6): this browser's name on
//! your devices, your account's key and its optional device passwords, and
//! the passwords this browser remembers.
//!
//! Most people never open this: plugging a device in by USB puts this
//! browser's key on it, and signing in puts your account's key there too,
//! so any browser you sign in on unlocks it. What is here is for the few
//! who want more:
//!
//! - **This browser** — the name devices list it under, renameable (a
//!   device re-labels it the next time it is plugged in).
//! - **Your account** — the account key (Reset rotates it), and two
//!   optional account passwords, play and edit, none by default: device
//!   passwords to tell friends, added next to the account key on every
//!   device you set up or plug in.
//! - **Remembered device passwords** — the ones friends shared with this
//!   browser, with a way to forget them. Local to this browser.

use dioxus::prelude::*;
use lpa_studio_core::{AccessCommand, UiDeviceSettingsView};
use lpc_cloud_api::AccountPasswordTier;

use super::access_entry_row::ICON_TILE_CLASS;
use super::access_fields::{
    GROUP_HEAD_CLASS, HELP_CLASS, NameField, PasswordField, TEXT_LINK_CLASS,
};
use super::browser_identity::BrowserPlatform;
use super::section_title_class;
use crate::base::{StudioIcon, StudioIconName};
use crate::cloud::account_access::AccountAccessState;
use crate::core::{ArmedConfirmButton, outline_action_class, quiet_action_class};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn AccessSettingsSection(
    settings: UiDeviceSettingsView,
    account: AccountAccessState,
    platform: BrowserPlatform,
    /// "Chrome", "Bluefy".
    browser: String,
    on_access: EventHandler<AccessCommand>,
    on_set_password: EventHandler<(AccountPasswordTier, Option<String>)>,
    on_reset_key: EventHandler<()>,
    /// Stories only: show the account passwords' text.
    #[props(default)]
    passwords_shown: bool,
) -> Element {
    let remembered = settings.remembered_passwords;
    rsx! {
        section { class: "tw:grid tw:max-w-md tw:gap-4",
            h2 { class: section_title_class(), "Unlocking your devices" }
            div { class: "tw:grid tw:gap-1.5",
                p { class: GROUP_HEAD_CLASS, "This browser" }
                ThisBrowserRow {
                    name: settings.browser_name.clone().unwrap_or_else(|| "This browser".to_string()),
                    platform,
                    browser,
                    on_access,
                }
            }
            div { class: "tw:grid tw:gap-1.5",
                p { class: GROUP_HEAD_CLASS, "Your account" }
                AccountGroup { account, on_set_password, on_reset_key, passwords_shown }
            }
            div { class: "tw:grid tw:gap-1.5",
                p { class: GROUP_HEAD_CLASS, "Remembered device passwords" }
                div { class: "tw:flex tw:min-w-0 tw:flex-wrap tw:items-center tw:gap-x-3 tw:gap-y-1",
                    span { class: "tw:text-xs tw:text-subtle-foreground", "{remembered_line(remembered)}" }
                    if remembered > 0 {
                        button {
                            class: TEXT_LINK_CLASS,
                            r#type: "button",
                            onclick: move |_| on_access.call(AccessCommand::ForgetRememberedPasswords),
                            "Forget them"
                        }
                    }
                }
            }
        }
    }
}

/// This browser's name, with Rename in place.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ThisBrowserRow(
    name: String,
    platform: BrowserPlatform,
    browser: String,
    on_access: EventHandler<AccessCommand>,
) -> Element {
    let mut renaming = use_signal(|| false);
    let mut draft = use_signal(String::new);
    let icon = if platform.is_phone() {
        StudioIconName::AccessPhone
    } else {
        StudioIconName::AccessLaptop
    };
    let platform_name = platform.name();
    let rename_from = name.clone();
    rsx! {
        div { class: OPT_CLASS,
            span { class: "{ICON_TILE_CLASS} tw:border-status-neutral-border tw:bg-status-neutral-bg tw:text-status-neutral-foreground",
                StudioIcon { name: icon, size: 15 }
            }
            if renaming() {
                form {
                    class: "tw:flex tw:min-w-0 tw:flex-1 tw:flex-wrap tw:items-center tw:gap-2",
                    onsubmit: move |event| {
                        event.prevent_default();
                        let name = draft.read().trim().to_string();
                        if !name.is_empty() {
                            on_access.call(AccessCommand::RenameBrowser(name));
                        }
                        renaming.set(false);
                    },
                    NameField { value: draft, label: "This browser's name".to_string(), autofocus: true }
                    button { class: outline_action_class(false), r#type: "submit", "Save" }
                    button {
                        class: quiet_action_class(),
                        r#type: "button",
                        onclick: move |_| renaming.set(false),
                        "Cancel"
                    }
                }
            } else {
                span { class: "tw:grid tw:min-w-0 tw:flex-1 tw:gap-0.5",
                    span { class: "tw:truncate tw:text-[13px] tw:font-bold tw:text-strong-foreground", "{name}" }
                    span { class: HELP_CLASS, "How your devices list this browser. {platform_name} · {browser}." }
                }
                button {
                    class: TEXT_LINK_CLASS,
                    r#type: "button",
                    onclick: move |_| {
                        draft.set(rename_from.clone());
                        renaming.set(true);
                    },
                    "Rename"
                }
            }
        }
    }
}

/// The account's key and passwords, or the invitation to sign in.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn AccountGroup(
    account: AccountAccessState,
    on_set_password: EventHandler<(AccountPasswordTier, Option<String>)>,
    on_reset_key: EventHandler<()>,
    passwords_shown: bool,
) -> Element {
    let (name, info, note) = match &account {
        AccountAccessState::SignedOut => {
            return rsx! {
                div { class: OPT_CLASS,
                    span { class: "{ICON_TILE_CLASS} tw:rounded-full tw:border-border-strong tw:text-xs tw:font-extrabold tw:text-dim-foreground", "?" }
                    span { class: "tw:grid tw:min-w-0 tw:flex-1 tw:gap-0.5",
                        span { class: "tw:text-[13px] tw:font-bold tw:text-strong-foreground", "Not signed in" }
                        span { class: HELP_CLASS, "Sign in and your devices unlock from any browser you sign in on." }
                    }
                    a { class: "{quiet_action_class()} tw:no-underline", href: "/account", "Sign in" }
                }
            };
        }
        AccountAccessState::Loading { name } => (name.clone(), None, Some("Reading your account…")),
        AccountAccessState::Unavailable { name } => (
            name.clone(),
            None,
            Some(
                "Can't reach your account right now. Your devices still unlock with the key this browser remembers.",
            ),
        ),
        AccountAccessState::Ready { name, info } => (name.clone(), Some(info.clone()), None),
    };
    let initial = name
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    rsx! {
        div { class: OPT_CLASS,
            span { class: "{ICON_TILE_CLASS} tw:rounded-full tw:border-status-good-border tw:bg-status-good-bg tw:text-xs tw:font-extrabold tw:text-status-good-foreground",
                "{initial}"
            }
            span { class: "tw:grid tw:min-w-0 tw:flex-1 tw:gap-0.5",
                span { class: "tw:truncate tw:text-[13px] tw:font-bold tw:text-strong-foreground", "{name}'s account key" }
                span { class: HELP_CLASS, "Any browser signed in as you unlocks your devices." }
            }
        }
        if let Some(note) = note {
            p { class: HELP_CLASS, "{note}" }
        }
        if let Some(info) = info {
            div { class: "tw:flex",
                ArmedConfirmButton {
                    label: Some("Reset account key…".to_string()),
                    armed_label: "Confirm reset".to_string(),
                    title: "Make a new account key. Devices drop the old one the next time you plug them in.".to_string(),
                    on_confirm: move |_| on_reset_key.call(()),
                }
            }
            p { class: "{GROUP_HEAD_CLASS} tw:mt-2", "Account passwords · optional" }
            div { class: "tw:grid",
                AccountPasswordRow {
                    tier: AccountPasswordTier::Play,
                    value: info.play_password.clone(),
                    initially_shown: passwords_shown,
                    on_set_password,
                }
                AccountPasswordRow {
                    tier: AccountPasswordTier::Edit,
                    value: info.edit_password.clone(),
                    initially_shown: passwords_shown,
                    on_set_password,
                }
            }
            p { class: HELP_CLASS,
                "Added to every device you set up or plug in, next to your account key. For friends who don't use LightPlayer — tell them the play one."
            }
        }
    }
}

/// One optional account password: "Not set" and Set, or the password
/// (masked) with Show, Change and its trash can.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn AccountPasswordRow(
    tier: AccountPasswordTier,
    value: Option<String>,
    initially_shown: bool,
    on_set_password: EventHandler<(AccountPasswordTier, Option<String>)>,
) -> Element {
    let mut editing = use_signal(|| false);
    let mut shown = use_signal(|| initially_shown);
    let draft = use_signal(String::new);
    let title = match tier {
        AccountPasswordTier::Play => "Play password",
        AccountPasswordTier::Edit => "Edit password",
    };
    let set = value
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    rsx! {
        div { class: "ux-armed-row tw:flex tw:min-w-0 tw:flex-wrap tw:items-center tw:gap-2.5 tw:border-t tw:border-border-muted tw:py-2 tw:first:border-t-0",
            span { class: "ux-armed-row-dim {ICON_TILE_CLASS} tw:border-status-warning-border tw:bg-status-warning-bg tw:text-status-warning-foreground",
                StudioIcon { name: StudioIconName::AccessKey, size: 15 }
            }
            span { class: "ux-armed-row-dim tw:grid tw:min-w-0 tw:flex-1 tw:gap-px",
                span { class: "tw:text-[13px] tw:font-bold tw:text-strong-foreground", "{title}" }
                span { class: "ux-armed-row-hide tw:truncate tw:text-xs tw:text-subtle-foreground",
                    match (&set, shown()) {
                        (None, _) => rsx! { "Not set" },
                        (Some(value), true) => rsx! { span { class: "tw:select-all tw:font-mono", "{value}" } },
                        (Some(_), false) => rsx! { span { class: "tw:font-mono", "••••••••" } },
                    }
                }
            }
            if !editing() {
                if set.is_some() {
                    button {
                        class: TEXT_LINK_CLASS,
                        r#type: "button",
                        onclick: move |_| {
                            let was = shown();
                            shown.set(!was);
                        },
                        if shown() { "Hide" } else { "Show" }
                    }
                    button {
                        class: TEXT_LINK_CLASS,
                        r#type: "button",
                        onclick: move |_| editing.set(true),
                        "Change"
                    }
                    ArmedConfirmButton {
                        icon: Some(StudioIconName::Remove),
                        armed_label: "Remove".to_string(),
                        title: format!("Remove the {}", title.to_lowercase()),
                        on_confirm: move |_| on_set_password.call((tier, None)),
                    }
                } else {
                    button {
                        class: quiet_action_class(),
                        r#type: "button",
                        onclick: move |_| editing.set(true),
                        "Set"
                    }
                }
            }
            if editing() {
                form {
                    class: "tw:grid tw:w-full tw:gap-2",
                    onsubmit: move |event| {
                        event.prevent_default();
                        let mut draft = draft;
                        let typed = draft.read().clone();
                        if !typed.is_empty() {
                            on_set_password.call((tier, Some(typed)));
                        }
                        draft.set(String::new());
                        editing.set(false);
                    },
                    PasswordField { value: draft, label: title.to_string(), initially_shown: true, autofocus: true }
                    div { class: "tw:flex tw:justify-end tw:gap-2",
                        button {
                            class: quiet_action_class(),
                            r#type: "button",
                            onclick: move |_| editing.set(false),
                            "Cancel"
                        }
                        button { class: outline_action_class(false), r#type: "submit", "Save" }
                    }
                }
            }
        }
    }
}

/// A settings row: a bordered tile with its icon.
const OPT_CLASS: &str = "tw:flex tw:min-w-0 tw:items-center tw:gap-2.5 tw:rounded-md tw:border tw:border-border tw:bg-card-subtle tw:px-3 tw:py-2.5";

fn remembered_line(count: usize) -> String {
    match count {
        0 => "None yet. Passwords friends share with you land here.".to_string(),
        1 => "1 password friends shared with you.".to_string(),
        n => format!("{n} passwords friends shared with you."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_remembered_line_counts_in_words() {
        assert!(remembered_line(0).starts_with("None"));
        assert_eq!(remembered_line(1), "1 password friends shared with you.");
        assert_eq!(remembered_line(2), "2 passwords friends shared with you.");
    }
}
