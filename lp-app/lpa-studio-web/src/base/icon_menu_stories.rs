//! Base icon-menu stories.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::base::{IconMenuButton, IconMenuTone, IconMenuVisualState, StudioIconName};

#[story(description = "Standard icon-triggered menus used by dense Studio controls.")]
fn tones() -> Element {
    rsx! {
        div { class: "tw:flex tw:min-h-56 tw:flex-wrap tw:items-start tw:gap-3 tw:pt-8",
            IconMenuStoryButton {
                label: "Quiet",
                tone: IconMenuTone::Quiet,
                icon: StudioIconName::Info,
                active: true,
            }
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

#[story(description = "An open icon menu with the popup positioned near its trigger.")]
fn open_menu() -> Element {
    rsx! {
        div { class: "tw:flex tw:min-h-72 tw:justify-end tw:pr-24 tw:pt-16",
            IconMenuStoryButton {
                label: "Bound",
                tone: IconMenuTone::Accent,
                icon: StudioIconName::BoundValue,
                active: true,
                initially_open: true,
            }
        }
    }
}

#[story(description = "Forced trigger states for the low-level icon menu primitive.")]
fn trigger_states() -> Element {
    let states = [
        ("Rest", IconMenuVisualState::Rest),
        ("Hover", IconMenuVisualState::Hover),
        ("Open", IconMenuVisualState::Open),
    ];
    let tones = [
        ("Quiet", IconMenuTone::Quiet, StudioIconName::Info, true),
        (
            "Neutral",
            IconMenuTone::Neutral,
            StudioIconName::AssignedValue,
            false,
        ),
        (
            "Bound",
            IconMenuTone::Accent,
            StudioIconName::BoundValue,
            true,
        ),
        (
            "Warning",
            IconMenuTone::Warning,
            StudioIconName::StepAttention,
            true,
        ),
        (
            "Error",
            IconMenuTone::Error,
            StudioIconName::StatusError,
            true,
        ),
    ];

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-2",
            div { class: "tw:grid tw:grid-cols-[72px_repeat(3,44px)] tw:items-center tw:gap-2",
                span { class: "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-subtle-foreground", "Tone" }
                for (state_label, _) in states {
                    span { class: "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-subtle-foreground", "{state_label}" }
                }
            }
            for (tone_label, tone, icon, active) in tones {
                div { class: "tw:grid tw:grid-cols-[72px_repeat(3,44px)] tw:items-center tw:gap-2",
                    span { class: "tw:text-xs tw:font-bold tw:text-strong-foreground", "{tone_label}" }
                    for (_, state) in states {
                        IconMenuStoryButton {
                            label: tone_label,
                            tone,
                            icon,
                            active,
                            visual_state: state,
                        }
                    }
                }
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
    #[props(default = IconMenuVisualState::Rest)] visual_state: IconMenuVisualState,
    #[props(default = false)] initially_open: bool,
) -> Element {
    rsx! {
        IconMenuButton {
            icon,
            label: format!("{label} menu"),
            title: format!("{label} menu"),
            tone,
            active,
            visual_state,
            initially_open,
            div { class: "tw:grid tw:gap-1",
                span { class: "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-heading", "icon menu" }
                strong { class: "tw:text-sm tw:text-strong-foreground", "{label}" }
                p { class: "tw:m-0 tw:text-xs tw:text-muted-foreground", "Reusable icon-triggered menu chrome." }
            }
        }
    }
}
