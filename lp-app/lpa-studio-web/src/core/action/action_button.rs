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
        div { class: "ux-action-item",
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
                    span { class: "ux-action-icon", aria_hidden: "true",
                        StudioIcon {
                            name: icon,
                            size: 15,
                        }
                    }
                }
                span { "{label}" }
            }
            if let Some(reason) = disabled_reason.as_ref() {
                p { class: "ux-disabled-reason", "{reason}" }
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
        ActionPriority::Primary => "ux-action ux-action-primary",
        ActionPriority::Secondary => "ux-action ux-action-secondary",
        ActionPriority::Tertiary => "ux-action ux-action-tertiary",
    }
}

fn disabled_reason(enablement: &ActionEnablement) -> Option<&str> {
    match enablement {
        ActionEnablement::Enabled => None,
        ActionEnablement::Disabled { reason } => Some(reason.as_str()),
    }
}
