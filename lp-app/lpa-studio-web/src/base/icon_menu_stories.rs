//! Base icon-menu stories.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::base::outline::{OutlineRect, merged_outline_path};
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

/// A static, measurement-free rendering of the settled open state: hardcoded
/// trigger/panel rects run through the same [`merged_outline_path`] the live
/// [`crate::base::PopoverButton`] uses, so this story is an honest preview of
/// the SVG merged-outline chrome across every side/align pairing.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn AttachedIconMenuStoryCase(side: &'static str, align: &'static str) -> Element {
    const STAGE_W: f64 = 300.0;
    const STAGE_H: f64 = 150.0;
    const TRIGGER: f64 = 24.0;
    const PANEL_H: f64 = 120.0;
    const EDGE: f64 = 2.0;
    const RADIUS: f64 = 8.0;
    const INFLATE: f64 = 3.0;

    // Start/end variants align the trigger's VISIBLE (inflated) edge to the
    // panel edge, matching the live component's alignment — a raw-edge
    // alignment would leave an INFLATE-wide shelf in the outline.
    let trigger_x = match align {
        "start" => EDGE + INFLATE,
        "end" => STAGE_W - EDGE - TRIGGER - INFLATE,
        _ => (STAGE_W - TRIGGER) / 2.0,
    };
    let (trigger_y, panel_y) = if side == "below" {
        (EDGE + INFLATE, EDGE + INFLATE + TRIGGER - 1.0)
    } else {
        let trigger_y = STAGE_H - EDGE - INFLATE - TRIGGER;
        (trigger_y, trigger_y + 1.0 - PANEL_H)
    };
    let trigger_rect = OutlineRect {
        x: trigger_x,
        y: trigger_y,
        w: TRIGGER,
        h: TRIGGER,
    };
    let panel_rect = OutlineRect {
        x: EDGE,
        y: panel_y,
        w: STAGE_W - 2.0 * EDGE,
        h: PANEL_H,
    };
    // Settled open state: the trigger outline carries its full inflate.
    let path = merged_outline_path(&[trigger_rect.inflate(INFLATE), panel_rect], RADIUS, 1.0);
    let grad_id = format!("ux-story-popover-grad-{side}-{align}");
    let trigger_fill = "var(--ux-popover-trigger-fill-top, var(--studio-color-surface-raised))";
    let panel_fill = "var(--ux-popover-panel-fill-away, var(--studio-color-surface-raised))";
    let (grad_stop_near, grad_stop_far) = if side == "below" {
        (trigger_fill, panel_fill)
    } else {
        (panel_fill, trigger_fill)
    };
    let trigger_style = format!(
        "position: absolute; left: {trigger_x}px; top: {trigger_y}px; width: {TRIGGER}px; \
         height: {TRIGGER}px; display: grid; place-items: center; padding: 0; border: 0; \
         background: transparent; color: var(--ux-popover-icon-color, var(--studio-color-text-strong)); \
         cursor: pointer;"
    );
    let panel_style = format!(
        "position: absolute; left: {}px; top: {}px; width: {}px; height: {}px; padding: 12px;",
        panel_rect.x, panel_rect.y, panel_rect.w, panel_rect.h
    );

    rsx! {
        div {
            class: "ux-popover-chrome-accent",
            style: "position: relative; width: {STAGE_W}px; height: {STAGE_H}px;",
            svg {
                style: "position: absolute; inset: 0; width: 100%; height: 100%; overflow: visible; pointer-events: none;",
                "aria-hidden": "true",
                defs {
                    linearGradient {
                        id: "{grad_id}",
                        x1: "0",
                        y1: "0",
                        x2: "0",
                        y2: "1",
                        stop { offset: "0", style: "stop-color: {grad_stop_near};" }
                        stop { offset: "1", style: "stop-color: {grad_stop_far};" }
                    }
                }
                path {
                    class: "ux-popover-outline-path",
                    d: "{path}",
                    fill: "url(#{grad_id})",
                    fill_rule: "evenodd",
                }
            }
            button {
                style: "{trigger_style}",
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
                class: "tw:text-sm tw:text-muted-foreground",
                style: "{panel_style}",
                role: "dialog",
                div { class: "tw:grid tw:gap-1",
                    span { class: "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-heading", "icon menu" }
                    strong { class: "tw:text-sm tw:text-strong-foreground", "Bound" }
                    p { class: "tw:m-0 tw:text-xs tw:text-muted-foreground", "One SVG path draws the merged trigger + panel chrome." }
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
