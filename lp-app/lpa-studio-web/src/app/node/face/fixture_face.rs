//! The fixture card's permanent face: lit preview + dominant brightness
//! fader, in the flat section grammar.
//!
//! The `output` section is the fixture's "one home" (2D mapping plan D9):
//! the control product's LED sample points rendered full-bleed with mapping
//! view toggles (wiring numbers, arrows, universe colors, live output) —
//! and, when the mapping is a `Map2d` document, an `edit` toggle that flips
//! the same section into the in-place mapping editor, synced through the
//! asset pipeline (whole-body apply / project save). No separate pane.
//! Toggle + edit-mode state are view-local for now, same as the drawer
//! open-state (a CardUiState re-home is an existing follow-up).
//!
//! The `controls` section holds one dominant horizontal fader bound to
//! `FixtureDef.brightness.some`.

use dioxus::prelude::*;
use dioxus_icons::lucide::Pencil;
use lpa_studio_core::{UiAction, UiFixtureFace as UiFixtureFaceData, UiProductKind};

use crate::app::node::map_view::{MapViewOptions, MapViewToggles};
use crate::app::node::mapping_asset_editor::MappingAssetEditor;
use crate::app::node::produced_product_view::ProductPreview;
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
    let mut map_view = use_signal(move || initial_map_view.unwrap_or_default());
    let mut editing = use_signal(|| edit_initially_open);
    let show_toggles = preview.kind == UiProductKind::Control;
    let editable = face.mapping_editor.is_some();
    let edit_open = editable && editing();

    rsx! {
        NodeCardSection { label: "output", first: true,
            if show_toggles || editable {
                div { class: "ux-map-toggle-bar",
                    if editable {
                        button {
                            class: if edit_open { "ux-map-toggle ux-map-toggle-on" } else { "ux-map-toggle" },
                            title: if edit_open { "close the mapping editor" } else { "edit the mapping here" },
                            onclick: move |_| {
                                let now = *editing.peek();
                                editing.set(!now);
                            },
                            Pencil { size: 13 }
                        }
                    }
                    if show_toggles && !edit_open {
                        MapViewToggles {
                            value: map_view(),
                            on_change: move |next| map_view.set(next),
                            bare: true,
                        }
                    }
                }
            }
            if edit_open {
                if let Some(editor) = face.mapping_editor.clone() {
                    MappingAssetEditor { editor, on_action }
                }
            } else {
                ProductPreview {
                    kind: preview.kind,
                    preview: preview.preview.clone(),
                    tracking: preview.tracking,
                    frame: preview.frame,
                    focus_action: None,
                    on_action,
                    map_view: map_view(),
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
