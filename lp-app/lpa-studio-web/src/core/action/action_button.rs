use dioxus::prelude::*;
use lpa_studio_core::{ActionEnablement, ActionPriority, UiAction};

use crate::base::{StudioIcon, action_icon_name};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ActionButton(action: UiAction, running: bool, on_action: EventHandler<UiAction>) -> Element {
    let action_to_run = action.clone();
    let meta = action.meta().clone();
    let disabled = running || !meta.enablement.is_enabled();
    let class = action_class(meta.priority);
    let disabled_reason = disabled_reason(&meta.enablement).map(ToString::to_string);
    let icon = action_icon_name(meta.icon.as_deref());
    let confirmation = meta.confirmation.clone();
    let label = meta.label;
    let summary = meta.summary;

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
                            size: 15,
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

fn action_class(priority: ActionPriority) -> &'static str {
    match priority {
        ActionPriority::Primary => {
            "tw:inline-flex tw:min-h-9 tw:max-w-full tw:items-center tw:justify-center tw:gap-2 tw:rounded-sm tw:border tw:border-accent-border tw:bg-accent tw:px-3 tw:text-sm tw:font-bold tw:leading-none tw:text-accent-foreground tw:break-words tw:hover:bg-accent-hover tw:disabled:cursor-not-allowed tw:disabled:opacity-60"
        }
        ActionPriority::Secondary => {
            "tw:inline-flex tw:min-h-9 tw:max-w-full tw:items-center tw:justify-center tw:gap-2 tw:rounded-sm tw:border tw:border-border-strong tw:bg-card-raised tw:px-3 tw:text-sm tw:font-bold tw:leading-none tw:text-soft-foreground tw:break-words tw:hover:bg-card-raised-strong tw:disabled:cursor-not-allowed tw:disabled:opacity-60"
        }
        ActionPriority::Tertiary => {
            "tw:inline-flex tw:min-h-9 tw:max-w-full tw:items-center tw:justify-center tw:gap-2 tw:rounded-sm tw:border tw:border-transparent tw:bg-transparent tw:px-3 tw:text-sm tw:font-bold tw:leading-none tw:text-muted-foreground tw:break-words tw:hover:border-border-strong tw:hover:bg-card-muted tw:disabled:cursor-not-allowed tw:disabled:opacity-60"
        }
    }
}

fn disabled_reason(enablement: &ActionEnablement) -> Option<&str> {
    match enablement {
        ActionEnablement::Enabled => None,
        ActionEnablement::Disabled { reason } => Some(reason.as_str()),
    }
}
