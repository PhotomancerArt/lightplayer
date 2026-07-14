use dioxus::prelude::*;
use lpa_studio_core::{ActionEnablement, ActionPriority, UiAction};

use crate::base::{StudioIcon, action_icon_name};

/// How an action renders in its surrounding context. One action model
/// (label / icon / priority / destructive / confirmation from
/// [`ActionMeta`](lpa_studio_core::ActionMeta)), several visual homes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ActionButtonVariant {
    /// The standing action-strip button (priority-tiered chrome).
    #[default]
    Solid,
    /// A compact bordered chip for section headers and toolbars.
    Quiet,
    /// A full-width left-aligned row inside a menu popup.
    MenuItem,
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ActionButton(
    action: UiAction,
    running: bool,
    #[props(default)] variant: ActionButtonVariant,
    on_action: EventHandler<UiAction>,
) -> Element {
    let action_to_run = action.clone();
    let meta = action.meta().clone();
    let disabled = running || !meta.enablement.is_enabled();
    let class = action_class(variant, meta.priority, meta.destructive);
    let disabled_reason = disabled_reason(&meta.enablement).map(ToString::to_string);
    let icon = action_icon_name(meta.icon.as_deref());
    let confirmation = meta.confirmation.clone();
    let label = meta.label;
    let summary = meta.summary;
    let icon_px = match variant {
        ActionButtonVariant::Solid => 15,
        ActionButtonVariant::Quiet | ActionButtonVariant::MenuItem => 14,
    };

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-1",
            button {
                class,
                r#type: "button",
                disabled,
                title: "{summary}",
                onclick: move |_| {
                    if confirmation_confirmed(confirmation.as_ref()) {
                        on_action.call(action_to_run.clone());
                    }
                },
                if let Some(icon) = icon {
                    span { class: "tw:inline-flex tw:h-[15px] tw:w-[15px] tw:items-center tw:justify-center", aria_hidden: "true",
                        StudioIcon {
                            name: icon,
                            size: icon_px,
                        }
                    }
                }
                span { "{label}" }
            }
            if let Some(reason) = disabled_reason.as_ref() {
                p { class: "tw:m-0 tw:text-xs tw:leading-snug tw:text-dim-foreground", "{reason}" }
            }
        }
    }
}

fn confirmation_confirmed(confirmation: Option<&lpa_studio_core::ActionConfirmation>) -> bool {
    let Some(confirmation) = confirmation else {
        return true;
    };
    let message = format!("{}\n\n{}", confirmation.title, confirmation.message);
    web_sys::window()
        .and_then(|window| window.confirm_with_message(&message).ok())
        .unwrap_or(false)
}

fn action_class(
    variant: ActionButtonVariant,
    priority: ActionPriority,
    destructive: bool,
) -> &'static str {
    match variant {
        ActionButtonVariant::Solid => solid_class(priority),
        ActionButtonVariant::Quiet => quiet_class(destructive),
        ActionButtonVariant::MenuItem => menu_item_class(destructive),
    }
}

fn solid_class(priority: ActionPriority) -> &'static str {
    match priority {
        ActionPriority::Primary => {
            "tw:inline-flex tw:min-h-9 tw:max-w-full tw:items-center tw:justify-center tw:gap-2 tw:rounded-sm tw:border tw:border-accent-border tw:bg-accent tw:px-3 tw:text-sm tw:font-bold tw:leading-none tw:text-accent-foreground tw:break-words tw:hover:bg-accent-hover tw:disabled:cursor-not-allowed tw:disabled:opacity-60"
        }
        ActionPriority::Secondary => {
            "tw:inline-flex tw:min-h-9 tw:max-w-full tw:items-center tw:justify-center tw:gap-2 tw:rounded-sm tw:border tw:border-border-strong tw:bg-card-raised tw:px-3 tw:text-sm tw:font-bold tw:leading-none tw:text-soft-foreground tw:break-words tw:hover:bg-card-raised-strong tw:disabled:cursor-not-allowed tw:disabled:opacity-60"
        }
        ActionPriority::Tertiary => {
            "tw:inline-flex tw:min-h-9 tw:max-w-full tw:items-center tw:justify-center tw:gap-2 tw:rounded-sm tw:border tw:border-border-strong tw:bg-transparent tw:px-3 tw:text-sm tw:font-bold tw:leading-none tw:text-muted-foreground tw:break-words tw:hover:bg-card-muted tw:disabled:cursor-not-allowed tw:disabled:opacity-60"
        }
    }
}

/// The compact toolbar chip. All priorities share one quiet look — the
/// header is not a hierarchy; destructive still wears the error tint.
/// Shared with non-action toolbar controls (e.g. the import file-input
/// label) via [`quiet_action_class`].
fn quiet_class(destructive: bool) -> &'static str {
    if destructive {
        "tw:inline-flex tw:cursor-pointer tw:items-center tw:gap-1.5 tw:rounded tw:border tw:border-border tw:bg-transparent tw:px-2.5 tw:py-1 tw:text-xs tw:font-semibold tw:text-status-error-foreground tw:transition-colors tw:hover:border-status-error-border tw:disabled:cursor-not-allowed tw:disabled:opacity-60"
    } else {
        "tw:inline-flex tw:cursor-pointer tw:items-center tw:gap-1.5 tw:rounded tw:border tw:border-border tw:bg-transparent tw:px-2.5 tw:py-1 tw:text-xs tw:font-semibold tw:text-muted-foreground tw:transition-colors tw:hover:border-border-strong tw:hover:text-strong-foreground tw:disabled:cursor-not-allowed tw:disabled:opacity-60"
    }
}

/// One row of a menu popup. Shared with non-action rows (e.g. web-side
/// export) via [`menu_item_action_class`].
fn menu_item_class(destructive: bool) -> &'static str {
    if destructive {
        "tw:flex tw:w-full tw:cursor-pointer tw:items-center tw:gap-2 tw:rounded tw:px-2 tw:py-1.5 tw:text-left tw:text-sm tw:text-status-error-foreground tw:transition-colors tw:hover:bg-status-error-bg tw:disabled:cursor-not-allowed tw:disabled:opacity-60"
    } else {
        "tw:flex tw:w-full tw:cursor-pointer tw:items-center tw:gap-2 tw:rounded tw:px-2 tw:py-1.5 tw:text-left tw:text-sm tw:text-muted-foreground tw:transition-colors tw:hover:bg-white/5 tw:hover:text-strong-foreground tw:disabled:cursor-not-allowed tw:disabled:opacity-60"
    }
}

/// The quiet-chip classes, for toolbar controls that cannot be `UiAction`s
/// (file-input labels) but must read identically.
pub fn quiet_action_class() -> &'static str {
    quiet_class(false)
}

/// The menu-row classes, for popup rows that cannot be `UiAction`s
/// (web-side handlers like export) but must read identically.
pub fn menu_item_action_class() -> &'static str {
    menu_item_class(false)
}

fn disabled_reason(enablement: &ActionEnablement) -> Option<&str> {
    match enablement {
        ActionEnablement::Enabled => None,
        ActionEnablement::Disabled { reason } => Some(reason.as_str()),
    }
}
