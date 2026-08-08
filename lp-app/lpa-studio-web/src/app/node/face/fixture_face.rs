//! The fixture card's permanent face: lit preview + dominant brightness
//! fader, in the flat section grammar.
//!
//! The `output` section is the fixture's "one home" (2D mapping plan D9):
//! the control product's LED sample points rendered full-bleed on the lamp
//! canvas — and, when the mapping is a `Map2d` document, an `edit` toggle
//! that flips the same section into the in-place mapping editor, synced
//! through the asset pipeline (whole-body apply / project save). No
//! separate pane.
//!
//! The bar above that hero carries the product's own chrome at its head —
//! name, the publish chip when the control output is wired to a bus
//! channel, and, at the far end, the same "i" detail affordance every slot
//! surface has (the clock face's product header, established at the
//! transport-hero G1 gate). A custom hero replaces the boxed
//! [`ProducedProductView`](crate::app::node::ProducedProductView) pane, and
//! the pane is what used to carry that chrome; without it the fixture's
//! output was the one produced product in Studio you could not inspect or
//! see the link status of ("we don't show the output detail / link status
//! either" — 2026-08-05). It shares the toggle bar rather than taking a
//! header row of its own: two near-empty bars stacked over the lamp field
//! is precisely the chrome two words and two buttons have not earned.
//!
//! The toggle bar is stable across the flip: the pencil keeps its spot at
//! the head of the instrument cluster (click again to leave edit mode) and
//! the shared view state feeds whichever renderer is showing, including
//! live output colors. What the
//! bar *offers* is not stable, and should not be: the wiring instruments
//! (numbers, arrows, universe colors) are authoring tools, so they appear
//! only in edit mode, and view mode's bar carries the live toggle alone.
//! Edit mode also adds the texture-frame toggle and a full-page expand
//! (fixed-position in place; the section never leaves the DOM). Toggle +
//! edit-mode state are view-local for now, same as the drawer open-state (a
//! CardUiState re-home is an existing follow-up).
//!
//! Between the output and the settings sits the `space` section — the
//! CONSUMER half of the two-sided space model (D13): this fixture's
//! `consume` policy and its "does strip order mean something?" bit,
//! rendered by the same component the shader card renders its declaration
//! with, so the two cannot drift apart.
//!
//! The `controls` section holds one dominant horizontal fader bound to
//! `FixtureDef.brightness.some`.

use dioxus::prelude::*;
use dioxus_icons::lucide::{Maximize2, Minimize2, Pencil, Scan};
use lpa_studio_core::{
    NodeCardDrawer, NodeUiOp, UiAction, UiFixtureFace as UiFixtureFaceData, UiProductKind,
    UiProductPreview,
};

use crate::app::node::face::node_ui_action;
use crate::app::node::lamp_view::control_live_lamp_colors;
use crate::app::node::map_view::{MapViewOptions, MapViewToggles};
use crate::app::node::mapping_asset_editor::MappingAssetEditor;
use crate::app::node::{
    NodeCardSection, PanelControl, ProductIdentity, ProductPreview, SlotDetailButton,
};

use super::space_section::{SPACE_SECTION_LABEL, SpaceSection};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn FixtureFace(
    face: UiFixtureFaceData,
    /// Open the fader's label-trigger detail popover on first render
    /// (stories).
    #[props(default = false)]
    detail_initially_open: bool,
    /// Open the produced output's header detail popover on first render
    /// (stories).
    #[props(default = false)]
    output_detail_initially_open: bool,
    /// Initial map view options (stories render deterministic states).
    #[props(default)]
    initial_map_view: Option<MapViewOptions>,
    /// Mount with the mapping editor open (stories).
    #[props(default = false)]
    edit_initially_open: bool,
    /// The node's address path, keying the power readout's open-the-drawer
    /// tap. Absent (stories) leaves the readout inert.
    #[props(default = None)]
    node: Option<String>,
    /// Open this space cell's tile picker on first render (stories).
    #[props(default = None)]
    space_picker_open_cell: Option<lpa_studio_core::UiSpaceCellRole>,
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
                div { class: "ux-map-toggle-bar",
                    // The product's own chrome, at the head of the bar the
                    // hero already had: name, publish chip, and (at the far
                    // end) the detail popover. One chrome row, not two —
                    // two near-empty bars stacked over the lamp field is
                    // exactly the chrome the style rules refuse.
                    ProductIdentity { product: preview.clone() }
                    div { class: "lpme-spacer" }
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
                    if show_toggles {
                        MapViewToggles {
                            value: view().into(),
                            on_change: move |next: MapViewOptions| {
                                next.apply_to_editor(&mut view.write());
                            },
                            bare: true,
                            // The wiring instruments are authoring tools:
                            // view mode shows the product, edit mode
                            // inspects the wiring.
                            wiring: edit_open,
                        }
                    }
                    SlotDetailButton {
                        label: preview.name.clone(),
                        aspects: preview.visible_aspects(),
                        initially_open: output_detail_initially_open,
                        on_action,
                        authoring: preview.authoring.clone(),
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
                        live: view().live,
                    }
                }
            }
        }
        // The consumer half of the mirror, in the same slot the shader card
        // gives the producer half: between the output and the settings
        // (D13). Same DTO, same component, opposite side.
        if let Some(space) = face.space.clone() {
            NodeCardSection { label: SPACE_SECTION_LABEL,
                SpaceSection {
                    section: space,
                    picker_open_cell: space_picker_open_cell,
                    on_action,
                }
            }
        }
        NodeCardSection { label: "settings",
            div { class: "tw:px-4 tw:py-3",
                PanelControl {
                    control: face.brightness.clone(),
                    detail_initially_open,
                    on_action,
                }
                if let Some(power) = face.power {
                    PowerReadout { power, node, on_action }
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
///
/// The line is also the way TO the budget: tapping it opens the advanced
/// drawer, where the `power` slot's lamp/budget editor lives. "A budget set
/// too low dims the show and is corrected in seconds" (the power ADR) only
/// holds if the person watching "limiting to 62%" can reach the editor from
/// the readout that told them.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PowerReadout(
    power: lpa_studio_core::UiFixturePower,
    #[props(default = None)] node: Option<String>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let limiting = power.is_limiting();
    let percent = power.percent_of_budget();
    let tappable = node.is_some() && on_action.is_some();
    // "Estimated" is load-bearing, not hedging: every lamp preset ships
    // datasheet and community figures, and nothing here has met a meter.
    let mut title = format!(
        "estimated draw from the lamp type's power model — not measured\n{} mA of {} mA budget",
        power.estimated_draw_ma, power.budget_ma
    );
    if tappable {
        title.push_str("\nclick to edit the budget (advanced drawer)");
    }

    rsx! {
        div {
            class: if tappable {
                "tw:mt-2 tw:flex tw:cursor-pointer tw:items-baseline tw:gap-2 tw:font-mono tw:text-[0.7rem] tw:text-muted-foreground tw:hover:text-foreground"
            } else {
                "tw:mt-2 tw:flex tw:items-baseline tw:gap-2 tw:font-mono tw:text-[0.7rem] tw:text-muted-foreground"
            },
            title,
            onclick: move |_| {
                let (Some(node), Some(handler)) = (node.clone(), on_action) else {
                    return;
                };
                handler.call(node_ui_action(NodeUiOp::SetDrawer {
                    node,
                    drawer: NodeCardDrawer::Advanced,
                    open: true,
                }));
            },
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
