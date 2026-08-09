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
    LpValue, ProjectSlotAddress, UiAction, UiCellProjection, UiMirrorDirection, UiNodeFace,
    UiProjectionDirection, UiProjectionOrigin, UiSpaceCell, UiSpaceCellRole, UiSpaceChoice,
    UiSpaceDirection, UiSpaceMismatch, UiSpaceSection, UiSpaceSide, UiVisualSpace,
};

use crate::app::node::slot_edit_actions::{slot_ensure_present_action, slot_set_value_action};
use crate::app::node::slot_fields::{field_class, field_wiring};
use crate::base::{
    PopoverButton, PopoverCloseHandle, PopoverPlacement, StudioIcon, StudioIconName,
    detail_popover_card_class,
};

// ---------------------------------------------------------------------------
// Wording (G1 ruled 2026-08-08; G1b rules on this pass — keep every
// user-facing string here)
// ---------------------------------------------------------------------------

/// The section's rail label. G1 sent "space" back ("it should have a very
/// clear 'Dimensionality'…"); G1b rules on this candidate.
pub(crate) const SPACE_SECTION_LABEL: &str = "dimensionality";

/// Per-other-dimension answer rows, in G1's "Show in 1d by:" shape.
const PRODUCER_IN_2D_LABEL: &str = "show in 2D by";
const PRODUCER_IN_1D_LABEL: &str = "show in 1D by";
/// The consumer's ONE dropdown (P4b — "then we just have one control").
const CONSUMER_PRIMARY_LABEL: &str = "show 1D sources by";

/// Variant vocabulary, keyed by role where one variant name reads
/// differently per cell.
const SPACE_ONE_D: &str = "1D";
const SPACE_TWO_D: &str = "2D";
/// Producer `Default` on the 2D answer: honest about what it resolves to.
/// `Auto` ≡ `Policy { from_1d: Extrude, force: false }` on the consumer
/// side, and the one dropdown's explicit picks always force — so a silent
/// declaration lands on extrude in every UI-reachable state. G1 ruling:
/// "there should always be a default… on the producer side"; "consumer
/// decides" is gone.
const PROJECTION_DEFAULT_EXTRUDE: &str = "extrude · default";
const PROJECTION_EXTRUDE: &str = "extrude";
const PROJECTION_RADIAL: &str = "radial";
const PROJECTION_ANGULAR: &str = "angular";
const PROJECTION_MIRROR: &str = "mirror";
const PROJECTION_CENTRE_SCANLINE: &str = "centre scanline";
/// The consumer dropdown's default entry.
const CONSUMER_FOLLOW: &str = "follow the source";
/// The consumer dropdown's FIRST entry — the `strip_order_meaningful` bit
/// absorbed into the one control (strip-order unification ruling: the
/// checkbox GATED the dropdown, so it became a choice of it). Vision D3's
/// scarf case, made pickable.
const CONSUMER_ALONG_WIRE: &str = "along the wire";
/// The synthetic variant ident the derivation emits for it (not a model
/// variant — the pick dispatches the bool `SetValue`, never an ensure).
const ALONG_WIRE_VARIANT: &str = "AlongWire";

/// The direction row under a directional shape (G1b ruling 4: "top
/// section is general shape, below that is direction").
const DIRECTION_ROW_LABEL: &str = "direction";

/// One direction segment's glyph and tooltip, by RAW variant ident —
/// per-shape vocabularies (mirror-direction ruling): single arrows for
/// extrude's run direction, PAIRED arrows for mirror's fold, and the
/// along-the-wire choice's forward/reversed pair.
fn direction_segment_face(variant: &str) -> (&'static str, &'static str) {
    match variant {
        "Right" => ("→", "left → right"),
        "Left" => ("←", "right → left"),
        "Down" => ("↓", "top → bottom"),
        "Up" => ("↑", "bottom → top"),
        "InwardX" => ("→←", "from both edges toward the centre"),
        "OutwardX" => ("←→", "from the centre toward both edges"),
        "InwardY" => ("↓↑", "from top and bottom toward the centre"),
        "OutwardY" => ("↑↓", "from the centre toward top and bottom"),
        "Forward" => ("→", "wire order, as wired"),
        "Reversed" => ("←", "wire order, reversed"),
        _ => ("·", "unknown direction"),
    }
}

/// One line per choice in the picker's tiles.
const HINT_DEFAULT_EXTRUDE: &str = "the standard projection, unless a fixture overrides";
const HINT_FOLLOW: &str = "each source projects the way it declares";
const HINT_ALONG_WIRE: &str = "run in wire order — the map doesn't apply";
const HINT_EXTRUDE: &str = "the strip, stretched down";
const HINT_RADIAL: &str = "the strip, out from the centre";
const HINT_ANGULAR: &str = "the strip, swept around";
const HINT_MIRROR: &str = "the strip, folded at the centre";
const HINT_CENTRE_SCANLINE: &str = "the texture's centre row, read as a strip";

/// What this side is saying, in one line under the primary row.
const PRODUCER_HINT_ONE_D: &str = "This shader renders along a strip.";
const PRODUCER_HINT_TWO_D: &str = "This shader renders in texture space.";
const CONSUMER_HINT_FOLLOW: &str = "1D sources project the way they declare.";
const CONSUMER_HINT_OVERRIDE: &str = "This fixture overrides what 1D sources declare.";
const CONSUMER_HINT_ALONG_WIRE: &str =
    "1D patterns run along the wire order — the map doesn't apply.";

/// The who-wins ladder (spike §3), compressed to the one rung that can
/// still surprise the person reading this card.
const LADDER_PRODUCER: &str = "A fixture that overrides its 1D sources wins over this.";

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

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-2 tw:px-4 tw:py-3",
            // Producer: the declaration leads as a tab-like segmented pair
            // (G1: "almost like tabs") — the section header carries the
            // word, so the control needs no row label of its own.
            // Consumer (P4b): the primary IS the one projection dropdown.
            match side {
                UiSpaceSide::Producer => rsx! {
                    SpaceSegments {
                        cell: section.primary.clone(),
                        side,
                        mismatched,
                        on_action,
                    }
                },
                UiSpaceSide::Consumer => rsx! {
                    SpaceCellRow {
                        cell: section.primary.clone(),
                        side,
                        picker_initially_open: picker_open_cell == Some(UiSpaceCellRole::Primary),
                        on_action,
                    }
                },
            }
            p { class: HINT_CLASS, "{primary_hint(&section)}" }
            for cell in section.cells.clone() {
                SpaceCellRow {
                    key: "{cell.role:?}",
                    cell: cell.clone(),
                    side,
                    picker_initially_open: picker_open_cell == Some(cell.role),
                    on_action,
                }
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

/// One answer cell: its label and the projection field (picker or
/// statement). The consumer's old inline `force` bit is gone (P4b): an
/// explicit pick IS the override, dispatched as part of the choice.
///
/// When the ACTIVE shape is directional the `direction` row renders
/// beneath the field (G1b ruling 4's second section) — shape first,
/// direction under it, never a flattened 8-tile grid.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SpaceCellRow(
    cell: UiSpaceCell,
    side: UiSpaceSide,
    #[props(default = false)] picker_initially_open: bool,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let direction = cell.direction.clone();
    rsx! {
        div { class: "tw:flex tw:min-w-0 tw:flex-wrap tw:items-center tw:gap-2",
            span { class: ROW_LABEL_CLASS, "{cell_label(side, cell.role)}" }
            ProjectionField {
                cell: cell.clone(),
                side,
                initially_open: picker_initially_open,
                on_action,
            }
        }
        if let Some(direction) = direction {
            SpaceDirectionRow { direction, on_action }
        }
    }
}

/// The direction row (G1b ruling 4): a segmented control drawing the
/// arrows, in the same squared-blocks discrete language as the 2D|1D
/// tabs. A pick dispatches whatever the row's backing slot already takes
/// — `EnsurePresent <direction row>.<D>` for an enum payload row,
/// `SetValue` for the along-the-wire bool — so the row stays a
/// presentation of the same write path.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SpaceDirectionRow(
    direction: UiSpaceDirection,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let active = direction.active.clone();
    let dispatch = direction.dispatch;
    let wiring = field_wiring(&direction.state, &direction.address, on_action);
    rsx! {
        div { class: "tw:flex tw:min-w-0 tw:flex-wrap tw:items-center tw:gap-2",
            span { class: ROW_LABEL_CLASS, "{DIRECTION_ROW_LABEL}" }
            span { class: DIRECTION_GROUP_CLASS,
                // The variant list comes from the backing row — each shape
                // brings its OWN vocabulary (extrude's run directions,
                // mirror's folds, the wire's forward/reversed), never a
                // hardcoded 4-way.
                for candidate in direction.variants.clone() {
                    if let Some((address, handler)) = wiring.clone() {
                        button {
                            key: "{candidate}",
                            class: direction_segment_class(candidate == active),
                            r#type: "button",
                            title: direction_segment_face(&candidate).1,
                            onclick: {
                                let selected = candidate == active;
                                let candidate = candidate.clone();
                                move |event: MouseEvent| {
                                    event.stop_propagation();
                                    if selected {
                                        return;
                                    }
                                    match dispatch {
                                        lpa_studio_core::UiSpaceDirectionDispatch::EnumVariant => {
                                            if let Some(target) = address.child_field(&candidate) {
                                                handler.call(slot_ensure_present_action(target));
                                            }
                                        }
                                        lpa_studio_core::UiSpaceDirectionDispatch::ReversedBool => {
                                            handler
                                                .call(
                                                    slot_set_value_action(
                                                        address.clone(),
                                                        LpValue::Bool(candidate == "Reversed"),
                                                    ),
                                                );
                                        }
                                    }
                                }
                            },
                            "{direction_segment_face(&candidate).0}"
                        }
                    } else {
                        span {
                            key: "{candidate}",
                            class: direction_segment_class(candidate == active),
                            title: direction_segment_face(&candidate).1,
                            "{direction_segment_face(&candidate).0}"
                        }
                    }
                }
            }
        }
    }
}

/// The op sequence one choice dispatches. Producer cells and the shader's
/// primary stay the single `EnsurePresent <enum>.<Variant>` the generic
/// variant field sends. The consumer's one dropdown (P4b + the
/// strip-order unification) fans out:
///
/// - `AlongWire` is the bool row's `SetValue strip_order = true` — the
///   consume policy is untouched (the true bit gates it anyway);
/// - `Auto` is `SetValue strip_order = false` + the plain
///   `EnsurePresent consume.Auto`;
/// - a projection is `SetValue strip_order = false` →
///   ensure-`Policy` → ensure-`from_1d.<V>` → set-`force = true`.
///
/// Each op is exactly what the drawer's own rows would send (structural
/// ensures order before assignments in the overlay), so the dropdown
/// remains a presentation of the same write path, never a second writer.
/// A projection or follow pick clears the bit because a true bit makes
/// the projection unreachable (`select_request_space`) — leaving it set
/// would render the pick a no-op.
fn choice_actions(
    side: UiSpaceSide,
    role: UiSpaceCellRole,
    address: &ProjectSlotAddress,
    strip_order: Option<&ProjectSlotAddress>,
    variant: &str,
) -> Vec<UiAction> {
    if side == UiSpaceSide::Consumer && role == UiSpaceCellRole::Primary {
        if variant == ALONG_WIRE_VARIANT {
            return strip_order
                .map(|strip| slot_set_value_action(strip.clone(), LpValue::Bool(true)))
                .into_iter()
                .collect();
        }
        let mut actions: Vec<UiAction> = strip_order
            .map(|strip| slot_set_value_action(strip.clone(), LpValue::Bool(false)))
            .into_iter()
            .collect();
        if variant == "Auto" {
            actions.extend(address.child_field("Auto").map(slot_ensure_present_action));
            return actions;
        }
        let Some(policy) = address.child_field("Policy") else {
            return actions;
        };
        actions.push(slot_ensure_present_action(policy.clone()));
        if let Some(target) = policy
            .child_field("from_1d")
            .and_then(|field| field.child_field(variant))
        {
            actions.push(slot_ensure_present_action(target));
        }
        if let Some(force) = policy.child_field("force") {
            actions.push(slot_set_value_action(force, LpValue::Bool(true)));
        }
        return actions;
    }
    address
        .child_field(variant)
        .map(slot_ensure_present_action)
        .into_iter()
        .collect()
}

/// The cell's control: an anchored tile picker when there is a real choice,
/// a read-only statement when there is not (`UiSpaceCell::is_choosable` —
/// the 2D→1D answer has one declared variant today, and a dropdown over one
/// option invites a gesture with nothing to change). The statement still
/// carries its glyph (G1: the text-only 2D→1D cell "feels odd").
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProjectionField(
    cell: UiSpaceCell,
    side: UiSpaceSide,
    #[props(default = false)] initially_open: bool,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    // The face wears the direction arrow beside the shape name (the
    // direction row beneath spells it out; the closed field should not
    // hide it).
    let active_label = format!(
        "{}{}",
        active_variant_label(side, &cell),
        directional_suffix(&cell)
    );
    let reachable = cell.is_choosable() && on_action.is_some();
    if !reachable {
        let kind = active_glyph(&cell);
        return rsx! {
            span { class: field_class(&cell.state),
                span { class: "tw:inline-flex tw:h-4 tw:w-6 tw:flex-none tw:items-center", aria_hidden: "true",
                    ProjectionGlyph { kind }
                }
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
                    strip_order: cell.strip_order.as_ref().and_then(|strip| strip.address.clone()),
                    on_action,
                }
            }
        }
    }
}

/// The closed field's face: the active choice's own glyph, its name, and
/// the caret that says a picker lives behind it. Rendered twice while the
/// popover is open (in-flow placeholder + top-layer copy), so it stays a
/// plain function of the cell.
fn projection_field_face(cell: &UiSpaceCell, active_label: &str) -> Element {
    let kind = active_glyph(cell);
    rsx! {
        span { class: "tw:inline-flex tw:h-4 tw:w-6 tw:flex-none tw:items-center", aria_hidden: "true",
            ProjectionGlyph { kind }
        }
        span { class: "tw:min-w-0 tw:grow tw:truncate", "{active_label}" }
        span { class: "tw:inline-flex tw:flex-none tw:text-subtle-foreground", aria_hidden: "true",
            StudioIcon { name: StudioIconName::Expanded, size: 12 }
        }
    }
}

/// The glyph for a cell's active choice, oriented to its active
/// direction.
fn active_glyph(cell: &UiSpaceCell) -> SpaceGlyph {
    glyph_with_active_direction(cell)
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
    /// The consumer cell's `strip_order_meaningful` row address — every
    /// consumer pick includes its `SetValue` (see [`choice_actions`]).
    #[props(default = None)]
    strip_order: Option<ProjectSlotAddress>,
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
                    title: "{choice_hint(role, &choice.variant)}",
                    onclick: {
                        let variant = choice.variant.clone();
                        let address = address.clone();
                        let strip_order = strip_order.clone();
                        let selected = choice.selected;
                        move |event: MouseEvent| {
                            event.stop_propagation();
                            if !selected
                                && let (Some(address), Some(handler)) = (address.clone(), on_action)
                            {
                                for action in choice_actions(
                                    side,
                                    role,
                                    &address,
                                    strip_order.as_ref(),
                                    &variant,
                                ) {
                                    handler.call(action);
                                }
                            }
                            if let Some(mut close) = close {
                                close.close();
                            }
                        }
                    },
                    span { class: "tw:block tw:h-10 tw:w-full tw:overflow-hidden tw:rounded-xs tw:bg-page",
                        ProjectionGlyph { kind: glyph_for(role, &choice.variant) }
                    }
                    span { class: "tw:min-w-0 tw:truncate tw:text-[11px] tw:font-bold",
                        "{variant_label(side, role, &choice)}"
                    }
                    span { class: "tw:min-w-0 tw:truncate tw:text-[10px] tw:leading-tight tw:text-dim-foreground",
                        "{choice_hint(role, &choice.variant)}"
                    }
                }
            }
        }
    }
}

/// What a tile or field glyph draws. Web-side vocabulary, deliberately
/// wider than [`UiCellProjection`]: the DTO says what a choice FORCES in a
/// probe; this says what the drawing shows — which lets the producer's
/// `Default` wear the extrude it resolves to and the 2D→1D statement draw
/// its centre scanline. The directional shapes carry their own direction
/// vocabulary (G1b ruling 4 + the mirror-direction ruling): the ramp
/// drawing follows it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SpaceGlyph {
    Extrude(UiProjectionDirection),
    Radial,
    Angular,
    Mirror(UiMirrorDirection),
    /// The 2D→1D answer: the texture's centre row, read as a strip.
    CentreScanline,
    /// The consumer dropdown's `Auto`: the answer lives on the source.
    FollowSource,
    /// The consumer dropdown's along-the-wire choice: the strip runs in
    /// wire order and the map does not apply — drawn as a serpentine wire
    /// carrying the ramp. `true` = reversed (the ramp runs the wire
    /// backwards).
    AlongWire(bool),
}

/// The drawing for one choice, by role and RAW variant name — the picker's
/// SHAPE tiles, which always wear each shape's DEFAULT direction (the
/// picker stays a 4-shape grid; the direction row under the field refines
/// it): extrude `Right`, mirror `OutwardX` — the folds a bare pick lands
/// on.
fn glyph_for(role: UiSpaceCellRole, variant: &str) -> SpaceGlyph {
    match variant {
        "Extrude" => SpaceGlyph::Extrude(UiProjectionDirection::Right),
        "Radial" => SpaceGlyph::Radial,
        "Angular" => SpaceGlyph::Angular,
        "Mirror" => SpaceGlyph::Mirror(UiMirrorDirection::OutwardX),
        "Auto" => SpaceGlyph::FollowSource,
        ALONG_WIRE_VARIANT => SpaceGlyph::AlongWire(false),
        "Default" => match role {
            // A 2D shader's 1D answer IS the centre scanline.
            UiSpaceCellRole::ProducerIn1d => SpaceGlyph::CentreScanline,
            // A 1D shader's silent 2D answer resolves to extrude in every
            // UI-reachable state (see PROJECTION_DEFAULT_EXTRUDE).
            _ => SpaceGlyph::Extrude(UiProjectionDirection::Right),
        },
        _ => SpaceGlyph::FollowSource,
    }
}

/// The FIELD's active glyph: the shape's drawing, oriented by the
/// direction row's active variant — each shape parses its OWN vocabulary
/// (extrude a run direction, mirror a fold).
fn glyph_with_active_direction(cell: &UiSpaceCell) -> SpaceGlyph {
    let active_direction = cell.direction.as_ref().map(|row| row.active.as_str());
    match (cell.active.as_str(), active_direction) {
        ("Extrude", Some(ident)) => SpaceGlyph::Extrude(UiProjectionDirection::from_variant(ident)),
        ("Mirror", Some(ident)) => SpaceGlyph::Mirror(UiMirrorDirection::from_variant(ident)),
        (ALONG_WIRE_VARIANT, Some(ident)) => SpaceGlyph::AlongWire(ident == "Reversed"),
        _ => glyph_for(cell.role, &cell.active),
    }
}

/// A schematic drawing of what one choice does to a 1D source.
///
/// **Not a live probe (plan A2, resolved; G1 ratified glyphs).** A live
/// tile means rendering THIS product through a forced policy, and nothing
/// in `lpa-studio-web` can issue a probe of its own — the tiles draw the
/// SHAPE of each answer instead.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProjectionGlyph(kind: SpaceGlyph) -> Element {
    // One vocabulary across the set: the source strip is a light-to-dark
    // ramp, and the glyph shows where that ramp goes.
    rsx! {
        svg {
            class: "tw:block tw:h-full tw:w-full tw:text-soft-foreground",
            view_box: "0 0 64 40",
            preserve_aspect_ratio: "none",
            role: "img",
            match kind {
                // The directional pair: the ramp's bands run along the
                // direction — `Right` is the original left→right drawing,
                // the other three are the same ramp re-laid (G1b ruling 4
                // retired the story-only Y-twin drawings for this).
                SpaceGlyph::Extrude(direction) => rsx! {
                    for (index , (x , y , width , height , opacity)) in ramp_bands(&RAMP, direction)
                        .into_iter()
                        .enumerate()
                    {
                        rect {
                            key: "{index}",
                            x: "{x}",
                            y: "{y}",
                            width: "{width}",
                            height: "{height}",
                            fill: "currentColor",
                            fill_opacity: "{opacity}",
                        }
                    }
                },
                SpaceGlyph::Mirror(direction) => rsx! {
                    for (index , (x , y , width , height , opacity)) in mirror_bands(direction)
                        .into_iter()
                        .enumerate()
                    {
                        rect {
                            key: "{index}",
                            x: "{x}",
                            y: "{y}",
                            width: "{width}",
                            height: "{height}",
                            fill: "currentColor",
                            fill_opacity: "{opacity}",
                        }
                    }
                },
                SpaceGlyph::Radial => rsx! {
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
                // The strip swept around the centre: adjacent pie sectors
                // whose opacity ramps with angle — a conic sweep. (G1: the
                // old ray spokes read as an asterisk.)
                SpaceGlyph::Angular => rsx! {
                    for (index , (path , opacity)) in angular_sectors().into_iter().enumerate() {
                        path {
                            key: "{index}",
                            d: "{path}",
                            fill: "currentColor",
                            fill_opacity: "{opacity}",
                        }
                    }
                },
                // The texture's centre row read as a strip: the 2D field
                // faint, the scanline bright and carrying the ramp.
                SpaceGlyph::CentreScanline => rsx! {
                    rect {
                        x: "1",
                        y: "1",
                        width: "62",
                        height: "38",
                        fill: "currentColor",
                        fill_opacity: "0.1",
                    }
                    for (index , opacity) in RAMP.iter().copied().enumerate() {
                        rect {
                            key: "{index}",
                            x: "{index * 8}",
                            y: "16",
                            width: "8",
                            height: "8",
                            fill: "currentColor",
                            fill_opacity: "{opacity}",
                        }
                    }
                },
                // Along the wire: a serpentine wire path carrying the
                // ramp — the strip runs in wire order, folding back on
                // itself, and the map never enters into it.
                SpaceGlyph::AlongWire(reversed) => rsx! {
                    for (index , (x , y , width , height , opacity)) in serpentine_segments(reversed)
                        .into_iter()
                        .enumerate()
                    {
                        rect {
                            key: "{index}",
                            x: "{x}",
                            y: "{y}",
                            width: "{width}",
                            height: "{height}",
                            fill: "currentColor",
                            fill_opacity: "{opacity}",
                        }
                    }
                },
                // "Follow the source": a dashed hollow — the answer lives
                // on the other side of the binding.
                SpaceGlyph::FollowSource => rsx! {
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
/// The ramp folded INWARD: the strip runs from both edges toward the
/// centre, so its bright end lands in the middle.
const MIRROR_INWARD_RAMP: [f32; 8] = [0.14, 0.36, 0.6, 0.96, 0.96, 0.6, 0.36, 0.14];
/// The ramp folded OUTWARD (today's mirror behavior, `|2s−1|`): the strip
/// runs from the centre toward both edges, bright ends outside.
const MIRROR_OUTWARD_RAMP: [f32; 8] = [0.96, 0.6, 0.36, 0.14, 0.14, 0.36, 0.6, 0.96];

/// The eight ramp bands laid along a direction, as `(x, y, width, height,
/// opacity)` rects in the glyph's 64×40 box: `Right` is the original
/// left→right columns, `Left` reverses them, `Down`/`Up` run the rows.
fn ramp_bands(ramp: &[f32; 8], direction: UiProjectionDirection) -> Vec<(u32, u32, u32, u32, f32)> {
    (0..8u32)
        .map(|index| {
            let opacity = ramp[index as usize];
            match direction {
                UiProjectionDirection::Right => (index * 8, 0, 8, 40, opacity),
                UiProjectionDirection::Left => ((7 - index) * 8, 0, 8, 40, opacity),
                UiProjectionDirection::Down => (0, index * 5, 64, 5, opacity),
                UiProjectionDirection::Up => (0, (7 - index) * 5, 64, 5, opacity),
            }
        })
        .collect()
}

/// A mirror fold's eight bands: the fold's SENSE picks which folded ramp
/// (inward = bright centre, outward = bright edges — outward-x is the
/// pre-direction drawing corrected to match the math), its AXIS whether
/// the bands run the columns or the rows.
fn mirror_bands(direction: UiMirrorDirection) -> Vec<(u32, u32, u32, u32, f32)> {
    match direction {
        UiMirrorDirection::InwardX => ramp_bands(&MIRROR_INWARD_RAMP, UiProjectionDirection::Right),
        UiMirrorDirection::OutwardX => {
            ramp_bands(&MIRROR_OUTWARD_RAMP, UiProjectionDirection::Right)
        }
        UiMirrorDirection::InwardY => ramp_bands(&MIRROR_INWARD_RAMP, UiProjectionDirection::Down),
        UiMirrorDirection::OutwardY => {
            ramp_bands(&MIRROR_OUTWARD_RAMP, UiProjectionDirection::Down)
        }
    }
}
/// The serpentine wire's segments: three horizontal runs (left→right,
/// right→left, left→right) joined by end connectors, the ramp's opacity
/// climbing along the WIRE path — which is the whole statement: position
/// along the wire is the only coordinate this choice reads.
fn serpentine_segments(reversed: bool) -> Vec<(u32, u32, u32, u32, f32)> {
    // (x, y, w, h) boxes laid along the path; opacity ramps by index.
    const BOXES: [(u32, u32, u32, u32); 14] = [
        // run 1, left → right (y = 2)
        (2, 2, 14, 8),
        (16, 2, 14, 8),
        (30, 2, 14, 8),
        (44, 2, 14, 8),
        // right connector down
        (54, 10, 8, 6),
        // run 2, right → left (y = 16)
        (48, 16, 14, 8),
        (34, 16, 14, 8),
        (20, 16, 14, 8),
        (6, 16, 14, 8),
        // left connector down
        (2, 24, 8, 6),
        // run 3, left → right (y = 30)
        (2, 30, 14, 8),
        (16, 30, 14, 8),
        (30, 30, 14, 8),
        (44, 30, 14, 8),
    ];
    BOXES
        .into_iter()
        .enumerate()
        .map(|(index, (x, y, width, height))| {
            let step = if reversed {
                BOXES.len() - 1 - index
            } else {
                index
            };
            let opacity = 0.14 + 0.82 * (step as f32) / ((BOXES.len() - 1) as f32);
            (x, y, width, height, opacity)
        })
        .collect()
}

/// Concentric rings, outermost first so the inner ones paint over.
const RADIAL_RINGS: [(u32, f32); 4] = [(26, 0.18), (19, 0.38), (12, 0.62), (5, 0.95)];

/// Twelve adjacent pie sectors around (32, 20), radius 30, opacity
/// ramping with angle — the conic sweep the angular projection actually
/// performs. Computed rather than tabulated: twelve hand-written arc
/// paths would hide the one fact that matters (adjacent sectors, one
/// ramp).
fn angular_sectors() -> Vec<(String, f32)> {
    const SECTORS: usize = 12;
    const CX: f32 = 32.0;
    const CY: f32 = 20.0;
    const R: f32 = 30.0;
    (0..SECTORS)
        .map(|index| {
            let start = (index as f32) / (SECTORS as f32) * core::f32::consts::TAU;
            let end = ((index + 1) as f32) / (SECTORS as f32) * core::f32::consts::TAU;
            let (x0, y0) = (CX + R * start.cos(), CY + R * start.sin());
            let (x1, y1) = (CX + R * end.cos(), CY + R * end.sin());
            let path = format!("M {CX} {CY} L {x0:.1} {y0:.1} A {R} {R} 0 0 1 {x1:.1} {y1:.1} Z");
            let opacity = 0.14 + 0.82 * (index as f32) / ((SECTORS - 1) as f32);
            (path, opacity)
        })
        .collect()
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

/// A projection's caption: the shape name, plus its direction glyph when
/// a directional shape runs anywhere but its default (`extrude ←`,
/// `mirror →←`). Each default stays bare because it IS the
/// pre-directional behavior and the captions around it predate glyphs.
fn projection_label(projection: UiCellProjection) -> String {
    match projection {
        UiCellProjection::Extrude(direction) => {
            if direction == UiProjectionDirection::Right {
                PROJECTION_EXTRUDE.to_string()
            } else {
                format!("{PROJECTION_EXTRUDE} {}", direction.arrow())
            }
        }
        UiCellProjection::Radial => PROJECTION_RADIAL.to_string(),
        UiCellProjection::Angular => PROJECTION_ANGULAR.to_string(),
        UiCellProjection::Mirror(direction) => {
            if direction == UiMirrorDirection::OutwardX {
                PROJECTION_MIRROR.to_string()
            } else {
                format!("{PROJECTION_MIRROR} {}", direction.arrows())
            }
        }
    }
}

/// The ` ↓` / ` →←` a cell's field face and drawer summary append when
/// its active shape runs anywhere but its default — empty otherwise, so
/// the defaults keep reading exactly as they did before directions
/// existed.
fn directional_suffix(cell: &UiSpaceCell) -> String {
    let Some(row) = cell.direction.as_ref() else {
        return String::new();
    };
    match cell.active.as_str() {
        "Extrude" => {
            let direction = UiProjectionDirection::from_variant(&row.active);
            if direction == UiProjectionDirection::Right {
                String::new()
            } else {
                format!(" {}", direction.arrow())
            }
        }
        "Mirror" => {
            let direction = UiMirrorDirection::from_variant(&row.active);
            if direction == UiMirrorDirection::OutwardX {
                String::new()
            } else {
                format!(" {}", direction.arrows())
            }
        }
        // The along-the-wire choice: a reversed wire wears the back
        // arrow; forward (the default) stays bare like every default.
        ALONG_WIRE_VARIANT => {
            if row.active == "Reversed" {
                " ←".to_string()
            } else {
                String::new()
            }
        }
        _ => String::new(),
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
        None => PROJECTION_CENTRE_SCANLINE.to_string(),
    };
    match meta.origin {
        Some(origin) => format!(
            "{CAPTION_IN} {space_label} · {how} ({})",
            origin_label(origin)
        ),
        None => format!("{CAPTION_IN} {space_label} · {how}"),
    }
}

fn cell_label(side: UiSpaceSide, role: UiSpaceCellRole) -> &'static str {
    match (side, role) {
        (_, UiSpaceCellRole::ProducerIn2d) => PRODUCER_IN_2D_LABEL,
        (_, UiSpaceCellRole::ProducerIn1d) => PRODUCER_IN_1D_LABEL,
        // The consumer's primary IS the one dropdown (P4b).
        (_, UiSpaceCellRole::Primary) => CONSUMER_PRIMARY_LABEL,
    }
}

/// A variant's display name.
///
/// Keyed by ROLE, not by variant alone: `Default` means "extrude ·
/// default" on a 1D shader's 2D answer (the projection silence resolves
/// to) and "centre scanline" on a 2D shader's 1D one, and one vocabulary
/// for both would lie about one of them. Anything the vocabulary does not
/// know falls back to the DTO's own label, so a variant added to the
/// model still renders something honest.
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
        (UiSpaceSide::Consumer, UiSpaceCellRole::Primary, "Auto") => Some(CONSUMER_FOLLOW),
        (UiSpaceSide::Consumer, UiSpaceCellRole::Primary, ALONG_WIRE_VARIANT) => {
            Some(CONSUMER_ALONG_WIRE)
        }
        (_, UiSpaceCellRole::ProducerIn1d, "Default") => Some(PROJECTION_CENTRE_SCANLINE),
        (_, _, "Default") => Some(PROJECTION_DEFAULT_EXTRUDE),
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

/// One line per choice, by role and RAW variant name.
fn choice_hint(role: UiSpaceCellRole, variant: &str) -> &'static str {
    match variant {
        "Auto" => HINT_FOLLOW,
        ALONG_WIRE_VARIANT => HINT_ALONG_WIRE,
        "Default" => match role {
            UiSpaceCellRole::ProducerIn1d => HINT_CENTRE_SCANLINE,
            _ => HINT_DEFAULT_EXTRUDE,
        },
        "Extrude" => HINT_EXTRUDE,
        "Radial" => HINT_RADIAL,
        "Angular" => HINT_ANGULAR,
        "Mirror" => HINT_MIRROR,
        _ => "",
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
            if section.primary.active == ALONG_WIRE_VARIANT {
                CONSUMER_HINT_ALONG_WIRE
            } else if section.primary.active == "Auto" {
                CONSUMER_HINT_FOLLOW
            } else {
                CONSUMER_HINT_OVERRIDE
            }
        }
    }
}

/// The who-wins rung worth stating on this card. Only the producer still
/// carries one: the consumer's hint line already says whether it follows
/// or overrides, which was the whole of its ladder.
fn ladder_line(section: &UiSpaceSection) -> Option<&'static str> {
    match section.side {
        UiSpaceSide::Producer => section
            .cell(UiSpaceCellRole::ProducerIn2d)
            .map(|_| LADDER_PRODUCER),
        UiSpaceSide::Consumer => None,
    }
}

/// The collapsed drawer row's summary (P4b: both sections are drawers
/// below `settings` now) — the declaration at a glance.
pub(crate) fn space_section_summary(section: &UiSpaceSection) -> String {
    match section.side {
        UiSpaceSide::Producer => {
            let space = section
                .declared_space
                .map(visual_space_label)
                .unwrap_or_default();
            if let Some(cell) = section.cell(UiSpaceCellRole::ProducerIn2d) {
                format!(
                    "{space} · in 2D: {}{}",
                    active_variant_label(section.side, cell),
                    directional_suffix(cell)
                )
            } else if section.cell(UiSpaceCellRole::ProducerIn1d).is_some() {
                format!("{space} · in 1D: {PROJECTION_CENTRE_SCANLINE}")
            } else {
                space.to_string()
            }
        }
        UiSpaceSide::Consumer => {
            if section.primary.active == ALONG_WIRE_VARIANT {
                format!(
                    "{CONSUMER_ALONG_WIRE}{}",
                    directional_suffix(&section.primary)
                )
            } else if section.primary.active == "Auto" {
                CONSUMER_FOLLOW.to_string()
            } else {
                format!(
                    "1D sources: {}{} (override)",
                    active_variant_label(section.side, &section.primary),
                    directional_suffix(&section.primary)
                )
            }
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

/// The segmented group's frame — full-width since P4b (G1: "almost like
/// tabs"; the declaration is the section's headline, not one row among
/// many). Mismatched declarations wear the error border: the segment row
/// IS the thing the compiler is objecting to.
fn segment_group_class(mismatched: bool) -> &'static str {
    if mismatched {
        "tw:flex tw:w-full tw:overflow-hidden tw:rounded-xs tw:border tw:border-status-error-border"
    } else {
        "tw:flex tw:w-full tw:overflow-hidden tw:rounded-xs tw:border tw:border-border-subtle"
    }
}

/// One tab of the segmented row — pressed reads as filled, the rest as
/// quiet text (the discrete-control language, `docs/style/ui.md`), each
/// taking an equal share of the row.
fn segment_class(selected: bool) -> &'static str {
    if selected {
        "tw:flex-1 tw:cursor-pointer tw:appearance-none tw:border-0 tw:bg-card-muted tw:px-2.5 tw:py-1.5 tw:text-center tw:text-xs tw:font-bold tw:text-strong-foreground"
    } else {
        "tw:flex-1 tw:cursor-pointer tw:appearance-none tw:border-0 tw:bg-transparent tw:px-2.5 tw:py-1.5 tw:text-center tw:text-xs tw:font-bold tw:text-subtle-foreground tw:hover:text-soft-foreground"
    }
}

/// The direction segmented control's frame — the squared-blocks discrete
/// language, compact (four fixed squares, not a full-width tab row).
const DIRECTION_GROUP_CLASS: &str =
    "tw:inline-flex tw:overflow-hidden tw:rounded-xs tw:border tw:border-border-subtle";

/// One segment of the direction control: pressed reads as filled, the
/// rest as quiet arrows (the same language as [`segment_class`], sized to
/// its glyph).
fn direction_segment_class(selected: bool) -> &'static str {
    if selected {
        "tw:inline-flex tw:h-7 tw:w-9 tw:cursor-pointer tw:appearance-none tw:items-center tw:justify-center tw:border-0 tw:bg-card-muted tw:text-xs tw:font-bold tw:text-strong-foreground"
    } else {
        "tw:inline-flex tw:h-7 tw:w-9 tw:cursor-pointer tw:appearance-none tw:items-center tw:justify-center tw:border-0 tw:bg-transparent tw:text-xs tw:font-bold tw:text-subtle-foreground tw:hover:text-soft-foreground"
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
            direction: None,
            strip_order: None,
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
            mismatch: None,
        }
    }

    /// The unified consumer section: ONE dropdown whose first choice is
    /// `along the wire` (the strip-order bit), then follow/projections.
    fn consumer(active: &str) -> UiSpaceSection {
        UiSpaceSection {
            side: UiSpaceSide::Consumer,
            primary: cell(
                UiSpaceCellRole::Primary,
                active,
                &[
                    ALONG_WIRE_VARIANT,
                    "Auto",
                    "Extrude",
                    "Radial",
                    "Angular",
                    "Mirror",
                ],
            ),
            declared_space: None,
            cells: Vec::new(),
            mismatch: None,
        }
    }

    /// `Default` is not one word: the same variant means "extrude ·
    /// default" on a 1D shader's 2D answer (the projection silence
    /// actually resolves to — "consumer decides" was killed at G1) and
    /// "centre scanline" on a 2D shader's 1D one.
    #[test]
    fn default_reads_differently_per_cell() {
        assert_eq!(
            known_variant_label(
                UiSpaceSide::Producer,
                UiSpaceCellRole::ProducerIn2d,
                "Default"
            ),
            Some(PROJECTION_DEFAULT_EXTRUDE)
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

    /// The glyphs match the labels' honesty rules: producer `Default`
    /// wears the extrude it resolves to, the 2D→1D statement draws its
    /// scanline, and the consumer's `Auto` defers visually.
    #[test]
    fn glyphs_follow_the_same_honesty_rules_as_labels() {
        assert_eq!(
            glyph_for(UiSpaceCellRole::ProducerIn2d, "Default"),
            SpaceGlyph::Extrude(UiProjectionDirection::Right)
        );
        assert_eq!(
            glyph_for(UiSpaceCellRole::ProducerIn1d, "Default"),
            SpaceGlyph::CentreScanline
        );
        assert_eq!(
            glyph_for(UiSpaceCellRole::Primary, "Auto"),
            SpaceGlyph::FollowSource
        );
        assert_eq!(
            glyph_for(UiSpaceCellRole::Primary, "Mirror"),
            SpaceGlyph::Mirror(UiMirrorDirection::OutwardX),
            "the mirror tile wears the default fold — the one a bare pick lands on"
        );
    }

    /// The caption rule (G1b ruling 4 + the mirror-direction ruling): a
    /// directional shape at its DEFAULT reads bare (it IS the
    /// pre-directional behavior); anywhere else it wears its own glyph —
    /// single arrows for extrude, paired arrows for mirror's fold.
    #[test]
    fn directional_captions_wear_their_glyph_except_at_the_default() {
        assert_eq!(
            projection_label(UiCellProjection::Extrude(UiProjectionDirection::Right)),
            "extrude"
        );
        assert_eq!(
            projection_label(UiCellProjection::Extrude(UiProjectionDirection::Left)),
            "extrude ←"
        );
        assert_eq!(
            projection_label(UiCellProjection::Mirror(UiMirrorDirection::OutwardX)),
            "mirror",
            "outward-x IS the pre-direction behavior, so it stays bare"
        );
        assert_eq!(
            projection_label(UiCellProjection::Mirror(UiMirrorDirection::InwardX)),
            "mirror →←"
        );
        assert_eq!(
            projection_label(UiCellProjection::Mirror(UiMirrorDirection::OutwardY)),
            "mirror ↑↓"
        );
        assert_eq!(projection_label(UiCellProjection::Radial), "radial");
    }

    /// The drawer summary and the active glyph follow the direction row —
    /// each shape through its OWN vocabulary: `1D · in 2D: mirror ↓↑`,
    /// glyph folded to match; a default row adds nothing.
    #[test]
    fn summaries_and_glyphs_follow_the_active_direction() {
        let directed = |active: &str, variants: &[&str]| lpa_studio_core::UiSpaceDirection {
            active: active.to_string(),
            variants: variants.iter().map(|ident| ident.to_string()).collect(),
            address: None,
            state: UiSlotFieldState::editable(),
            dispatch: lpa_studio_core::UiSpaceDirectionDispatch::EnumVariant,
        };
        const FOLDS: [&str; 4] = ["InwardX", "OutwardX", "InwardY", "OutwardY"];
        let mut answer = cell(
            UiSpaceCellRole::ProducerIn2d,
            "Mirror",
            &["Default", "Extrude", "Radial", "Angular", "Mirror"],
        );
        answer.direction = Some(directed("InwardY", &FOLDS));
        assert_eq!(
            active_glyph(&answer),
            SpaceGlyph::Mirror(UiMirrorDirection::InwardY)
        );
        let shader = producer("OneD", vec![answer.clone()]);
        assert_eq!(space_section_summary(&shader), "1D · in 2D: mirror ↓↑");

        answer.direction = Some(directed("OutwardX", &FOLDS));
        let shader = producer("OneD", vec![answer]);
        assert_eq!(
            space_section_summary(&shader),
            "1D · in 2D: mirror",
            "the default fold keeps the pre-directional reading"
        );

        let mut fixture = consumer("Extrude");
        fixture.primary.direction = Some(directed("Left", &["Right", "Left", "Down", "Up"]));
        assert_eq!(
            space_section_summary(&fixture),
            "1D sources: extrude ← (override)"
        );
    }

    /// The consumer dropdown's dispatch (strip-order unification):
    /// `along the wire` is the bool SetValue alone; `Auto` clears the bit
    /// and ensures `consume.Auto`; a projection clears the bit and runs
    /// the ensure-Policy → ensure-variant → force=true sequence (the pick
    /// IS the override, and a set bit would gate it off).
    #[test]
    fn consumer_choices_dispatch_the_op_sequence() {
        use lpa_studio_core::{ProjectNodeAddress, ProjectSlotRoot};
        let node = ProjectNodeAddress::parse("/demo.module/panel.fixture").expect("address");
        let address = ProjectSlotAddress::new(
            node.clone(),
            ProjectSlotRoot::def(),
            lpc_model::SlotPath::parse("consume").expect("path"),
        );
        let strip = ProjectSlotAddress::new(
            node,
            ProjectSlotRoot::def(),
            lpc_model::SlotPath::parse("strip_order_meaningful").expect("path"),
        );

        let along = choice_actions(
            UiSpaceSide::Consumer,
            UiSpaceCellRole::Primary,
            &address,
            Some(&strip),
            ALONG_WIRE_VARIANT,
        );
        assert_eq!(along.len(), 1, "one SetValue — consume is untouched");

        let auto = choice_actions(
            UiSpaceSide::Consumer,
            UiSpaceCellRole::Primary,
            &address,
            Some(&strip),
            "Auto",
        );
        assert_eq!(auto.len(), 2, "clear the bit, ensure consume.Auto");

        let mirror = choice_actions(
            UiSpaceSide::Consumer,
            UiSpaceCellRole::Primary,
            &address,
            Some(&strip),
            "Mirror",
        );
        assert_eq!(
            mirror.len(),
            4,
            "clear the bit, ensure Policy, ensure from_1d.Mirror, set force"
        );

        // Without a strip row there is nothing to clear — the
        // pre-unification sequences remain.
        let auto = choice_actions(
            UiSpaceSide::Consumer,
            UiSpaceCellRole::Primary,
            &address,
            None,
            "Auto",
        );
        assert_eq!(auto.len(), 1);

        // Producer cells keep the single generic gesture.
        let producer = choice_actions(
            UiSpaceSide::Producer,
            UiSpaceCellRole::ProducerIn2d,
            &address,
            None,
            "Radial",
        );
        assert_eq!(producer.len(), 1);
    }

    /// The collapsed drawer summaries say the declaration at a glance.
    #[test]
    fn summaries_state_the_declaration_at_a_glance() {
        let shader = producer(
            "OneD",
            vec![cell(
                UiSpaceCellRole::ProducerIn2d,
                "Radial",
                &["Default", "Extrude", "Radial", "Angular", "Mirror"],
            )],
        );
        assert_eq!(space_section_summary(&shader), "1D · in 2D: radial");
        assert_eq!(space_section_summary(&producer("TwoD", Vec::new())), "2D");
        assert_eq!(space_section_summary(&consumer("Auto")), CONSUMER_FOLLOW);
        assert_eq!(
            space_section_summary(&consumer("Mirror")),
            "1D sources: mirror (override)"
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

    /// The two sides keep their own vocabulary through the shared shapes
    /// (D13's mirror): the producer's tabs speak spaces, the consumer's
    /// dropdown speaks follow-or-override.
    #[test]
    fn the_two_sides_share_one_shape_and_two_vocabularies() {
        let shader = producer("OneD", Vec::new());
        let fixture = consumer("Auto");
        assert_eq!(
            active_variant_label(shader.side, &shader.primary),
            SPACE_ONE_D
        );
        assert_eq!(
            active_variant_label(fixture.side, &fixture.primary),
            CONSUMER_FOLLOW
        );
        assert_eq!(
            cell_label(UiSpaceSide::Consumer, UiSpaceCellRole::Primary),
            CONSUMER_PRIMARY_LABEL
        );
        assert_eq!(primary_hint(&fixture), CONSUMER_HINT_FOLLOW);
        assert_eq!(primary_hint(&consumer("Radial")), CONSUMER_HINT_OVERRIDE);
    }

    /// The ladder states the one rung that can still surprise, and only
    /// where something contends: the consumer's hint line covers its side.
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
        assert_eq!(ladder_line(&consumer("Auto")), None);
        assert_eq!(ladder_line(&consumer("Mirror")), None);
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
            projection: Some(UiCellProjection::Extrude(UiProjectionDirection::Right)),
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
        assert_eq!(space_badge(&consumer("Auto")), None);

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
