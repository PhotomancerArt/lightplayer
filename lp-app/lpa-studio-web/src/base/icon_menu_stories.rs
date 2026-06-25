//! Base icon-menu stories.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::base::{IconMenuButton, IconMenuTone, StudioIconName};

#[story(description = "Standard icon-triggered menus used by dense Studio controls.")]
fn tones() -> Element {
    rsx! {
        div { class: "tw:flex tw:flex-wrap tw:items-center tw:gap-3",
            IconMenuStoryButton {
                label: "Neutral",
                tone: IconMenuTone::Neutral,
                icon: StudioIconName::AssignedValue,
                active: false,
            }
            IconMenuStoryButton {
                label: "Bound",
                tone: IconMenuTone::Accent,
                icon: StudioIconName::BoundValue,
                active: true,
            }
            IconMenuStoryButton {
                label: "Running",
                tone: IconMenuTone::Good,
                icon: StudioIconName::StatusRunning,
                active: true,
            }
            IconMenuStoryButton {
                label: "Warning",
                tone: IconMenuTone::Warning,
                icon: StudioIconName::StepAttention,
                active: true,
            }
            IconMenuStoryButton {
                label: "Error",
                tone: IconMenuTone::Error,
                icon: StudioIconName::StatusError,
                active: true,
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn IconMenuStoryButton(
    label: &'static str,
    tone: IconMenuTone,
    icon: StudioIconName,
    active: bool,
) -> Element {
    rsx! {
        IconMenuButton {
            icon,
            label: format!("{label} menu"),
            title: format!("{label} menu"),
            tone,
            active,
            div { class: "tw:grid tw:gap-1",
                span { class: "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-heading", "icon menu" }
                strong { class: "tw:text-sm tw:text-strong-foreground", "{label}" }
                p { class: "tw:m-0 tw:text-xs tw:text-muted-foreground", "Reusable icon-triggered menu chrome." }
            }
        }
    }
}
