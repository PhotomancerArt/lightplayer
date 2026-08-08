//! The `space` section both visual-side faces grow — one component, both
//! sides of the two-sided space model (dimensionality plan-B P4, spike
//! `spikes/dimensionality/index.html` §2/§3/§4B).
//!
//! **One component, two sides.** P3 made the mirror a data fact: a shader's
//! declaration and a fixture's consume policy arrive in the same
//! [`UiSpaceSection`] DTO, differing only by [`UiSpaceSide`]. This renders
//! that DTO, so the two cards cannot drift apart by styling accident — the
//! producer's "default projection" cell and the consumer's "from 1D
//! sources" cell are literally the same row renderer with the same picker
//! behind it.
//!
//! **No parallel write path.** Every gesture here is the op the generic
//! drawer row would have sent: a variant tile dispatches `EnsurePresent` at
//! `cell.address.child_field(&variant)` (exactly `EnumVariantField`'s
//! gesture), a flag checkbox dispatches `SetValue` at `flag.address`
//! (exactly `BoolSlotField`'s). The section is a different PRESENTATION of
//! the rows it claimed out of the advanced drawer, never a second writer.
//!
//! **Tiles are schematic, not live** (plan A2, decided here — see the
//! `ProjectionGlyph` doc): the picker draws each projection's shape rather
//! than probing the product through it, because nothing on the web side can
//! issue an ad-hoc forced-policy probe today.
//!
//! **Wording lives at the top of this file.** The G1 gate rules on the
//! labels (plan Q10/D16), and a ruling has to be a one-file edit — so the
//! component reads a cell's ROLE from the DTO and spells the label here,
//! rather than printing the derivation's own `label` strings.

use std::sync::atomic::{AtomicUsize, Ordering};

use dioxus::prelude::*;
use lpa_studio_core::{
    LpValue, ProjectSlotAddress, UiAction, UiCellProjection, UiNodeFace, UiProjectionOrigin,
    UiSpaceCell, UiSpaceCellRole, UiSpaceChoice, UiSpaceFlag, UiSpaceFlagRole, UiSpaceMismatch,
    UiSpaceSection, UiSpaceSide, UiVisualSpace,
};

use crate::app::node::slot_edit_actions::{slot_ensure_present_action, slot_set_value_action};
use crate::app::node::slot_fields::{field_class, field_wiring};
use crate::base::{
    PopoverButton, PopoverCloseHandle, PopoverPlacement, StudioIcon, StudioIconName,
    detail_popover_card_class,
};

// ---------------------------------------------------------------------------
// Wording (G1 rules on all of it — keep every user-facing string here)
// ---------------------------------------------------------------------------

/// The section's rail label, same rank as `output` and `settings` (D13).
pub(crate) const SPACE_SECTION_LABEL: &str = "space";

/// Producer side: what the leading enum is answering.
const PRODUCER_PRIMARY_LABEL: &str = "renders in";
/// Consumer side: same slot in the layout, the other voice.
const CONSUMER_PRIMARY_LABEL: &str = "consumes";

/// A 1D producer's answer for 2D consumers (plan Q10 candidate).
const PRODUCER_IN_2D_LABEL: &str = "default projection";
/// A 2D producer's answer for 1D consumers — one variant today, so a
/// statement rather than a picker.
const PRODUCER_IN_1D_LABEL: &str = "to 1D consumers";
/// A consumer's default for 1D sources (plan Q10 candidate).
const CONSUMER_FROM_1D_LABEL: &str = "from 1D sources";

/// The inline bit that makes a consumer policy win (spike §3).
const FORCE_LABEL: &str = "force";
const FORCE_TITLE: &str = "Use this fixture's default even when the source declares one";
/// Vision D3's authored bit.
const STRIP_ORDER_LABEL: &str = "strip order means something";
const STRIP_ORDER_TITLE: &str = "Yes: 1D effects run along the wire order (a strip worn in a shape). No: the map is the real \
     layout and wire order is plumbing.";

/// Variant vocabulary. `Default` reads differently per cell, which is why
/// these are keyed by role rather than by variant alone.
const SPACE_ONE_D: &str = "1D";
const SPACE_TWO_D: &str = "2D";
const CONSUME_AUTO: &str = "auto";
const CONSUME_POLICY: &str = "policy";
const PROJECTION_DEFER: &str = "consumer decides";
const PROJECTION_EXTRUDE: &str = "extrude";
const PROJECTION_RADIAL: &str = "radial";
const PROJECTION_ANGULAR: &str = "angular";
const PROJECTION_MIRROR: &str = "mirror";
const PROJECTION_CENTRE_SCANLINE: &str = "centre scanline";

/// One line per projection in the picker's tiles.
const HINT_DEFER: &str = "let the fixture decide";
const HINT_EXTRUDE: &str = "the strip, stretched down";
const HINT_RADIAL: &str = "the strip, out from the centre";
const HINT_ANGULAR: &str = "the strip, swept around";
const HINT_MIRROR: &str = "the strip, folded at the centre";

/// What this side is saying, in one line under the primary row.
const PRODUCER_HINT_ONE_D: &str = "This shader renders along a strip.";
const PRODUCER_HINT_TWO_D: &str = "This shader renders in texture space.";
const CONSUMER_HINT_AUTO: &str = "Follows whatever each source declares.";
const CONSUMER_HINT_POLICY: &str = "Fills in when a source declares nothing.";

/// The who-wins ladder (spike §3), compressed to the one rung that can
/// still surprise the person reading this card.
const LADDER_PRODUCER: &str = "A fixture that forces its own default wins over this.";
const LADDER_CONSUMER_FILLS: &str = "A source's own declaration wins over this.";
const LADDER_CONSUMER_FORCES: &str = "Forced: this wins over a source's own declaration.";

/// D1 — the declaration and the GLSL entry disagree.
const MISMATCH_TITLE: &str = "This declaration doesn't match the code.";
const MISMATCH_FIX: &str = "Change the declaration here, or rename the entry in the code drawer.";
const ENTRY_ONE_D: &str = "render_1d";
const ENTRY_TWO_D: &str = "render_2d";

/// The picker.
const PICKER_LABEL: &str = "Choose a projection";
const PICKER_TITLE: &str = "How a 1D source fills 2D space";

/// The card header's dimensionality badge (spike §4B).
const BADGE_TITLE: &str = "The space this shader renders in";

// ---------------------------------------------------------------------------
// Preview-space wording (the D15 checkboxes and their captions live in
// `preview_spaces.rs`, but their strings belong to the same G1 ruling)
// ---------------------------------------------------------------------------

/// The checkbox bar's own label and its two boxes.
pub(crate) const PREVIEW_SPACES_TITLE_ONE_D: &str = "preview along a 1D strip";
pub(crate) const PREVIEW_SPACES_TITLE_TWO_D: &str = "preview in 2D texture space";
pub(crate) const PREVIEW_SPACES_LAST_ON: &str = "one preview space has to stay on";
/// Caption vocabulary: `native · 1D`, `in 2D · radial (declared)`.
const CAPTION_NATIVE: &str = "native";
const CAPTION_IN: &str = "in";
const ORIGIN_DECLARED: &str = "declared";
const ORIGIN_CONSUMER_DEFAULT: &str = "consumer default";
const ORIGIN_FORCED: &str = "forced";

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

/// Anchored-outline ids for the picker fields (one per mounted cell).
static NEXT_PICKER_ID: AtomicUsize = AtomicUsize::new(1);

/// How wide the tile picker gets regardless of the field it hangs from: two
/// tile columns plus their labels need more room than a cell field has.
const PICKER_MIN_WIDTH_PX: f64 = 268.0;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn SpaceSection(
    section: UiSpaceSection,
    /// Open this cell's tile picker on first render (stories).
    #[props(default = None)]
    picker_open_cell: Option<UiSpaceCellRole>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let side = section.side;
    let mismatched = section.mismatch.is_some();
    // The force bit rides the policy row it qualifies (spike §3's inline
    // checkbox), so it is pulled out of the flag list here rather than
    // stacking as a row of its own.
    let force = section.flag(UiSpaceFlagRole::ForcePolicy).cloned();
    let standalone_flags: Vec<UiSpaceFlag> = section
        .flags
        .iter()
        .filter(|flag| flag.role != UiSpaceFlagRole::ForcePolicy)
        .cloned()
        .collect();

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-2 tw:px-4 tw:py-3",
            div { class: "tw:flex tw:min-w-0 tw:flex-wrap tw:items-center tw:gap-2",
                span { class: ROW_LABEL_CLASS, "{primary_label(side)}" }
                SpaceSegments {
                    cell: section.primary.clone(),
                    side,
                    mismatched,
                    on_action,
                }
            }
            p { class: HINT_CLASS, "{primary_hint(&section)}" }
            for cell in section.cells.clone() {
                SpaceCellRow {
                    key: "{cell.role:?}",
                    cell: cell.clone(),
                    side,
                    force: force.clone().filter(|_| cell.role == UiSpaceCellRole::ConsumerFrom1d),
                    picker_initially_open: picker_open_cell == Some(cell.role),
                    on_action,
                }
            }
            for flag in standalone_flags {
                SpaceFlagRow { key: "{flag.role:?}", flag, on_action }
            }
            if let Some(ladder) = ladder_line(&section) {
                p { class: LADDER_CLASS, "{ladder}" }
            }
            if let Some(mismatch) = section.mismatch.clone() {
                SpaceMismatchNote { mismatch }
            }
        }
    }
}

/// The leading enum as a segmented row of squared blocks — a discrete
/// choice between two named states, which is the shape
/// `docs/style/ui.md`'s discrete-control language asks for (never a
/// dropdown over two items).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SpaceSegments(
    cell: UiSpaceCell,
    side: UiSpaceSide,
    #[props(default = false)] mismatched: bool,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let wiring = field_wiring(&cell.state, &cell.address, on_action);
    rsx! {
        span { class: segment_group_class(mismatched),
            for choice in cell.choices.clone() {
                if let Some((address, handler)) = wiring.clone() {
                    button {
                        key: "{choice.variant}",
                        class: segment_class(choice.selected),
                        r#type: "button",
                        onclick: {
                            let variant = choice.variant.clone();
                            let selected = choice.selected;
                            move |event: MouseEvent| {
                                event.stop_propagation();
                                if selected {
                                    return;
                                }
                                if let Some(target) = address.child_field(&variant) {
                                    handler.call(slot_ensure_present_action(target));
                                }
                            }
                        },
                        "{variant_label(side, cell.role, &choice)}"
                    }
                } else {
                    span { key: "{choice.variant}", class: segment_class(choice.selected),
                        "{variant_label(side, cell.role, &choice)}"
                    }
                }
            }
        }
    }
}

/// One answer cell: its label, the projection field (picker or statement),
/// and — on the consumer's policy row — the inline `force` bit.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SpaceCellRow(
    cell: UiSpaceCell,
    side: UiSpaceSide,
    #[props(default = None)] force: Option<UiSpaceFlag>,
    #[props(default = false)] picker_initially_open: bool,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    rsx! {
        div { class: "tw:flex tw:min-w-0 tw:flex-wrap tw:items-center tw:gap-2",
            span { class: ROW_LABEL_CLASS, "{cell_label(side, cell.role)}" }
            ProjectionField {
                cell: cell.clone(),
                side,
                initially_open: picker_initially_open,
                on_action,
            }
            if let Some(force) = force {
                SpaceFlagCheckbox { flag: force, title: FORCE_TITLE, on_action }
            }
        }
    }
}

/// The cell's control: an anchored tile picker when there is a real choice,
/// a read-only statement when there is not (`UiSpaceCell::is_choosable` —
/// the 2D→1D answer has one declared variant today, and a dropdown over one
/// option invites a gesture with nothing to change).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProjectionField(
    cell: UiSpaceCell,
    side: UiSpaceSide,
    #[props(default = false)] initially_open: bool,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let active_label = active_variant_label(side, &cell);
    let reachable = cell.is_choosable() && on_action.is_some();
    if !reachable {
        return rsx! {
            span { class: field_class(&cell.state),
                span { class: "tw:min-w-0 tw:truncate", "{active_label}" }
            }
        };
    }

    // Anchored mode: the FIELD is the trigger, so the merged outline grows
    // out of the control the tiles are about (`palette_swatch_field`'s
    // idiom — "the control IS the trigger").
    let anchor_id = use_hook(|| {
        let id = NEXT_PICKER_ID.fetch_add(1, Ordering::Relaxed);
        format!("ux-projection-field-{id}")
    });
    let face = projection_field_face(&cell, &active_label);

    rsx! {
        span {
            id: "{anchor_id}",
            class: "tw:inline-grid tw:min-w-0 tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page",
            PopoverButton {
                class: FIELD_TRIGGER_CLASS.to_string(),
                open_class: FIELD_TRIGGER_CLASS.to_string(),
                trigger: face.clone(),
                label: PICKER_LABEL.to_string(),
                title: PICKER_TITLE.to_string(),
                popup_class: detail_popover_card_class().to_string(),
                placement: PopoverPlacement::BottomStart,
                initially_open,
                match_anchor_width: true,
                min_panel_width_px: Some(PICKER_MIN_WIDTH_PX),
                anchor_id: Some(anchor_id.clone()),
                anchor_visual: face,
                ProjectionTileGrid {
                    choices: cell.choices.clone(),
                    side,
                    role: cell.role,
                    address: cell.address.clone(),
                    on_action,
                }
            }
        }
    }
}

/// The closed field's face: the active projection's own glyph, its name,
/// and the caret that says a picker lives behind it. Rendered twice while
/// the popover is open (in-flow placeholder + top-layer copy), so it stays
/// a plain function of the cell.
fn projection_field_face(cell: &UiSpaceCell, active_label: &str) -> Element {
    let projection = cell
        .choices
        .iter()
        .find(|choice| choice.selected)
        .and_then(|choice| choice.projection);
    rsx! {
        span { class: "tw:inline-flex tw:h-4 tw:w-6 tw:flex-none tw:items-center", aria_hidden: "true",
            ProjectionGlyph { projection }
        }
        span { class: "tw:min-w-0 tw:grow tw:truncate", "{active_label}" }
        span { class: "tw:inline-flex tw:flex-none tw:text-subtle-foreground", aria_hidden: "true",
            StudioIcon { name: StudioIconName::Expanded, size: 12 }
        }
    }
}

/// The picker's content: one tile per declared variant, each drawing what
/// that answer does to a strip. A pick dispatches and closes — a selection
/// is a completed gesture (the palette chooser's rule).
///
/// Its own component so a story can capture the grid directly as well as
/// through an open popover.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProjectionTileGrid(
    choices: Vec<UiSpaceChoice>,
    side: UiSpaceSide,
    role: UiSpaceCellRole,
    #[props(default = None)] address: Option<ProjectSlotAddress>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let close = try_consume_context::<PopoverCloseHandle>();
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:grid-cols-2 tw:gap-1.5 tw:p-2",
            for choice in choices {
                button {
                    key: "{choice.variant}",
                    class: tile_class(choice.selected),
                    r#type: "button",
                    title: "{projection_hint(choice.projection)}",
                    onclick: {
                        let variant = choice.variant.clone();
                        let address = address.clone();
                        let selected = choice.selected;
                        move |event: MouseEvent| {
                            event.stop_propagation();
                            if !selected
                                && let (Some(address), Some(handler)) = (address.clone(), on_action)
                                && let Some(target) = address.child_field(&variant)
                            {
                                handler.call(slot_ensure_present_action(target));
                            }
                            if let Some(mut close) = close {
                                close.close();
                            }
                        }
                    },
                    span { class: "tw:block tw:h-10 tw:w-full tw:overflow-hidden tw:rounded-xs tw:bg-page",
                        ProjectionGlyph { projection: choice.projection }
                    }
                    span { class: "tw:min-w-0 tw:truncate tw:text-[11px] tw:font-bold",
                        "{variant_label(side, role, &choice)}"
                    }
                    span { class: "tw:min-w-0 tw:truncate tw:text-[10px] tw:leading-tight tw:text-dim-foreground",
                        "{projection_hint(choice.projection)}"
                    }
                }
            }
        }
    }
}

/// A schematic drawing of what one projection does to a 1D source.
///
/// **Not a live probe (plan A2, resolved).** A live tile means rendering
/// THIS product through a forced policy, and the only path to that is
/// `ProjectSync`'s per-card `(product, space)` request table: it takes a
/// space and a hero, not a per-projection policy, and nothing in
/// `lpa-studio-web` can issue a probe of its own. Wiring one would be new
/// core plumbing (a per-`(product, projection)` tile request, its result
/// cache, and a DTO field to carry the bytes) — P4's brief explicitly
/// excludes that, so the tiles draw the SHAPE of each answer instead. The
/// glyphs read the same at 64×40 as a 32×32 probe would, and G1 rules on
/// whether live tiles are worth the plumbing.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProjectionGlyph(#[props(default = None)] projection: Option<UiCellProjection>) -> Element {
    // One vocabulary across all four: the source strip is a light-to-dark
    // ramp, and the glyph shows where that ramp goes.
    rsx! {
        svg {
            class: "tw:block tw:h-full tw:w-full tw:text-soft-foreground",
            view_box: "0 0 64 40",
            preserve_aspect_ratio: "none",
            role: "img",
            match projection {
                Some(UiCellProjection::Extrude) => rsx! {
                    for (index , opacity) in RAMP.iter().copied().enumerate() {
                        rect {
                            key: "{index}",
                            x: "{index * 8}",
                            y: "0",
                            width: "8",
                            height: "40",
                            fill: "currentColor",
                            fill_opacity: "{opacity}",
                        }
                    }
                },
                Some(UiCellProjection::Mirror) => rsx! {
                    for (index , opacity) in MIRROR_RAMP.iter().copied().enumerate() {
                        rect {
                            key: "{index}",
                            x: "{index * 8}",
                            y: "0",
                            width: "8",
                            height: "40",
                            fill: "currentColor",
                            fill_opacity: "{opacity}",
                        }
                    }
                },
                Some(UiCellProjection::Radial) => rsx! {
                    for (index , (radius , opacity)) in RADIAL_RINGS.iter().copied().enumerate() {
                        circle {
                            key: "{index}",
                            cx: "32",
                            cy: "20",
                            r: "{radius}",
                            fill: "currentColor",
                            fill_opacity: "{opacity}",
                        }
                    }
                },
                Some(UiCellProjection::Angular) => rsx! {
                    for (index , (x , y , opacity)) in ANGULAR_RAYS.iter().copied().enumerate() {
                        line {
                            key: "{index}",
                            x1: "32",
                            y1: "20",
                            x2: "{x}",
                            y2: "{y}",
                            stroke: "currentColor",
                            stroke_opacity: "{opacity}",
                            stroke_width: "7",
                        }
                    }
                },
                // "Consumer decides": nothing is drawn, because nothing is
                // decided here — a dashed hollow says the answer lives on
                // the other side of the binding.
                None => rsx! {
                    rect {
                        x: "3",
                        y: "3",
                        width: "58",
                        height: "34",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_opacity: "0.45",
                        stroke_width: "2",
                        stroke_dasharray: "5 4",
                        rx: "3",
                    }
                },
            }
        }
    }
}

/// The strip's ramp across eight bands (the source, left to right).
const RAMP: [f32; 8] = [0.14, 0.24, 0.36, 0.48, 0.6, 0.72, 0.84, 0.96];
/// The same ramp folded at the centre — what `mirror` does.
const MIRROR_RAMP: [f32; 8] = [0.14, 0.36, 0.6, 0.96, 0.96, 0.6, 0.36, 0.14];
/// Concentric rings, outermost first so the inner ones paint over.
const RADIAL_RINGS: [(u32, f32); 4] = [(26, 0.18), (19, 0.38), (12, 0.62), (5, 0.95)];
/// Eight rays at 45° steps, radius 26 from (32, 20) — the strip swept
/// around the centre.
const ANGULAR_RAYS: [(f32, f32, f32); 8] = [
    (58.0, 20.0, 0.14),
    (50.4, 38.4, 0.26),
    (32.0, 46.0, 0.38),
    (13.6, 38.4, 0.5),
    (6.0, 20.0, 0.62),
    (13.6, 1.6, 0.74),
    (32.0, -6.0, 0.86),
    (50.4, 1.6, 0.96),
];

/// A boolean the section owns, as its own row (`strip order means
/// something` — vision D3's authored bit).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SpaceFlagRow(
    flag: UiSpaceFlag,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let title = flag_title(flag.role);
    rsx! {
        div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2",
            SpaceFlagCheckbox { flag, title, on_action }
        }
    }
}

/// The flag itself: a squared checkbox plus its label, dispatching the
/// ordinary `SetValue` a bool row dispatches.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SpaceFlagCheckbox(
    flag: UiSpaceFlag,
    title: &'static str,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let label = flag_label(flag.role);
    let value = flag.value;
    let Some((address, handler)) = field_wiring(&flag.state, &flag.address, on_action) else {
        return rsx! {
            span { class: "tw:inline-flex tw:items-center tw:gap-1.5 tw:text-[11px] tw:text-dim-foreground",
                title,
                span { class: checkbox_box_class(value), aria_hidden: "true",
                    if value {
                        StudioIcon { name: StudioIconName::StepComplete, size: 10 }
                    }
                }
                "{label}"
            }
        };
    };

    rsx! {
        button {
            class: "tw:inline-flex tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-1.5 tw:border-0 tw:bg-transparent tw:p-0 tw:text-[11px] tw:text-subtle-foreground tw:hover:text-strong-foreground",
            r#type: "button",
            title,
            aria_pressed: "{value}",
            onclick: move |event| {
                event.stop_propagation();
                handler.call(slot_set_value_action(address.clone(), LpValue::Bool(!value)));
            },
            span { class: checkbox_box_class(value), aria_hidden: "true",
                if value {
                    StudioIcon { name: StudioIconName::StepComplete, size: 10 }
                }
            }
            "{label}"
        }
    }
}

/// D1 made visible on the card instead of buried in a compile log: the two
/// sides named, and where to fix it.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SpaceMismatchNote(mismatch: UiSpaceMismatch) -> Element {
    rsx! {
        div {
            class: "tw:grid tw:gap-0.5 tw:rounded-xs tw:border tw:border-status-error-border tw:bg-status-error-bg tw:px-2 tw:py-1.5",
            title: "{mismatch.message}",
            span { class: "tw:text-[11px] tw:font-bold tw:text-status-error-foreground",
                "{MISMATCH_TITLE}"
            }
            span { class: "tw:font-mono tw:text-[10.5px] tw:text-status-error-foreground",
                "{mismatch_line(&mismatch)}"
            }
            span { class: "tw:text-[10.5px] tw:leading-tight tw:text-status-error-foreground",
                "{MISMATCH_FIX}"
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Text
// ---------------------------------------------------------------------------

/// The dimensionality badge a card header wears (spike §4B). `None` on the
/// consumer side by construction: a fixture states a policy, not a space —
/// its own dimensionality comes from its mapping.
pub(crate) fn face_space_badge(face: &UiNodeFace) -> Option<&'static str> {
    let section = match face {
        UiNodeFace::Shader(face) => face.space.as_ref()?,
        UiNodeFace::Fixture(face) => face.space.as_ref()?,
        _ => return None,
    };
    space_badge(section)
}

fn space_badge(section: &UiSpaceSection) -> Option<&'static str> {
    section.declared_space.map(visual_space_label)
}

/// Tooltip for that badge.
pub(crate) const fn space_badge_title() -> &'static str {
    BADGE_TITLE
}

pub(crate) fn visual_space_label(space: UiVisualSpace) -> &'static str {
    match space {
        UiVisualSpace::OneD => SPACE_ONE_D,
        UiVisualSpace::TwoD => SPACE_TWO_D,
    }
}

fn projection_label(projection: UiCellProjection) -> &'static str {
    match projection {
        UiCellProjection::Extrude => PROJECTION_EXTRUDE,
        UiCellProjection::Radial => PROJECTION_RADIAL,
        UiCellProjection::Angular => PROJECTION_ANGULAR,
        UiCellProjection::Mirror => PROJECTION_MIRROR,
    }
}

fn origin_label(origin: UiProjectionOrigin) -> &'static str {
    match origin {
        UiProjectionOrigin::Declared => ORIGIN_DECLARED,
        UiProjectionOrigin::ConsumerDefault => ORIGIN_CONSUMER_DEFAULT,
        UiProjectionOrigin::Forced => ORIGIN_FORCED,
    }
}

/// The caption under one preview (D15): `native · 1D`,
/// `in 2D · radial (declared)`, `in 1D · centre scanline`.
///
/// Origin is never omitted when it is known — D11's honesty rule is the
/// whole point of the caption: a projection nobody authored must not read
/// like one somebody did.
pub(crate) fn preview_space_caption(
    space: UiVisualSpace,
    meta: Option<lpa_studio_core::UiVisualProductSpace>,
) -> String {
    let space_label = visual_space_label(space);
    let Some(meta) = meta else {
        return space_label.to_string();
    };
    if meta.space == meta.primary {
        return format!("{CAPTION_NATIVE} · {space_label}");
    }
    let how = match meta.projection {
        Some(projection) => projection_label(projection),
        // A 2D producer filling a 1D request has no 1D→2D cell to name;
        // the centre scanline is the only answer there is.
        None => PROJECTION_CENTRE_SCANLINE,
    };
    match meta.origin {
        Some(origin) => format!(
            "{CAPTION_IN} {space_label} · {how} ({})",
            origin_label(origin)
        ),
        None => format!("{CAPTION_IN} {space_label} · {how}"),
    }
}

fn primary_label(side: UiSpaceSide) -> &'static str {
    match side {
        UiSpaceSide::Producer => PRODUCER_PRIMARY_LABEL,
        UiSpaceSide::Consumer => CONSUMER_PRIMARY_LABEL,
    }
}

fn cell_label(side: UiSpaceSide, role: UiSpaceCellRole) -> &'static str {
    match (side, role) {
        (_, UiSpaceCellRole::ProducerIn2d) => PRODUCER_IN_2D_LABEL,
        (_, UiSpaceCellRole::ProducerIn1d) => PRODUCER_IN_1D_LABEL,
        (_, UiSpaceCellRole::ConsumerFrom1d) => CONSUMER_FROM_1D_LABEL,
        (side, UiSpaceCellRole::Primary) => primary_label(side),
    }
}

fn flag_label(role: UiSpaceFlagRole) -> &'static str {
    match role {
        UiSpaceFlagRole::ForcePolicy => FORCE_LABEL,
        UiSpaceFlagRole::StripOrderMeaningful => STRIP_ORDER_LABEL,
    }
}

fn flag_title(role: UiSpaceFlagRole) -> &'static str {
    match role {
        UiSpaceFlagRole::ForcePolicy => FORCE_TITLE,
        UiSpaceFlagRole::StripOrderMeaningful => STRIP_ORDER_TITLE,
    }
}

/// A variant's display name.
///
/// Keyed by ROLE, not by variant alone: `Default` means "consumer decides"
/// on a 1D shader's 2D answer and "centre scanline" on a 2D shader's 1D
/// one, and one vocabulary for both would lie about one of them. Anything
/// the vocabulary does not know falls back to the DTO's own label, so a
/// variant added to the model still renders something honest.
fn variant_label(side: UiSpaceSide, role: UiSpaceCellRole, choice: &UiSpaceChoice) -> String {
    known_variant_label(side, role, &choice.variant)
        .map(str::to_string)
        .unwrap_or_else(|| choice.label.clone())
}

fn known_variant_label(
    side: UiSpaceSide,
    role: UiSpaceCellRole,
    variant: &str,
) -> Option<&'static str> {
    match (side, role, variant) {
        (UiSpaceSide::Producer, UiSpaceCellRole::Primary, "OneD") => Some(SPACE_ONE_D),
        (UiSpaceSide::Producer, UiSpaceCellRole::Primary, "TwoD") => Some(SPACE_TWO_D),
        (UiSpaceSide::Consumer, UiSpaceCellRole::Primary, "Auto") => Some(CONSUME_AUTO),
        (UiSpaceSide::Consumer, UiSpaceCellRole::Primary, "Policy") => Some(CONSUME_POLICY),
        (_, UiSpaceCellRole::ProducerIn1d, "Default") => Some(PROJECTION_CENTRE_SCANLINE),
        (_, _, "Default") => Some(PROJECTION_DEFER),
        (_, _, "Extrude") => Some(PROJECTION_EXTRUDE),
        (_, _, "Radial") => Some(PROJECTION_RADIAL),
        (_, _, "Angular") => Some(PROJECTION_ANGULAR),
        (_, _, "Mirror") => Some(PROJECTION_MIRROR),
        _ => None,
    }
}

/// The active variant's label, from the cell's own selected choice (the
/// DTO's `active_label` is the derivation's vocabulary, not this file's).
fn active_variant_label(side: UiSpaceSide, cell: &UiSpaceCell) -> String {
    cell.choices
        .iter()
        .find(|choice| choice.selected)
        .map(|choice| variant_label(side, cell.role, choice))
        .unwrap_or_else(|| cell.active_label.clone())
}

fn projection_hint(projection: Option<UiCellProjection>) -> &'static str {
    match projection {
        None => HINT_DEFER,
        Some(UiCellProjection::Extrude) => HINT_EXTRUDE,
        Some(UiCellProjection::Radial) => HINT_RADIAL,
        Some(UiCellProjection::Angular) => HINT_ANGULAR,
        Some(UiCellProjection::Mirror) => HINT_MIRROR,
    }
}

/// What this side is saying, in one line.
fn primary_hint(section: &UiSpaceSection) -> &'static str {
    match section.side {
        UiSpaceSide::Producer => match section.declared_space {
            Some(UiVisualSpace::OneD) => PRODUCER_HINT_ONE_D,
            _ => PRODUCER_HINT_TWO_D,
        },
        UiSpaceSide::Consumer => {
            if section.primary.active == "Auto" {
                CONSUMER_HINT_AUTO
            } else {
                CONSUMER_HINT_POLICY
            }
        }
    }
}

/// The who-wins rung worth stating on this card. `None` where nothing can
/// contend: a 2D shader declares no 1D→2D cell, and an `Auto` fixture has
/// no opinion to lose with.
fn ladder_line(section: &UiSpaceSection) -> Option<&'static str> {
    match section.side {
        UiSpaceSide::Producer => section
            .cell(UiSpaceCellRole::ProducerIn2d)
            .map(|_| LADDER_PRODUCER),
        UiSpaceSide::Consumer => {
            let forced = section
                .flag(UiSpaceFlagRole::ForcePolicy)
                .is_some_and(|flag| flag.value);
            section.cell(UiSpaceCellRole::ConsumerFrom1d).map(|_| {
                if forced {
                    LADDER_CONSUMER_FORCES
                } else {
                    LADDER_CONSUMER_FILLS
                }
            })
        }
    }
}

/// The mismatch stated as the pair it is: what the project declares, and
/// what the GLSL actually defines.
fn mismatch_line(mismatch: &UiSpaceMismatch) -> String {
    format!(
        "declared {} · the code defines {}",
        visual_space_label(mismatch.declared),
        entry_label(mismatch.entry)
    )
}

fn entry_label(space: UiVisualSpace) -> &'static str {
    match space {
        UiVisualSpace::OneD => ENTRY_ONE_D,
        UiVisualSpace::TwoD => ENTRY_TWO_D,
    }
}

// ---------------------------------------------------------------------------
// Classes
// ---------------------------------------------------------------------------

const ROW_LABEL_CLASS: &str = "tw:w-28 tw:flex-none tw:text-[0.66rem] tw:font-bold tw:uppercase tw:leading-none tw:tracking-[0.08em] tw:text-subtle-foreground";

const HINT_CLASS: &str = "tw:m-0 tw:text-[11px] tw:leading-snug tw:text-dim-foreground";

const LADDER_CLASS: &str = "tw:m-0 tw:text-[11px] tw:leading-snug tw:text-subtle-foreground";

/// The field's trigger: no chrome of its own — the frame around it is the
/// visual, and the frame is the popover's outline anchor.
const FIELD_TRIGGER_CLASS: &str = "tw:flex tw:min-h-7 tw:w-full tw:min-w-0 tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-1.5 tw:border-0 tw:bg-transparent tw:px-2 tw:py-1 tw:text-left tw:text-sm tw:font-medium tw:text-muted-foreground";

/// The segmented group's frame. Mismatched declarations wear the error
/// border: the segment row IS the thing the compiler is objecting to.
fn segment_group_class(mismatched: bool) -> &'static str {
    if mismatched {
        "tw:inline-flex tw:overflow-hidden tw:rounded-xs tw:border tw:border-status-error-border"
    } else {
        "tw:inline-flex tw:overflow-hidden tw:rounded-xs tw:border tw:border-border-subtle"
    }
}

/// One squared block of the segmented row — pressed reads as filled, the
/// rest as quiet text (the discrete-control language, `docs/style/ui.md`).
fn segment_class(selected: bool) -> &'static str {
    if selected {
        "tw:cursor-pointer tw:appearance-none tw:border-0 tw:bg-card-muted tw:px-2.5 tw:py-1 tw:text-xs tw:font-bold tw:text-strong-foreground"
    } else {
        "tw:cursor-pointer tw:appearance-none tw:border-0 tw:bg-transparent tw:px-2.5 tw:py-1 tw:text-xs tw:font-bold tw:text-subtle-foreground tw:hover:text-soft-foreground"
    }
}

/// One tile of the picker grid.
fn tile_class(selected: bool) -> &'static str {
    if selected {
        "tw:grid tw:min-w-0 tw:cursor-pointer tw:appearance-none tw:gap-0.5 tw:rounded-xs tw:border tw:border-border-strong tw:bg-card-muted tw:p-1.5 tw:text-left tw:text-strong-foreground"
    } else {
        "tw:grid tw:min-w-0 tw:cursor-pointer tw:appearance-none tw:gap-0.5 tw:rounded-xs tw:border tw:border-border-subtle tw:bg-transparent tw:p-1.5 tw:text-left tw:text-muted-foreground tw:hover:border-border-strong tw:hover:text-strong-foreground"
    }
}

/// The squared checkbox box (no preflight: a `<button>`'s box is drawn
/// here or not at all).
fn checkbox_box_class(value: bool) -> &'static str {
    if value {
        "tw:inline-flex tw:h-3.5 tw:w-3.5 tw:flex-none tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-border-strong tw:bg-card-muted tw:text-strong-foreground"
    } else {
        "tw:inline-flex tw:h-3.5 tw:w-3.5 tw:flex-none tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page"
    }
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::{UiSlotFieldState, UiVisualProductSpace};

    use super::*;

    fn choice(variant: &str, selected: bool) -> UiSpaceChoice {
        UiSpaceChoice {
            variant: variant.to_string(),
            label: format!("dto:{variant}"),
            projection: None,
            selected,
        }
    }

    fn cell(role: UiSpaceCellRole, active: &str, variants: &[&str]) -> UiSpaceCell {
        UiSpaceCell {
            role,
            label: "row".to_string(),
            active: active.to_string(),
            active_label: format!("dto:{active}"),
            choices: variants
                .iter()
                .map(|variant| choice(variant, *variant == active))
                .collect(),
            address: None,
            state: UiSlotFieldState::editable(),
        }
    }

    fn producer(active: &str, cells: Vec<UiSpaceCell>) -> UiSpaceSection {
        UiSpaceSection {
            side: UiSpaceSide::Producer,
            primary: cell(UiSpaceCellRole::Primary, active, &["TwoD", "OneD"]),
            declared_space: Some(if active == "OneD" {
                UiVisualSpace::OneD
            } else {
                UiVisualSpace::TwoD
            }),
            cells,
            flags: Vec::new(),
            mismatch: None,
        }
    }

    fn consumer(active: &str, cells: Vec<UiSpaceCell>, force: Option<bool>) -> UiSpaceSection {
        UiSpaceSection {
            side: UiSpaceSide::Consumer,
            primary: cell(UiSpaceCellRole::Primary, active, &["Auto", "Policy"]),
            declared_space: None,
            cells,
            flags: force
                .map(|value| {
                    vec![UiSpaceFlag {
                        role: UiSpaceFlagRole::ForcePolicy,
                        label: "Force".to_string(),
                        value,
                        address: None,
                        state: UiSlotFieldState::editable(),
                    }]
                })
                .unwrap_or_default(),
            mismatch: None,
        }
    }

    /// `Default` is not one word: the same variant means "the consumer
    /// decides" on a 1D shader's 2D answer and "centre scanline" on a 2D
    /// shader's 1D one.
    #[test]
    fn default_reads_differently_per_cell() {
        assert_eq!(
            known_variant_label(
                UiSpaceSide::Producer,
                UiSpaceCellRole::ProducerIn2d,
                "Default"
            ),
            Some(PROJECTION_DEFER)
        );
        assert_eq!(
            known_variant_label(
                UiSpaceSide::Producer,
                UiSpaceCellRole::ProducerIn1d,
                "Default"
            ),
            Some(PROJECTION_CENTRE_SCANLINE)
        );
    }

    /// A variant this file's vocabulary has never heard of still renders —
    /// as the derivation's own label, never as a blank.
    #[test]
    fn an_unknown_variant_falls_back_to_the_dto_label() {
        let unknown = choice("Cylindrical", true);
        assert_eq!(
            variant_label(
                UiSpaceSide::Producer,
                UiSpaceCellRole::ProducerIn2d,
                &unknown
            ),
            "dto:Cylindrical"
        );
    }

    /// The primary segments speak each side's own vocabulary from the same
    /// component (D13's mirror).
    #[test]
    fn the_two_sides_share_one_row_shape_and_two_vocabularies() {
        let shader = producer("OneD", Vec::new());
        let fixture = consumer("Policy", Vec::new(), Some(false));
        assert_eq!(primary_label(shader.side), PRODUCER_PRIMARY_LABEL);
        assert_eq!(primary_label(fixture.side), CONSUMER_PRIMARY_LABEL);
        assert_eq!(
            active_variant_label(shader.side, &shader.primary),
            SPACE_ONE_D
        );
        assert_eq!(
            active_variant_label(fixture.side, &fixture.primary),
            CONSUME_POLICY
        );
    }

    /// The ladder states the rung that can still surprise: nothing where
    /// nothing contends, and the FORCED voice only when force is on.
    #[test]
    fn the_ladder_names_the_rung_that_can_surprise() {
        assert_eq!(ladder_line(&producer("TwoD", Vec::new())), None);
        assert_eq!(
            ladder_line(&producer(
                "OneD",
                vec![cell(UiSpaceCellRole::ProducerIn2d, "Radial", &["Radial"])]
            )),
            Some(LADDER_PRODUCER)
        );
        assert_eq!(ladder_line(&consumer("Auto", Vec::new(), None)), None);
        let policy = vec![cell(UiSpaceCellRole::ConsumerFrom1d, "Mirror", &["Mirror"])];
        assert_eq!(
            ladder_line(&consumer("Policy", policy.clone(), Some(false))),
            Some(LADDER_CONSUMER_FILLS)
        );
        assert_eq!(
            ladder_line(&consumer("Policy", policy, Some(true))),
            Some(LADDER_CONSUMER_FORCES)
        );
    }

    /// D15's captions, including D11's honesty rule: a projection nobody
    /// authored must never read like one somebody did.
    #[test]
    fn captions_name_the_space_the_projection_and_its_origin() {
        let native = UiVisualProductSpace {
            space: UiVisualSpace::OneD,
            projection: None,
            origin: None,
            primary: UiVisualSpace::OneD,
        };
        assert_eq!(
            preview_space_caption(UiVisualSpace::OneD, Some(native)),
            "native · 1D"
        );

        let declared = UiVisualProductSpace {
            space: UiVisualSpace::TwoD,
            projection: Some(UiCellProjection::Radial),
            origin: Some(UiProjectionOrigin::Declared),
            primary: UiVisualSpace::OneD,
        };
        assert_eq!(
            preview_space_caption(UiVisualSpace::TwoD, Some(declared)),
            "in 2D · radial (declared)"
        );

        let filled = UiVisualProductSpace {
            origin: Some(UiProjectionOrigin::ConsumerDefault),
            projection: Some(UiCellProjection::Extrude),
            ..declared
        };
        assert_eq!(
            preview_space_caption(UiVisualSpace::TwoD, Some(filled)),
            "in 2D · extrude (consumer default)"
        );

        let forced = UiVisualProductSpace {
            origin: Some(UiProjectionOrigin::Forced),
            ..filled
        };
        assert_eq!(
            preview_space_caption(UiVisualSpace::TwoD, Some(forced)),
            "in 2D · extrude (forced)"
        );

        // A 2D producer filling a 1D request: no cell to name, so the
        // caption says what actually happened.
        let scanline = UiVisualProductSpace {
            space: UiVisualSpace::OneD,
            projection: None,
            origin: None,
            primary: UiVisualSpace::TwoD,
        };
        assert_eq!(
            preview_space_caption(UiVisualSpace::OneD, Some(scanline)),
            "in 1D · centre scanline"
        );

        // Before any space-tagged result lands there is nothing to claim.
        assert_eq!(preview_space_caption(UiVisualSpace::TwoD, None), "2D");
    }

    /// The mismatch names BOTH sides — the declaration and the entry the
    /// GLSL actually defines (D1).
    #[test]
    fn the_mismatch_names_both_sides() {
        let mismatch = UiSpaceMismatch {
            declared: UiVisualSpace::OneD,
            entry: UiVisualSpace::TwoD,
            message: "shader compile: declared 1D but defines `render_2d`".to_string(),
        };
        assert_eq!(
            mismatch_line(&mismatch),
            "declared 1D · the code defines render_2d"
        );
        assert!(segment_group_class(true).contains("status-error-border"));
    }

    /// The header badge is the producer's declaration and nothing else: a
    /// fixture states a policy, so it has no space to badge.
    #[test]
    fn only_a_declared_space_earns_a_header_badge() {
        assert_eq!(space_badge(&producer("OneD", Vec::new())), Some("1D"));
        assert_eq!(space_badge(&producer("TwoD", Vec::new())), Some("2D"));
        // A fixture states a POLICY, so `declared_space` is `None` by
        // construction and the card wears no badge.
        assert_eq!(space_badge(&consumer("Policy", Vec::new(), None)), None);

        let mut shader = lpa_studio_core::UiShaderFace {
            preview: lpa_studio_core::UiProducedProduct::visual("output"),
            controls: Vec::new(),
            agent: None,
            code_drawer: None,
            space: Some(producer("OneD", Vec::new())),
        };
        assert_eq!(
            face_space_badge(&UiNodeFace::Shader(shader.clone())),
            Some("1D")
        );
        shader.space = None;
        assert_eq!(face_space_badge(&UiNodeFace::Shader(shader)), None);
    }
}
