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

use dioxus::prelude::*;
use lpa_studio_core::{
    LpValue, ProjectSlotAddress, UiAction, UiCellProjection, UiNodeFace, UiProjectionOrigin,
    UiProjectionShape, UiSpaceBoolRow, UiSpaceCell, UiSpaceCellRole, UiSpaceChoice,
    UiSpaceMismatch, UiSpaceModifiers, UiSpaceSection, UiSpaceSide, UiVisualSpace,
    UiWireDirectionRow,
};

use crate::app::node::slot_edit_actions::{slot_ensure_present_action, slot_set_value_action};
use crate::app::node::slot_fields::{field_class, field_wiring};
use crate::base::{StudioIcon, StudioIconName};

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
/// The factored shape vocabulary (THE FACTORIZATION): four base shapes,
/// refined by the two modifier toggles beneath the tiles.
const SHAPE_EXTRUDE_X: &str = "extrude-x";
const SHAPE_EXTRUDE_Y: &str = "extrude-y";
const SHAPE_RADIAL: &str = "radial";
const SHAPE_ANGULAR: &str = "angular";
/// The modifier words, as the toggles label themselves and as captions
/// spell them (`extrude-x · mirrored · flipped`).
const MODIFIER_MIRROR: &str = "mirror";
const MODIFIER_FLIP: &str = "flip";
const MODIFIER_MIRRORED: &str = "mirrored";
const MODIFIER_FLIPPED: &str = "flipped";
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

/// The modifier tiles' hint lines (the same uniform chain the engine
/// runs: mirror folds first, flip reverses after).
const MIRROR_TITLE: &str = "fold at the middle — out and back";
const FLIP_TITLE: &str = "reverse the strip";
/// The along-the-wire direction tiles.
const WIRE_FORWARD: &str = "forward";
const WIRE_REVERSED: &str = "reversed";
const HINT_WIRE_FORWARD: &str = "wire order, as wired";
const HINT_WIRE_REVERSED: &str = "wire order, read back to front";

/// One line per choice in the picker's tiles.
const HINT_FOLLOW: &str = "each source projects the way it declares";
const HINT_ALONG_WIRE: &str = "run in wire order — the map doesn't apply";
const HINT_EXTRUDE_X: &str = "the strip, stretched down";
const HINT_EXTRUDE_Y: &str = "the strip, stretched across";
const HINT_RADIAL: &str = "the strip, out from the centre";
const HINT_ANGULAR: &str = "the strip, swept around";
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
const ORIGIN_FORCED: &str = "forced";

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn SpaceSection(
    section: UiSpaceSection,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let side = section.side;
    let mismatched = section.mismatch.is_some();

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-2 tw:px-4 tw:py-3",
            // Producer: the declaration leads as a tab-like segmented pair
            // (G1: "almost like tabs") — the section header carries the
            // word, so the control needs no row label of its own.
            // Consumer: the primary IS the one projection choice, its
            // tiles inline (the inline-tiles ruling — no popover here).
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
                    SpaceCellRow { cell: section.primary.clone(), side, on_action }
                },
            }
            p { class: HINT_CLASS, "{primary_hint(&section)}" }
            for cell in section.cells.clone() {
                SpaceCellRow {
                    key: "{cell.role:?}",
                    cell: cell.clone(),
                    side,
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

/// One answer cell: its label and the projection choice — inline tiles
/// when there is a real choice (the inline-tiles ruling: no popover, no
/// dropdown; the choices are always visible like the 1D/2D tab pair), a
/// read-only statement when there is not (`UiSpaceCell::is_choosable` —
/// the 2D→1D answer has one declared variant today). The consumer's old
/// inline `force` bit is gone (P4b): an explicit pick IS the override,
/// dispatched as part of the choice.
///
/// The factored form (THE FACTORIZATION): the two modifier toggles
/// ("mirror", "flip") render beneath the tiles when a projection is
/// active; the along-the-wire state gets the wire's [forward|reversed]
/// row instead. Shape first, modifiers under it — never a flattened
/// sixteen-tile grid.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SpaceCellRow(
    cell: UiSpaceCell,
    side: UiSpaceSide,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let modifiers = cell.modifiers.clone();
    let wire_direction = cell.wire_direction.clone();
    let base = active_projection(&cell);
    let choosable = cell.is_choosable() && on_action.is_some();
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-1.5",
            span { class: ROW_LABEL_CLASS, "{cell_label(side, cell.role)}" }
            if choosable {
                ChoiceTiles {
                    choices: cell.choices.clone(),
                    side,
                    role: cell.role,
                    address: cell.address.clone(),
                    strip_order: cell.strip_order.as_ref().and_then(|strip| strip.address.clone()),
                    current_modifiers: cell
                        .modifiers
                        .as_ref()
                        .map(|rows| (rows.mirror.value, rows.flip.value)),
                    on_action,
                }
            } else {
                // The statement still carries its glyph (G1: the
                // text-only 2D→1D cell "feels odd").
                span { class: field_class(&cell.state),
                    span {
                        class: "tw:inline-flex tw:h-4 tw:w-6 tw:flex-none tw:items-center",
                        aria_hidden: "true",
                        ProjectionGlyph { kind: active_glyph(&cell) }
                    }
                    span { class: "tw:min-w-0 tw:truncate",
                        "{active_variant_label(side, &cell)}{modifier_suffix(&cell)}"
                    }
                }
            }
        }
        if let (Some(modifiers), Some(base)) = (modifiers, base) {
            SpaceModifierTiles { modifiers, base, on_action }
        }
        if let Some(wire) = wire_direction {
            WireDirectionTiles { wire, on_action }
        }
    }
}

/// The ACTIVE factored projection a cell currently spells — the base the
/// modifier tiles draw their what-if faces over. `None` when the active
/// choice is not a projection (Auto / along-the-wire / the scanline
/// statement).
fn active_projection(cell: &UiSpaceCell) -> Option<UiCellProjection> {
    match active_glyph(cell) {
        SpaceGlyph::Projection(projection) => Some(projection),
        _ => None,
    }
}

/// The two modifier TILES under the shape tiles ("mirror", "flip") —
/// the modifier-tiles ruling: the checkboxes were "very small and
/// non-visual compared to the projection", so the modifiers wear the
/// same ChoiceTiles clothing, MUTUALLY REFLECTIVE with the shapes:
///
/// - each modifier tile draws the CURRENT shape and other modifier with
///   ITS OWN modifier forced ON — the tile always shows what pressing it
///   produces; the selected treatment (accent border + wash + check),
///   not the drawing, says whether it is active;
/// - the shape tiles (in [`ChoiceTiles`]) draw with the current
///   modifiers applied, closing the loop.
///
/// All faces come from the one chain-derived drawing function, so
/// reflectivity is free. A press is still the ordinary bool `SetValue`.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SpaceModifierTiles(
    modifiers: UiSpaceModifiers,
    /// The active factored projection the what-if faces build on.
    base: UiCellProjection,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    rsx! {
        div { class: TILE_GRID_CLASS,
            ModifierTile {
                label: MODIFIER_MIRROR,
                hint: MIRROR_TITLE,
                projection: modifier_tile_projection(base, Modifier::Mirror),
                row: modifiers.mirror,
                on_action,
            }
            ModifierTile {
                label: MODIFIER_FLIP,
                hint: FLIP_TITLE,
                projection: modifier_tile_projection(base, Modifier::Flip),
                row: modifiers.flip,
                on_action,
            }
        }
    }
}

/// Which modifier a tile toggles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Modifier {
    Mirror,
    Flip,
}

/// The what-if face a modifier tile draws: the current projection with
/// THIS modifier forced on (the reflectivity rule — the tile shows what
/// it produces, always).
fn modifier_tile_projection(base: UiCellProjection, which: Modifier) -> UiCellProjection {
    match which {
        Modifier::Mirror => UiCellProjection {
            mirror: true,
            ..base
        },
        Modifier::Flip => UiCellProjection { flip: true, ..base },
    }
}

/// One modifier tile: the what-if drawing, the word, and the selected
/// treatment when the bool is ON. A press dispatches the ordinary bool
/// `SetValue` (toggling), never a second write path.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ModifierTile(
    label: &'static str,
    hint: &'static str,
    projection: UiCellProjection,
    row: UiSpaceBoolRow,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let value = row.value;
    let wiring = field_wiring(&row.state, &row.address, on_action);
    rsx! {
        button {
            class: tile_class(value),
            r#type: "button",
            title: hint,
            aria_pressed: "{value}",
            onclick: move |event: MouseEvent| {
                event.stop_propagation();
                if let Some((address, handler)) = wiring.clone() {
                    handler.call(slot_set_value_action(address, LpValue::Bool(!value)));
                }
            },
            TileFace {
                kind: SpaceGlyph::Projection(projection),
                selected: value,
                label: label.to_string(),
                hint: hint.to_string(),
            }
        }
    }
}

/// The along-the-wire direction as two TILES — adopted into the tile
/// form with the modifiers (the modifier-tiles ruling's consistency
/// note): "forward" draws the serpentine as wired, "reversed" draws it
/// read back to front; the selected treatment marks the current
/// direction. A press is the ordinary bool `SetValue` over
/// `wire_reversed`.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn WireDirectionTiles(
    wire: UiWireDirectionRow,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let reversed = wire.reversed;
    let wiring = field_wiring(&wire.state, &wire.address, on_action);
    rsx! {
        div { class: TILE_GRID_CLASS,
            for (candidate , label , hint) in [
                (false, WIRE_FORWARD, HINT_WIRE_FORWARD),
                (true, WIRE_REVERSED, HINT_WIRE_REVERSED),
            ] {
                button {
                    key: "{candidate}",
                    class: tile_class(candidate == reversed),
                    r#type: "button",
                    title: hint,
                    onclick: {
                        let wiring = wiring.clone();
                        move |event: MouseEvent| {
                            event.stop_propagation();
                            if candidate == reversed {
                                return;
                            }
                            if let Some((address, handler)) = wiring.clone() {
                                handler.call(slot_set_value_action(address, LpValue::Bool(candidate)));
                            }
                        }
                    },
                    TileFace {
                        kind: SpaceGlyph::AlongWire(candidate),
                        selected: candidate == reversed,
                        label: label.to_string(),
                        hint: hint.to_string(),
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
/// - a shape is `SetValue strip_order = false` → ensure-`Policy` →
///   ensure-`from_1d.Project.shape.<Shape>` (the factored cell's shape
///   row) → set-`force = true`.
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
            .and_then(|field| field.child_field("Project"))
            .and_then(|project| project.child_field("shape"))
            .and_then(|shape| shape.child_field(variant))
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

/// The shared choice-tiles control (the inline-tiles ruling): one
/// glyph+label tile per declared variant, rendered DIRECTLY in the
/// section body on both cards — always visible, like the 1D/2D tab pair;
/// no popover, no dropdown, no nested expansion. The selected tile is
/// unmistakable: accent border, accent wash fill, and a check glyph
/// (G1b follow-up: the old selected treatment was hard to read).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ChoiceTiles(
    choices: Vec<UiSpaceChoice>,
    side: UiSpaceSide,
    role: UiSpaceCellRole,
    #[props(default = None)] address: Option<ProjectSlotAddress>,
    /// The consumer cell's `strip_order_meaningful` row address — every
    /// consumer pick includes its `SetValue` (see [`choice_actions`]).
    #[props(default = None)]
    strip_order: Option<ProjectSlotAddress>,
    /// The cell's CURRENT (mirror, flip) — the reflectivity rule (the
    /// modifier-tiles ruling): shape tiles draw with the live modifiers
    /// applied, so every tile face is a true what-if of pressing it.
    #[props(default = None)]
    current_modifiers: Option<(bool, bool)>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    rsx! {
        div { class: TILE_GRID_CLASS,
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
                        }
                    },
                    TileFace {
                        kind: tile_glyph(role, &choice.variant, current_modifiers),
                        selected: choice.selected,
                        label: variant_label(side, role, &choice),
                        hint: choice_hint(role, &choice.variant).to_string(),
                    }
                }
            }
        }
    }
}

/// One tile's inner face — the drawing, the check badge, the word, the
/// hint — shared by the shape tiles, the modifier tiles, and the wire
/// tiles so the three sets cannot drift apart visually.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn TileFace(kind: SpaceGlyph, selected: bool, label: String, hint: String) -> Element {
    rsx! {
        span { class: "tw:relative tw:block tw:h-10 tw:w-full tw:overflow-hidden tw:rounded-xs tw:bg-page",
            ProjectionGlyph { kind }
            // The unmistakable half of the selected state: a check badge
            // over the drawing's corner, paired with the accent
            // border+wash on the tile.
            if selected {
                span {
                    class: TILE_CHECK_CLASS,
                    aria_hidden: "true",
                    StudioIcon { name: StudioIconName::StepComplete, size: 10 }
                }
            }
        }
        span { class: "tw:min-w-0 tw:truncate tw:text-[11px] tw:font-bold", "{label}" }
        span { class: "tw:min-w-0 tw:truncate tw:text-[10px] tw:leading-tight tw:text-dim-foreground",
            "{hint}"
        }
    }
}

/// A shape tile's face, reflective (the modifier-tiles ruling): a
/// projection tile draws its shape WITH the cell's current modifiers
/// applied; the deferring choices keep their own schematics.
fn tile_glyph(
    role: UiSpaceCellRole,
    variant: &str,
    current_modifiers: Option<(bool, bool)>,
) -> SpaceGlyph {
    match (glyph_for(role, variant), current_modifiers) {
        (SpaceGlyph::Projection(projection), Some((mirror, flip))) => {
            SpaceGlyph::Projection(UiCellProjection {
                mirror,
                flip,
                ..projection
            })
        }
        (glyph, _) => glyph,
    }
}

/// What a tile or field glyph draws. Web-side vocabulary, wider than
/// [`UiCellProjection`]: the projection drawings run the SAME transform
/// chain the engine runs (one drawing function — the factorization
/// ruling), and the deferring choices keep their own schematics.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SpaceGlyph {
    /// A factored projection cell: the drawing IS the chain, evaluated
    /// over a grid (see [`chain_cells`]).
    Projection(UiCellProjection),
    /// The 2D→1D answer: the texture's centre row, read as a strip.
    CentreScanline,
    /// The consumer dropdown's `Auto`: the answer lives on the source.
    FollowSource,
    /// The consumer dropdown's along-the-wire choice: the strip runs in
    /// wire order and the map does not apply — drawn as a serpentine wire
    /// carrying the ramp. `true` = reversed.
    AlongWire(bool),
}

/// The drawing for one choice, by role and RAW variant name — the shape
/// tiles wear the PLAIN shape (the modifier toggles refine the drawing
/// on the active cell via [`active_glyph`]).
fn glyph_for(role: UiSpaceCellRole, variant: &str) -> SpaceGlyph {
    match variant {
        "ExtrudeX" => SpaceGlyph::Projection(UiCellProjection::plain(UiProjectionShape::ExtrudeX)),
        "ExtrudeY" => SpaceGlyph::Projection(UiCellProjection::plain(UiProjectionShape::ExtrudeY)),
        "Radial" => SpaceGlyph::Projection(UiCellProjection::plain(UiProjectionShape::Radial)),
        "Angular" => SpaceGlyph::Projection(UiCellProjection::plain(UiProjectionShape::Angular)),
        "Auto" => SpaceGlyph::FollowSource,
        ALONG_WIRE_VARIANT => SpaceGlyph::AlongWire(false),
        // A 2D shader's 1D answer IS the centre scanline.
        "Default" if role == UiSpaceCellRole::ProducerIn1d => SpaceGlyph::CentreScanline,
        _ => SpaceGlyph::FollowSource,
    }
}

/// The FIELD's active glyph: the active shape composed with the LIVE
/// modifier toggles (one chain, one drawing), or the wire drawing
/// oriented by the wire row.
fn active_glyph(cell: &UiSpaceCell) -> SpaceGlyph {
    if cell.active == ALONG_WIRE_VARIANT {
        return SpaceGlyph::AlongWire(
            cell.wire_direction
                .as_ref()
                .is_some_and(|wire| wire.reversed),
        );
    }
    match glyph_for(cell.role, &cell.active) {
        SpaceGlyph::Projection(mut projection) => {
            if let Some(modifiers) = &cell.modifiers {
                projection.mirror = modifiers.mirror.value;
                projection.flip = modifiers.flip.value;
            }
            SpaceGlyph::Projection(projection)
        }
        other => other,
    }
}

/// A schematic drawing of what one choice does to a 1D source.
///
/// **The projection drawings are the chain itself** (the factorization
/// ruling: one drawing function, not N): [`chain_cells`] evaluates the
/// same shape → mirror → flip composition the engine runs over a coarse
/// grid, and the ramp opacity IS the strip coordinate. Every reachable
/// cell — including angular + mirror, which no hand drawing existed for
/// — renders itself.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProjectionGlyph(kind: SpaceGlyph) -> Element {
    rsx! {
        svg {
            class: "tw:block tw:h-full tw:w-full tw:text-soft-foreground",
            view_box: "0 0 64 40",
            preserve_aspect_ratio: "none",
            role: "img",
            match kind {
                SpaceGlyph::Projection(projection) => rsx! {
                    for (index , (x , y , width , height , opacity)) in chain_cells(projection)
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

/// The strip's ramp across eight bands (the centre-scanline drawing).
const RAMP: [f32; 8] = [0.14, 0.24, 0.36, 0.48, 0.6, 0.72, 0.84, 0.96];

/// THE drawing function (the factorization ruling): the projection
/// chain — shape coordinate, then mirror's fold, then flip's reversal,
/// exactly `lpc-engine`'s `project_2d_to_1d` — evaluated at each cell of
/// a 16×10 grid over the glyph's 64×40 box, with the strip coordinate
/// rendered as the ramp opacity. One function draws every reachable
/// projection, composites included.
fn chain_cells(projection: UiCellProjection) -> Vec<(f32, f32, f32, f32, f32)> {
    const COLS: usize = 16;
    const ROWS: usize = 10;
    const CELL_W: f32 = 64.0 / COLS as f32;
    const CELL_H: f32 = 40.0 / ROWS as f32;
    let mut cells = Vec::with_capacity(COLS * ROWS);
    for row in 0..ROWS {
        for col in 0..COLS {
            let u = (col as f32 + 0.5) / COLS as f32;
            let v = (row as f32 + 0.5) / ROWS as f32;
            let mut t = match projection.shape {
                UiProjectionShape::ExtrudeX => u,
                UiProjectionShape::ExtrudeY => v,
                UiProjectionShape::Radial => {
                    let dx = u - 0.5;
                    let dy = v - 0.5;
                    ((dx * dx + dy * dy).sqrt() / core::f32::consts::FRAC_1_SQRT_2).min(1.0)
                }
                UiProjectionShape::Angular => {
                    let turns = (v - 0.5).atan2(u - 0.5) / core::f32::consts::TAU;
                    if turns < 0.0 { turns + 1.0 } else { turns }
                }
            };
            if projection.mirror {
                t = 1.0 - (2.0 * t - 1.0).abs();
            }
            if projection.flip {
                t = 1.0 - t;
            }
            cells.push((
                col as f32 * CELL_W,
                row as f32 * CELL_H,
                CELL_W,
                CELL_H,
                0.08 + 0.88 * t.clamp(0.0, 1.0),
            ));
        }
    }
    cells
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

/// A projection's caption (THE FACTORIZATION): the shape name plus its
/// modifier words — `extrude-x · mirrored · flipped`. A plain cell stays
/// bare.
fn projection_label(projection: UiCellProjection) -> String {
    let mut label = shape_label(projection.shape).to_string();
    if projection.mirror {
        label.push_str(" · ");
        label.push_str(MODIFIER_MIRRORED);
    }
    if projection.flip {
        label.push_str(" · ");
        label.push_str(MODIFIER_FLIPPED);
    }
    label
}

/// A shape's caption/tile word.
const fn shape_label(shape: UiProjectionShape) -> &'static str {
    match shape {
        UiProjectionShape::ExtrudeX => SHAPE_EXTRUDE_X,
        UiProjectionShape::ExtrudeY => SHAPE_EXTRUDE_Y,
        UiProjectionShape::Radial => SHAPE_RADIAL,
        UiProjectionShape::Angular => SHAPE_ANGULAR,
    }
}

/// The ` · mirrored` / ` · flipped` a cell's statement face and drawer
/// summary append from the LIVE modifier toggles (or ` ←` from the wire
/// row) — empty when everything sits at its default.
fn modifier_suffix(cell: &UiSpaceCell) -> String {
    if cell.active == ALONG_WIRE_VARIANT {
        return if cell
            .wire_direction
            .as_ref()
            .is_some_and(|wire| wire.reversed)
        {
            " ←".to_string()
        } else {
            String::new()
        };
    }
    let Some(modifiers) = cell.modifiers.as_ref() else {
        return String::new();
    };
    let mut suffix = String::new();
    if modifiers.mirror.value {
        suffix.push_str(" · ");
        suffix.push_str(MODIFIER_MIRRORED);
    }
    if modifiers.flip.value {
        suffix.push_str(" · ");
        suffix.push_str(MODIFIER_FLIPPED);
    }
    suffix
}

fn origin_label(origin: UiProjectionOrigin) -> &'static str {
    match origin {
        UiProjectionOrigin::Declared => ORIGIN_DECLARED,
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
        (_, _, "ExtrudeX") => Some(SHAPE_EXTRUDE_X),
        (_, _, "ExtrudeY") => Some(SHAPE_EXTRUDE_Y),
        (_, _, "Radial") => Some(SHAPE_RADIAL),
        (_, _, "Angular") => Some(SHAPE_ANGULAR),
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
        .or_else(|| known_variant_label(side, cell.role, &cell.active).map(str::to_string))
        .unwrap_or_else(|| cell.active_label.clone())
}

/// One line per choice, by role and RAW variant name.
fn choice_hint(role: UiSpaceCellRole, variant: &str) -> &'static str {
    match variant {
        "Auto" => HINT_FOLLOW,
        ALONG_WIRE_VARIANT => HINT_ALONG_WIRE,
        "Default" if role == UiSpaceCellRole::ProducerIn1d => HINT_CENTRE_SCANLINE,
        "ExtrudeX" => HINT_EXTRUDE_X,
        "ExtrudeY" => HINT_EXTRUDE_Y,
        "Radial" => HINT_RADIAL,
        "Angular" => HINT_ANGULAR,
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
                    modifier_suffix(cell)
                )
            } else if section.cell(UiSpaceCellRole::ProducerIn1d).is_some() {
                format!("{space} · in 1D: {PROJECTION_CENTRE_SCANLINE}")
            } else {
                space.to_string()
            }
        }
        UiSpaceSide::Consumer => {
            if section.primary.active == ALONG_WIRE_VARIANT {
                format!("{CONSUMER_ALONG_WIRE}{}", modifier_suffix(&section.primary))
            } else if section.primary.active == "Auto" {
                CONSUMER_FOLLOW.to_string()
            } else {
                format!(
                    "1D sources: {}{} (override)",
                    active_variant_label(section.side, &section.primary),
                    modifier_suffix(&section.primary)
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

/// The tile grid every tile set lays out in (shapes, modifiers, wire
/// direction) — one template, so the sets align into one field.
const TILE_GRID_CLASS: &str =
    "tw:grid tw:min-w-0 tw:grid-cols-[repeat(auto-fill,minmax(7.5rem,1fr))] tw:gap-1.5";

/// One tile of the inline choice grid. Selected = accent border + accent
/// wash + the check badge ([`TILE_CHECK_CLASS`]) — three signals, because
/// the old filled-grey treatment was ruled hard to read.
fn tile_class(selected: bool) -> &'static str {
    if selected {
        "tw:grid tw:min-w-0 tw:cursor-pointer tw:appearance-none tw:gap-0.5 tw:rounded-xs tw:border tw:border-accent tw:bg-accent-wash tw:p-1.5 tw:text-left tw:text-strong-foreground"
    } else {
        "tw:grid tw:min-w-0 tw:cursor-pointer tw:appearance-none tw:gap-0.5 tw:rounded-xs tw:border tw:border-border-subtle tw:bg-transparent tw:p-1.5 tw:text-left tw:text-muted-foreground tw:hover:border-border-strong tw:hover:text-strong-foreground"
    }
}

/// The selected tile's check badge, over the drawing's top-right corner.
const TILE_CHECK_CLASS: &str = "tw:absolute tw:right-1 tw:top-1 tw:inline-flex tw:h-4 tw:w-4 tw:items-center tw:justify-center tw:rounded-pill tw:bg-accent tw:text-accent-foreground";

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
            modifiers: None,
            wire_direction: None,
            strip_order: None,
        }
    }

    fn bool_row(value: bool) -> lpa_studio_core::UiSpaceBoolRow {
        lpa_studio_core::UiSpaceBoolRow {
            value,
            address: None,
            state: UiSlotFieldState::editable(),
        }
    }

    fn modifiers(mirror: bool, flip: bool) -> UiSpaceModifiers {
        UiSpaceModifiers {
            mirror: bool_row(mirror),
            flip: bool_row(flip),
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

    /// The factored consumer section: ONE choice list whose first entry
    /// is `along the wire`, then follow/shapes.
    fn consumer(active: &str) -> UiSpaceSection {
        UiSpaceSection {
            side: UiSpaceSide::Consumer,
            primary: cell(
                UiSpaceCellRole::Primary,
                active,
                &[
                    ALONG_WIRE_VARIANT,
                    "Auto",
                    "ExtrudeX",
                    "ExtrudeY",
                    "Radial",
                    "Angular",
                ],
            ),
            declared_space: None,
            cells: Vec::new(),
            mismatch: None,
        }
    }

    /// The shape cell of a 1D producer, in the factored form.
    fn shape_cell(active: &str) -> UiSpaceCell {
        cell(
            UiSpaceCellRole::ProducerIn2d,
            active,
            &["ExtrudeX", "ExtrudeY", "Radial", "Angular"],
        )
    }

    /// The caption rule (THE FACTORIZATION): shape word plus modifier
    /// words, defaults bare — `extrude-x · mirrored · flipped`.
    #[test]
    fn captions_spell_the_shape_and_its_modifiers() {
        assert_eq!(
            projection_label(UiCellProjection::plain(UiProjectionShape::ExtrudeX)),
            "extrude-x"
        );
        assert_eq!(
            projection_label(UiCellProjection {
                shape: UiProjectionShape::ExtrudeX,
                mirror: true,
                flip: true,
            }),
            "extrude-x · mirrored · flipped"
        );
        assert_eq!(
            projection_label(UiCellProjection {
                shape: UiProjectionShape::Angular,
                mirror: true,
                flip: false,
            }),
            "angular · mirrored",
            "the up-and-back sweep — a state the old vocabulary could not spell"
        );
        assert_eq!(
            projection_label(UiCellProjection {
                shape: UiProjectionShape::Radial,
                mirror: false,
                flip: true,
            }),
            "radial · flipped"
        );
    }

    /// The statement face and drawer summaries wear the LIVE modifier
    /// toggles; the along-the-wire state wears the wire arrow instead.
    #[test]
    fn summaries_wear_the_live_modifiers() {
        let mut answer = shape_cell("Radial");
        answer.modifiers = Some(modifiers(false, true));
        let shader = producer("OneD", vec![answer]);
        assert_eq!(
            space_section_summary(&shader),
            "1D · in 2D: radial · flipped"
        );

        let mut fixture = consumer("Angular");
        fixture.primary.modifiers = Some(modifiers(true, false));
        assert_eq!(
            space_section_summary(&fixture),
            "1D sources: angular · mirrored (override)"
        );

        let mut wire = consumer(ALONG_WIRE_VARIANT);
        wire.primary.wire_direction = Some(UiWireDirectionRow {
            reversed: true,
            address: None,
            state: UiSlotFieldState::editable(),
        });
        assert_eq!(space_section_summary(&wire), "along the wire ←");
    }

    /// The reflectivity rules (the modifier-tiles ruling): shape tiles
    /// draw with the CURRENT modifiers applied; each modifier tile draws
    /// the current projection with ITS modifier forced on — every face a
    /// true what-if of pressing it.
    #[test]
    fn tiles_reflect_each_other() {
        // Shape tiles wear the live modifiers…
        assert_eq!(
            tile_glyph(UiSpaceCellRole::ProducerIn2d, "Radial", Some((true, false))),
            SpaceGlyph::Projection(UiCellProjection {
                shape: UiProjectionShape::Radial,
                mirror: true,
                flip: false,
            })
        );
        // …the deferring choices keep their schematics…
        assert_eq!(
            tile_glyph(UiSpaceCellRole::Primary, "Auto", Some((true, true))),
            SpaceGlyph::FollowSource
        );
        // …and a modifier tile forces its own bit over the current cell
        // (the drawing shows what pressing produces — selected state
        // alone says whether it is already active).
        let base = UiCellProjection {
            shape: UiProjectionShape::Angular,
            mirror: false,
            flip: true,
        };
        assert_eq!(
            modifier_tile_projection(base, Modifier::Mirror),
            UiCellProjection {
                shape: UiProjectionShape::Angular,
                mirror: true,
                flip: true,
            }
        );
        assert_eq!(
            modifier_tile_projection(base, Modifier::Flip),
            base,
            "flip already on: the tile shows the current state itself"
        );
    }

    /// The active glyph composes the shape with the LIVE toggles — one
    /// chain, one drawing (the factorization ruling).
    #[test]
    fn the_active_glyph_composes_shape_and_modifiers() {
        let mut answer = shape_cell("Angular");
        answer.modifiers = Some(modifiers(true, false));
        assert_eq!(
            active_glyph(&answer),
            SpaceGlyph::Projection(UiCellProjection {
                shape: UiProjectionShape::Angular,
                mirror: true,
                flip: false,
            })
        );
        // The TILES stay plain — the toggles refine the active cell only.
        assert_eq!(
            glyph_for(UiSpaceCellRole::ProducerIn2d, "ExtrudeY"),
            SpaceGlyph::Projection(UiCellProjection::plain(UiProjectionShape::ExtrudeY))
        );
        assert_eq!(
            glyph_for(UiSpaceCellRole::Primary, "Auto"),
            SpaceGlyph::FollowSource
        );
        assert_eq!(
            glyph_for(UiSpaceCellRole::ProducerIn1d, "Default"),
            SpaceGlyph::CentreScanline
        );
    }

    /// The chain drawing IS the engine chain: extrude-x ramps along the
    /// columns; the mirror modifier folds it; the flip reverses it. Spot
    /// checks over the grid cells the glyph rasterizes.
    #[test]
    fn the_glyph_chain_matches_the_engine_chain() {
        let opacity_at =
            |projection: UiCellProjection, index: usize| chain_cells(projection)[index].4;
        // First cell of the top row (u ≈ 0.03): plain extrude-x is dark,
        // flipped is bright.
        let plain = UiCellProjection::plain(UiProjectionShape::ExtrudeX);
        let flipped = UiCellProjection {
            flip: true,
            ..plain
        };
        assert!(opacity_at(plain, 0) < 0.15);
        assert!(opacity_at(flipped, 0) > 0.85);
        // Mirrored extrude-x: both ends dark, centre bright.
        let mirrored = UiCellProjection {
            mirror: true,
            ..plain
        };
        assert!(opacity_at(mirrored, 0) < 0.2);
        assert!(opacity_at(mirrored, 15) < 0.2);
        assert!(opacity_at(mirrored, 7) > 0.8);
    }

    /// The consumer choice list's dispatch (strip-order unification +
    /// the factorization): `along the wire` is the bool SetValue alone;
    /// `Auto` clears the bit and ensures `consume.Auto`; a shape clears
    /// the bit and runs ensure-Policy →
    /// ensure-`from_1d.Project.shape.<Shape>` → force=true.
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

        let shape = choice_actions(
            UiSpaceSide::Consumer,
            UiSpaceCellRole::Primary,
            &address,
            Some(&strip),
            "ExtrudeY",
        );
        assert_eq!(
            shape.len(),
            4,
            "clear the bit, ensure Policy, ensure from_1d.Project.shape.ExtrudeY, set force"
        );

        // Producer cells keep the single generic gesture at the SHAPE row.
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
        let shader = producer("OneD", vec![shape_cell("Radial")]);
        assert_eq!(space_section_summary(&shader), "1D · in 2D: radial");
        assert_eq!(space_section_summary(&producer("TwoD", Vec::new())), "2D");
        assert_eq!(space_section_summary(&consumer("Auto")), CONSUMER_FOLLOW);
        assert_eq!(
            space_section_summary(&consumer("ExtrudeY")),
            "1D sources: extrude-y (override)"
        );
        assert_eq!(
            space_section_summary(&consumer(ALONG_WIRE_VARIANT)),
            CONSUMER_ALONG_WIRE
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
    /// choice list speaks wire/follow/override.
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
        assert_eq!(
            primary_hint(&consumer(ALONG_WIRE_VARIANT)),
            CONSUMER_HINT_ALONG_WIRE
        );
    }

    /// The ladder states the one rung that can still surprise, and only
    /// where something contends: the consumer's hint line covers its side.
    #[test]
    fn the_ladder_names_the_rung_that_can_surprise() {
        assert_eq!(ladder_line(&producer("TwoD", Vec::new())), None);
        assert_eq!(
            ladder_line(&producer("OneD", vec![shape_cell("Radial")])),
            Some(LADDER_PRODUCER)
        );
        assert_eq!(ladder_line(&consumer("Auto")), None);
        assert_eq!(ladder_line(&consumer("Radial")), None);
    }

    /// D15's captions, including D11's honesty rule — post-v9 there are
    /// two origins (the producer always declares).
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
            projection: Some(UiCellProjection::plain(UiProjectionShape::Radial)),
            origin: Some(UiProjectionOrigin::Declared),
            primary: UiVisualSpace::OneD,
        };
        assert_eq!(
            preview_space_caption(UiVisualSpace::TwoD, Some(declared)),
            "in 2D · radial (declared)"
        );

        let forced = UiVisualProductSpace {
            origin: Some(UiProjectionOrigin::Forced),
            projection: Some(UiCellProjection {
                shape: UiProjectionShape::ExtrudeX,
                mirror: true,
                flip: false,
            }),
            ..declared
        };
        assert_eq!(
            preview_space_caption(UiVisualSpace::TwoD, Some(forced)),
            "in 2D · extrude-x · mirrored (forced)"
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
        assert_eq!(space_badge(&consumer("Auto")), None);
    }
}
