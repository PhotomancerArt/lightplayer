//! Canvas furniture: the floating zoom control, the keyboard-help float,
//! and the contextual tool hint — composed next to the canvas by whatever
//! hosts it (the M5 gate direction: zoom is canvas furniture, not toolbar
//! chrome).

use dioxus::prelude::*;

use crate::editor_core::camera::Camera;
use crate::editor_core::editor_session::MapEditorSession;
use crate::editor_core::map_tool::{MapTool, PolygonMode};

/// The hint teaches the group grammar exactly when it applies (G1
/// feedback: double-click descend is undiscoverable without a prompt):
/// a selected group invites entering; a descended selection explains
/// write-through and the way out. Tool hints otherwise.
///
/// Every hint NAMES ITS KEYS and stays short (the G1 ruling: "tell me what
/// keys do what" — a line describing what the canvas is already showing
/// teaches nothing).
#[must_use]
pub fn tool_hint(session: &MapEditorSession) -> &'static str {
    let selected_shape = matches!(session.tool, MapTool::Select)
        .then(|| session.selection.single())
        .flatten()
        .and_then(|path| path.resolve(session.doc()));
    let selected_group = selected_shape
        .is_some_and(|shape| crate::editor_core::shape_path::structural_child_count(shape) > 0);
    let vertexed = selected_shape.is_some_and(|shape| {
        crate::editor_core::editor_session::editable_vertices(shape).is_some()
    });
    let descended = matches!(session.tool, MapTool::Select)
        && session
            .selection
            .single()
            .is_some_and(|path| !path.is_root());
    match session.tool {
        MapTool::Select if selected_group => {
            "double-click enters the group — edit its sub-object with every instance live · esc leaves"
        }
        MapTool::Select if descended => {
            "editing the sub-object — every instance follows · esc leaves the group"
        }
        // With an outline in hand, the two gestures worth naming are the ones
        // nothing on screen suggests: corners are visible, adding and removing
        // them is not.
        MapTool::Select if vertexed => {
            "dbl-click an edge adds a corner · ⌫ removes the selected one · ⌘Z undo"
        }
        MapTool::Select => {
            "click selects · ⇧-click adds to selection · drag empty space for marquee · ⌘Z undo"
        }
        MapTool::Grid => "click to drop a default grid — size it in the properties popover",
        MapTool::Ring => "click to drop a default ring — tune it in the properties popover",
        MapTool::Path { .. } => {
            "click to place lamps · ⏎ or double-click finishes · esc backs out one point"
        }
        // The polygon hint tracks the DRAFT, because what the next click can
        // do changes while drawing: nothing can close an outline until three
        // points exist. Which population is coming is the toggle's job (and
        // the ghosts'), so only the empty draft spends words on it.
        MapTool::Polygon { ref draft, mode } => match (draft.len(), mode) {
            (0, PolygonMode::Outline) => "click to place the first corner · lamps ride the outline",
            (0, PolygonMode::Filled) => {
                "click to place the first corner · lamps fill a lattice inside"
            }
            (1..=2, _) => "click adds a corner · three make a shape · esc backs out one",
            _ => "click adds a corner · click the first corner or ⏎ closes · esc backs out one",
        },
    }
}

/// Floating "?" toggling a compact keyboard-shortcut reference, next to
/// the zoom float (the M5 round-3 ask: every tool has a key, and the keys
/// deserve a home).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn HelpFloat() -> Element {
    let mut open = use_signal(|| false);
    rsx! {
        div { class: "lpme-help-float",
            button {
                class: if open() { "lpme-btn lpme-btn-on" } else { "lpme-btn" },
                title: "keyboard shortcuts",
                onclick: move |_| {
                    let now = *open.peek();
                    open.set(!now);
                },
                "?"
            }
        }
        if open() {
            div { class: "lpme-help-panel",
                div { class: "lpme-help-title", "keyboard" }
                for (keys, what) in [
                    ("V / G / R / P / O", "select · grid · ring · path · polygon tool"),
                    ("N / A / L", "numbers · arrows · live"),
                    ("F", "texture-frame preview"),
                    ("0", "zoom to fit"),
                    ("⌘ + scroll", "zoom at cursor"),
                    ("right/middle-drag · scroll · space + drag", "pan"),
                    ("⌘Z / ⇧⌘Z", "undo · redo"),
                    ("⌘A", "select all"),
                    ("⌫", "delete the selected corner, else the selection"),
                    ("⏎", "finish path · close polygon"),
                    ("dbl-click", "enter a group · add a corner on an edge"),
                    ("esc", "back out · leave group · clear · leave the dive"),
                ] {
                    div { class: "lpme-help-row",
                        span { class: "lpme-help-keys", "{keys}" }
                        span { class: "lpme-help-what", "{what}" }
                    }
                }
            }
        }
    }
}

/// Floating zoom control, bottom-right of the canvas pane. The percent
/// readout doubles as "zoom to fit". Anchors button zooms on the measured
/// viewport center.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ZoomFloat(
    camera: Signal<Camera>,
    viewport: Signal<Option<[f32; 2]>>,
    fit_pending: Signal<bool>,
) -> Element {
    let mut camera = camera;
    let mut fit_pending = fit_pending;
    let percent = (camera().scale * 100.0).round() as u32;
    let center = move || {
        viewport.peek().map_or([600.0, 400.0], |[width, height]| {
            [width / 2.0, height / 2.0]
        })
    };
    rsx! {
        div { class: "lpme-zoom-float",
            button {
                class: "lpme-btn",
                title: "zoom out",
                onclick: move |_| camera.write().zoom_at(center(), 0.8),
                "−"
            }
            button {
                class: "lpme-btn lpme-zoom-pct",
                title: "zoom to fit (0)",
                onclick: move |_| fit_pending.set(true),
                "{percent}%"
            }
            button {
                class: "lpme-btn",
                title: "zoom in",
                onclick: move |_| camera.write().zoom_at(center(), 1.25),
                "+"
            }
        }
    }
}
