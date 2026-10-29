//! Mapping view options and the toggle bar above a fixture's output
//! section.
//!
//! Wiring instruments — `numbers`, `arrows` — are **edit-mode
//! tools**: inspecting how a fixture is wired is an authoring activity, so
//! only the mapping editor renders them and only edit mode offers their
//! toggles. View mode is a product display: `live` colors lamps from the
//! control frame, off paints the neutral layout, and that is the whole
//! surface (see `LampView`).

use dioxus::prelude::*;

use crate::base::icon::{StudioIcon, StudioIconName};

/// View options for the lamp map display. `numbers`/`arrows` drive the
/// mapping editor only — nothing in view mode reads them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapViewOptions {
    pub numbers: bool,
    pub arrows: bool,
    pub live: bool,
}

impl Default for MapViewOptions {
    fn default() -> Self {
        Self {
            numbers: false,
            arrows: false,
            live: true,
        }
    }
}

/// Pinned icon-toggle bar for the map view options (sits above the lamp
/// display in the output section — pinned, not floating, per the M3 gate).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn MapViewToggles(
    value: MapViewOptions,
    on_change: EventHandler<MapViewOptions>,
    /// Render only the buttons (the host provides the bar wrapper).
    #[props(default = false)]
    bare: bool,
    /// Offer the wiring instruments (numbers, arrows).
    /// Edit mode only — view mode is a product display, so its bar carries
    /// the live toggle alone.
    #[props(default = false)]
    wiring: bool,
) -> Element {
    let toggle = move |apply: fn(MapViewOptions) -> MapViewOptions| {
        let next = apply(value);
        move |_| on_change.call(next)
    };
    let class_for = |on: bool| {
        if on {
            "ux-map-toggle ux-map-toggle-on"
        } else {
            "ux-map-toggle"
        }
    };
    let buttons = rsx! {
            if wiring {
                button {
                    class: class_for(value.numbers),
                    title: "wiring numbers (N)",
                    onclick: toggle(|mut v| { v.numbers = !v.numbers; v }),
                    StudioIcon { name: StudioIconName::MapNumbers, size: 13 }
                }
                button {
                    class: class_for(value.arrows),
                    title: "wiring arrows (A)",
                    onclick: toggle(|mut v| { v.arrows = !v.arrows; v }),
                    StudioIcon { name: StudioIconName::MapArrows, size: 13 }
                }
            }
            button {
                class: class_for(value.live),
                title: "live output colors (L)",
                onclick: toggle(|mut v| { v.live = !v.live; v }),
                StudioIcon { name: StudioIconName::MapLive, size: 13 }
            }
    };
    if bare {
        buttons
    } else {
        rsx! {
            div { class: "ux-map-toggle-bar", {buttons} }
        }
    }
}
