//! "Unlocking your boards": this browser's name, the account's key and
//! passwords, and the remembered passwords (PD12), in a fold after the
//! boards sections.
//!
//! The fold starts closed: most people never open it (plugging a board in
//! puts this browser's key on it). Whether it is open is page-local view
//! state (PD4) — a `use_signal`, no action, not in the offer tree. What is
//! inside is [`AccessSettingsSection`], unchanged but for its title, wired
//! the way the Devices page wired it: the access context, the account's
//! access context, and this browser's platform and name.

use dioxus::prelude::*;
use lpa_studio_core::{UiDeviceSettingsView, UiHomeSection};

use super::page_section::TITLE_CLASS;
use crate::app::home::access_settings_section::AccessSettingsSection;
use crate::app::home::access_ui_context::use_access_ui;
use crate::app::home::browser_identity::{detect_browser, detect_platform};
use crate::base::{StudioIcon, StudioIconName};
use crate::cloud::account_access::{AccountAccessState, use_account_access_ui};

/// The fold. It draws nothing where there are no settings to show (a page
/// with no access context, and no story pin).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn KeysFold(
    /// Stories only: the settings the section shows (the app reads them
    /// from its access context).
    #[props(default)]
    settings: Option<UiDeviceSettingsView>,
    /// Stories only: mount the fold open.
    #[props(default)]
    initially_open: bool,
) -> Element {
    let access_ui = use_access_ui();
    let account_ui = use_account_access_ui();
    let mut open = use_signal(|| initially_open);
    let settings = settings.or_else(|| access_ui.map(|ui| ui.device_settings.read().clone()));
    let Some(settings) = settings else {
        return rsx! {};
    };
    let title = UiHomeSection::UnlockingYourBoards.label();
    let is_open = open();

    rsx! {
        section { class: "tw:grid tw:min-w-0 tw:content-start tw:gap-3",
            button {
                class: TOGGLE_CLASS,
                r#type: "button",
                "aria-expanded": "{is_open}",
                onclick: move |_| open.set(!is_open),
                span { class: "tw:inline-flex tw:text-subtle-foreground", aria_hidden: "true",
                    StudioIcon {
                        name: if is_open { StudioIconName::Expanded } else { StudioIconName::Collapsed },
                        size: 13,
                    }
                }
                h2 { class: TITLE_CLASS, "{title}" }
                span { class: "tw:h-px tw:flex-1 tw:bg-border-muted", aria_hidden: "true" }
            }
            if is_open {
                AccessSettingsSection {
                    titled: false,
                    settings,
                    account: account_ui
                        .map(|ui| ui.state.read().clone())
                        .unwrap_or(AccountAccessState::SignedOut),
                    platform: detect_platform(),
                    browser: detect_browser().to_string(),
                    on_access: move |command| {
                        if let Some(ui) = access_ui {
                            ui.on_access.call(command);
                        }
                    },
                    on_set_password: move |change| {
                        if let Some(ui) = account_ui {
                            ui.set_password.call(change);
                        }
                    },
                    on_reset_key: move |_| {
                        if let Some(ui) = account_ui {
                            ui.reset_key.call(());
                        }
                    },
                }
            }
        }
    }
}

/// The toggle: the section's header row (chevron, title, the hairline),
/// a plain button so the whole row opens and shuts the fold.
const TOGGLE_CLASS: &str = "tw:flex tw:w-full tw:cursor-pointer tw:items-center tw:gap-2 tw:border-0 tw:bg-transparent tw:p-0 tw:text-left ux-focus-ring";
