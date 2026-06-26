//! Base icon-menu stories.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::base::{
    IconMenuButton, IconMenuTone, IconMenuVisualState, PopoverPlacement, StudioIcon, StudioIconName,
};

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
    let cases = [
        ("Below / Start", "below-start", "below", "start"),
        ("Below / Middle", "below-middle", "below", "middle"),
        ("Below / End", "below-end", "below", "end"),
        ("Above / Start", "above-start", "above", "start"),
        ("Above / Middle", "above-middle", "above", "middle"),
        ("Above / End", "above-end", "above", "end"),
    ];

    rsx! {
        section { class: "ux-attached-popover-story",
            for (title, meta, side, align) in cases {
                article { class: "ux-attached-popover-story-card ux-attached-popover-story-card-{meta}",
                    header { class: "ux-attached-popover-story-heading",
                        strong { "{title}" }
                        span { "{meta}" }
                    }
                    div { class: "ux-attached-popover-story-canvas ux-attached-popover-story-canvas-{meta}",
                        AttachedIconMenuStoryCase {
                            side,
                            align,
                        }
                    }
                }
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
fn AttachedIconMenuStoryCase(side: &'static str, align: &'static str) -> Element {
    let panel_corner_class = attached_story_panel_corner_class(side, align);
    let bridge_corner_class = attached_story_bridge_corner_class(align);
    let button_class = format!(
        "tw:inline-flex tw:h-6 tw:w-6 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-accent-border tw:bg-transparent tw:p-0 tw:text-accent ux-popover-chrome-accent ux-popover-trigger-attached ux-popover-trigger-attached-{side} ux-attached-popover-story-button ux-attached-popover-story-button-{side} ux-attached-popover-story-{align}"
    );
    let panel_class = format!(
        "tw:grid tw:w-[min(320px,calc(100vw-24px))] tw:gap-3 tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-3 tw:text-sm tw:text-muted-foreground tw:shadow-lg ux-popover-chrome-accent ux-popover-panel ux-attached-popover-panel ux-attached-popover-panel-{side} ux-attached-popover-story-panel ux-attached-popover-story-panel-{side} ux-attached-popover-story-{align} {panel_corner_class}"
    );
    let bridge_class = format!(
        "ux-popover-chrome-accent ux-popover-bridge ux-popover-bridge-{side} ux-attached-popover-story-bridge ux-attached-popover-story-bridge-{side} ux-attached-popover-story-{align} {bridge_corner_class}"
    );

    rsx! {
        button {
            class: "{button_class}",
            r#type: "button",
            aria_label: "Bound menu",
            title: "Bound menu",
            aria_expanded: "true",
            StudioIcon {
                name: StudioIconName::BoundValue,
                size: 14,
            }
        }
        aside {
            class: "{panel_class}",
            role: "dialog",
            div { class: "tw:grid tw:gap-1",
                span { class: "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-heading", "icon menu" }
                strong { class: "tw:text-sm tw:text-strong-foreground", "Bound" }
                p { class: "tw:m-0 tw:text-xs tw:text-muted-foreground", "Reusable icon-triggered menu chrome." }
            }
        }
        div {
            class: "{bridge_class}",
            aria_hidden: "true",
            span { class: "ux-popover-bridge-corner ux-popover-bridge-corner-left" }
            span { class: "ux-popover-bridge-corner ux-popover-bridge-corner-right" }
        }
    }
}

fn attached_story_panel_corner_class(side: &str, align: &str) -> &'static str {
    match (side, align) {
        ("below", "start") => "ux-attached-popover-panel-square-top-left",
        ("below", "end") => "ux-attached-popover-panel-square-top-right",
        ("above", "start") => "ux-attached-popover-panel-square-bottom-left",
        ("above", "end") => "ux-attached-popover-panel-square-bottom-right",
        _ => "",
    }
}

fn attached_story_bridge_corner_class(align: &str) -> &'static str {
    match align {
        "start" => "ux-popover-bridge-no-left-corner",
        "end" => "ux-popover-bridge-no-right-corner",
        _ => "",
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn IconMenuStoryButton(
    label: &'static str,
    tone: IconMenuTone,
    icon: StudioIconName,
    active: bool,
    #[props(default = PopoverPlacement::BottomEnd)] placement: PopoverPlacement,
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
            placement,
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
