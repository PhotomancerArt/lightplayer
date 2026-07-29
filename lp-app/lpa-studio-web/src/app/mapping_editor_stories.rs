//! Stories for the standalone mapping editor (`#/mapping`,
//! `lpa-mapping-editor`). Mount states are pinned via the editor's
//! deterministic story props — no animation, no measured viewport.

use dioxus::prelude::*;
use lpa_mapping_editor::{EditorViewOptions, MapEditor};
use lpa_studio_web_story_macros::story;

use crate::app::node::face_story_fixtures::fyeah_presentable_doc;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn EditorCanvasFrame(children: Element) -> Element {
    rsx! {
        div { style: "height: 620px; display: flex; flex-direction: column; border: 1px solid var(--studio-color-border-strong); border-radius: 10px; overflow: hidden;",
            {children}
        }
    }
}

#[story(
    description = "The 16×16 snake panel in the editor: wiring numbers + arrows at fit zoom — the editor's default view."
)]
pub(crate) fn editor_panel_wiring() -> Element {
    rsx! {
        EditorCanvasFrame {
            MapEditor { doc_epoch: 0, doc: lpc_mapping::corpus::panel_16x16() }
        }
    }
}

#[story(
    description = "Single selection with the anchored properties popover: the cat-ears headband path (count, direction, reorder, expand, delete)."
)]
pub(crate) fn editor_selection_popover() -> Element {
    rsx! {
        EditorCanvasFrame {
            MapEditor {
                doc_epoch: 0,
                doc: lpc_mapping::corpus::cat_ears(),
                initial_selection: vec![2],
            }
        }
    }
}

#[story(
    description = "Group selection across the whole sign import: bbox outline + corner resize handles + multi-select popover."
)]
pub(crate) fn editor_group_selected() -> Element {
    rsx! {
        EditorCanvasFrame {
            MapEditor {
                doc_epoch: 0,
                doc: fyeah_presentable_doc(),
                initial_selection: vec![0, 1, 2, 3, 4, 5, 6],
            }
        }
    }
}

#[story(
    description = "Path drawing in progress: draft vertices, resolved ghost lamps, and the gold chain link from the previous object."
)]
pub(crate) fn editor_path_draft() -> Element {
    rsx! {
        EditorCanvasFrame {
            MapEditor {
                doc_epoch: 0,
                doc: lpc_mapping::corpus::cat_ears(),
                initial_draft: vec![[460.0, 320.0], [520.0, 240.0], [580.0, 320.0]],
            }
        }
    }
}

#[story(
    description = "Texture-frame fit preview on the sign import: the square render target vs the doc canvas — why the sign sat corner-pinned."
)]
pub(crate) fn editor_fit_preview() -> Element {
    rsx! {
        EditorCanvasFrame {
            MapEditor {
                doc_epoch: 0,
                doc: fyeah_presentable_doc(),
                initial_view: Some(EditorViewOptions {
                    numbers: false,
                    arrows: true,
                    universes: false,
                    live: false,
                    fit_preview: true,
                }),
            }
        }
    }
}

#[story(
    description = "Universe coloring in the editor: 256 panel lamps flowing across the 170-lamp boundary, ranges annotated per universe in the rail."
)]
pub(crate) fn editor_universes() -> Element {
    rsx! {
        EditorCanvasFrame {
            MapEditor {
                doc_epoch: 0,
                doc: lpc_mapping::corpus::panel_16x16(),
                initial_view: Some(EditorViewOptions {
                    numbers: false,
                    arrows: false,
                    universes: true,
                    live: false,
                    fit_preview: false,
                }),
            }
        }
    }
}
