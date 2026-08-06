//! Mapping view options and the toggle bar shared across the output
//! section's view ⇄ edit flip.
//!
//! Wiring instruments — `numbers`, `arrows`, `universes` — are **edit-mode
//! tools**: inspecting how a fixture is wired is an authoring activity, so
//! only the mapping editor renders them and only edit mode offers their
//! toggles. View mode is a product display: `live` colors lamps from the
//! control frame, off paints the neutral layout, and that is the whole
//! surface (see `LampView`).
//!
//! These options are still the bridge type for the shared toggle state: the
//! bar survives the flip, so one state feeds the display renderer and the
//! editor canvas.

use dioxus::prelude::*;
use lpa_mapping_editor::EditorViewOptions;

use crate::base::icon::{StudioIcon, StudioIconName};

/// View options for the lamp map display. `numbers`/`arrows`/`universes`
/// drive the mapping editor only — nothing in view mode reads them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapViewOptions {
    pub numbers: bool,
    pub arrows: bool,
    pub universes: bool,
    pub live: bool,
}

impl Default for MapViewOptions {
    fn default() -> Self {
        Self {
            numbers: false,
            arrows: false,
            universes: false,
            live: true,
        }
    }
}

// One view state serves both faces of the output section (the toggle bar
// stays live across the view ⇄ edit flip): the editor's options are the
// superset, these conversions carry the shared fields.
impl From<EditorViewOptions> for MapViewOptions {
    fn from(opts: EditorViewOptions) -> Self {
        Self {
            numbers: opts.numbers,
            arrows: opts.arrows,
            universes: opts.universes,
            live: opts.live,
        }
    }
}

impl MapViewOptions {
    /// Editor options with these shared fields and editor-only fields at
    /// their defaults (initial face state).
    #[must_use]
    pub fn into_editor(self) -> EditorViewOptions {
        EditorViewOptions {
            numbers: self.numbers,
            arrows: self.arrows,
            universes: self.universes,
            live: self.live,
            fit_preview: false,
            reference: true,
        }
    }

    /// Write the shared fields into `editor`, preserving editor-only state
    /// (fit preview).
    pub fn apply_to_editor(self, editor: &mut EditorViewOptions) {
        editor.numbers = self.numbers;
        editor.arrows = self.arrows;
        editor.universes = self.universes;
        editor.live = self.live;
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
    /// Offer the wiring instruments (numbers, arrows, universe colors).
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
                button {
                    class: class_for(value.universes),
                    title: "universe colors, 170 lamps each (U)",
                    onclick: toggle(|mut v| { v.universes = !v.universes; v }),
                    StudioIcon { name: StudioIconName::MapUniverses, size: 13 }
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
