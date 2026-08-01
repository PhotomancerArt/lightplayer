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
    // Bumped on expand/collapse: the editor re-fits to the new box size.
    let mut refit = use_signal(|| 0_u64);
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
                                title: "texture-frame preview — how the doc fits shader space (F)",
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
                                    let bump = *refit.peek() + 1;
                                    refit.set(bump);
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
                            refit_epoch: refit(),
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
                if let Some(power) = face.power {
                    PowerReadout { power }
                }
            }
        }
    }
}

/// Estimated draw against the fixture's budget: a setup readout first, a
/// limiting indicator second.
///
/// One quiet line under the fader — no panel, no border, no badge. Two numbers
/// do not earn chrome (`docs/style/ui.md`). Only the limiting state takes
/// colour, and it takes `attention` rather than `warning`: shedding current to
/// stay inside a declared budget is the feature working, not a fault.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PowerReadout(power: lpa_studio_core::UiFixturePower) -> Element {
    let limiting = power.is_limiting();
    let percent = power.percent_of_budget();
    // "Estimated" is load-bearing, not hedging: every lamp preset ships
    // datasheet and community figures, and nothing here has met a meter.
    let title = format!(
        "estimated draw from the lamp type's power model — not measured\n{} mA of {} mA budget",
        power.estimated_draw_ma, power.budget_ma
    );

    rsx! {
        div {
            class: "tw:mt-2 tw:flex tw:items-baseline tw:gap-2 tw:font-mono tw:text-[0.7rem] tw:text-muted-foreground",
            title,
            // Nested so the row's gap falls only before the limiting chip —
            // the slash has to butt against the estimate to read as one figure.
            span {
                span { "≈{power.estimated_draw_ma}" }
                span { class: "tw:text-dim-foreground", "/{power.budget_ma} mA ({percent}%)" }
            }
            if limiting {
                span { class: "tw:text-status-attention-foreground",
                    "limiting to {(power.scale * 100.0).round() as u32}%"
                }
            }
        }
    }
}
