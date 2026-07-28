//! The fixture card's permanent face: lit preview + dominant brightness
//! fader, in the flat section grammar.
//!
//! The `output` section is the fixture's "one home" (2D mapping plan D9):
//! the control product's LED sample points rendered full-bleed with mapping
//! view toggles (wiring numbers, arrows, universe colors, live output) —
//! and, when the mapping is a `Map2d` document, an `edit` toggle that flips
//! the same section into the in-place mapping editor, synced through the
//! asset pipeline (whole-body apply / project save). No separate pane.
//!
//! The toggle bar is stable across the flip: the pencil keeps its far-left
//! spot (click again to leave edit mode) and the same view toggles keep
//! driving whichever renderer is showing — one shared view state feeds the
//! display and the editor canvas, including live output colors. Edit mode
//! adds the texture-frame toggle and a full-page expand (fixed-position in
//! place; the section never leaves the DOM). Toggle + edit-mode state are
//! view-local for now, same as the drawer open-state (a CardUiState
//! re-home is an existing follow-up).
//!
//! The `controls` section holds one dominant horizontal fader bound to
//! `FixtureDef.brightness.some`.

use dioxus::prelude::*;
use dioxus_icons::lucide::{Maximize2, Minimize2, Pencil, Scan};
use lpa_studio_core::{
    UiAction, UiFixtureFace as UiFixtureFaceData, UiProductKind, UiProductPreview,
};

use crate::app::node::map_view::{MapViewOptions, MapViewToggles};
use crate::app::node::mapping_asset_editor::MappingAssetEditor;
use crate::app::node::produced_product_view::{ProductPreview, control_live_lamp_colors};
use crate::app::node::{NodeCardSection, PanelControl};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn FixtureFace(
    face: UiFixtureFaceData,
    /// Open the fader's label-trigger detail popover on first render
    /// (stories).
    #[props(default = false)]
    detail_initially_open: bool,
    /// Initial map view options (stories render deterministic states).
    #[props(default)]
    initial_map_view: Option<MapViewOptions>,
    /// Mount with the mapping editor open (stories).
    #[props(default = false)]
    edit_initially_open: bool,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let preview = face.preview.clone();
    // One view state for both faces of the section: the same toggle bar
    // (and its state) survives the view ⇄ edit flip, and the toggles drive
    // the editor canvas exactly like the display renderer.
    let mut view = use_signal(move || initial_map_view.unwrap_or_default().into_editor());
    let mut editing = use_signal(|| edit_initially_open);
    let mut expanded = use_signal(|| false);
    let show_toggles = preview.kind == UiProductKind::Control;
    let editable = face.mapping_editor.is_some();
    let edit_open = editable && editing();
    let full = edit_open && expanded();
    // Live lamp colors for the editor's live view, decoded from the same
    // control preview the display mode renders. Only fed while the live
    // toggle is on: an empty vec keeps the editor's props stable so it
    // skips the per-frame re-render entirely when live is off.
    let live_colors = if edit_open && view().live {
        match &preview.preview {
            UiProductPreview::ControlNative(control) => control_live_lamp_colors(control),
            _ => Vec::new(),
        }
    } else {
        Vec::new()
    };

    rsx! {
        NodeCardSection { label: "output", first: true,
            div { class: if full { "ux-map-home ux-map-home-full" } else { "ux-map-home" },
                if show_toggles || editable {
                    div { class: "ux-map-toggle-bar",
                        if editable {
                            button {
                                class: if edit_open { "ux-map-toggle ux-map-toggle-on" } else { "ux-map-toggle" },
                                title: if edit_open { "close the mapping editor" } else { "edit the mapping here" },
                                onclick: move |_| {
                                    let now = *editing.peek();
                                    editing.set(!now);
                                    if now {
                                        expanded.set(false);
                                    }
                                },
                                Pencil { size: 13 }
                            }
                        }
                        if edit_open {
                            button {
                                class: if view().fit_preview { "ux-map-toggle ux-map-toggle-on" } else { "ux-map-toggle" },
                                title: "texture-frame preview (how the doc fits shader space)",
                                onclick: move |_| {
                                    let now = view.peek().fit_preview;
                                    view.write().fit_preview = !now;
                                },
                                Scan { size: 13 }
                            }
                            button {
                                class: "ux-map-toggle",
                                title: if full { "back to the card" } else { "expand the editor to the full page" },
                                onclick: move |_| {
                                    let now = *expanded.peek();
                                    expanded.set(!now);
                                },
                                if full {
                                    Minimize2 { size: 13 }
                                } else {
                                    Maximize2 { size: 13 }
                                }
                            }
                        }
                        div { class: "lpme-spacer" }
                        if show_toggles {
                            MapViewToggles {
                                value: view().into(),
                                on_change: move |next: MapViewOptions| {
                                    next.apply_to_editor(&mut view.write());
                                },
                                bare: true,
                            }
                        }
                    }
                }
                if edit_open {
                    if let Some(editor) = face.mapping_editor.clone() {
                        MappingAssetEditor {
                            editor,
                            shared_view: view,
                            live_colors,
                            on_action,
                        }
                    }
                } else {
                    ProductPreview {
                        kind: preview.kind,
                        preview: preview.preview.clone(),
                        tracking: preview.tracking,
                        frame: preview.frame,
                        focus_action: None,
                        on_action,
                        map_view: view().into(),
                    }
                }
            }
        }
        NodeCardSection { label: "controls",
            div { class: "tw:px-4 tw:py-3",
                PanelControl {
                    control: face.brightness.clone(),
                    detail_initially_open,
                    on_action,
                }
            }
        }
    }
}
