use dioxus::prelude::*;
use lpa_studio_ux::{ActionEnablement, ActionPriority, AvailableAction, StudioAction};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ActionButton(
    action: AvailableAction<StudioAction>,
    running: bool,
    on_action: EventHandler<StudioAction>,
) -> Element {
    let command = action.command.clone();
    let disabled = running || !action.meta.enablement.is_enabled();
    let class = action_class(action.meta.priority);
    let disabled_reason = disabled_reason(&action.meta.enablement).map(ToString::to_string);
    let icon_class = action_icon_class(action.meta.icon.as_deref());
    let label = action.meta.label.clone();
    let summary = action.meta.summary.clone();

    rsx! {
        div { class: "ux-action-item",
            button {
                class,
                r#type: "button",
                disabled,
                title: "{summary}",
                onclick: move |_| on_action.call(command.clone()),
                if let Some(icon_class) = icon_class {
                    span { class: "{icon_class}", aria_hidden: "true" }
                }
                span { "{label}" }
            }
            if let Some(reason) = disabled_reason.as_ref() {
                p { class: "ux-disabled-reason", "{reason}" }
            }
        }
    }
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

fn action_icon_class(icon: Option<&str>) -> Option<&'static str> {
    match icon {
        Some("play") => Some("ux-action-icon ux-action-icon-play"),
        Some("usb") => Some("ux-action-icon ux-action-icon-usb"),
        Some("test-tube") => Some("ux-action-icon ux-action-icon-test"),
        _ => None,
    }
}
