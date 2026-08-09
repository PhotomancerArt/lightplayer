//! Derivation of the `dimensionality` section both visual-side faces carry
//! ([`UiSpaceSection`]) — the two-sided space model lifted out of the
//! advanced drawer and onto the card (ADR
//! `2026-08-09-dimensionality-authoring-surface.md`).
//!
//! One derivation, two sides. The shader's `space` enum
//! (`TwoD { in_1d } | OneD { in_2d }`) and the fixture's `consume` enum
//! (`Auto | Policy { from_1d, force }`) plus `strip_order_meaningful` and
//! `wire_reversed` land in the SAME DTO, so D13's "the two sections are
//! visual mirrors" is a data fact the web cannot accidentally break.
//!
//! Everything is read off the already-projected config rows — the same
//! rows the advanced drawer would render, which is exactly why the face
//! then CLAIMS them ([`claimed_config_rows`]): the section IS their
//! surface now, and two controls writing one slot is the defect this
//! avoids. No gesture is invented here either: a cell carries its enum
//! row's address so a choice is the `EnsurePresent <enum>.<Variant>` the
//! generic variant field already dispatches, and a bool row carries its
//! own address for the ordinary `SetValue`.
//!
//! The consumer's single control is a PRESENTATION over two slots: its
//! first choice, "along the wire", is `strip_order_meaningful` — which
//! the engine reads as a gate on the projection, so a pick anywhere in
//! this control writes that bit as part of the same batch. Nothing else
//! would be honest about which control is live.
//!
//! Enum payload rows arrive FLATTENED (`SlotController` hoists a variant's
//! record fields to the enum row's own record body), so the shader's
//! answer cell is a field of the `space` row keyed `space.OneD.in_2d` —
//! there is no intermediate variant row to descend through.

use crate::{
    UiCellProjection, UiConfigSlot, UiConfigSlotBody, UiProjectionShape, UiSlotComposite,
    UiSlotValueKind, UiSpaceBoolRow, UiSpaceCell, UiSpaceCellRole, UiSpaceChoice, UiSpaceMismatch,
    UiSpaceModifiers, UiSpaceSection, UiSpaceSide, UiVisualSpace, UiWireDirectionRow,
};

/// The shader def's producer-side declaration row.
pub(in crate::app::project) const SHADER_SPACE_ROW: &str = "space";
/// The fixture def's consumer-side policy row.
pub(in crate::app::project) const FIXTURE_CONSUME_ROW: &str = "consume";
/// The fixture def's "does strip order mean something?" row (vision D3).
pub(in crate::app::project) const FIXTURE_STRIP_ORDER_ROW: &str = "strip_order_meaningful";
/// The fixture def's along-the-wire direction row (the wire-reversed
/// addendum).
pub(in crate::app::project) const FIXTURE_WIRE_REVERSED_ROW: &str = "wire_reversed";

/// The fixture def's mapping row (a Shape-preset target; stays in the
/// advanced drawer, never claimed).
const FIXTURE_MAPPING_ROW: &str = "mapping";
/// The fixture def's render-size row (a Shape-preset target).
const FIXTURE_RENDER_SIZE_ROW: &str = "render_size";

/// The Shape declaration moment's slot targets (D13, plan-B P5) — see
/// [`crate::UiShapePresets`] for the seam contract. Every address is the
/// SAME row the advanced drawer (or the dimensionality section) would
/// dispatch at; this function only gathers them.
pub(in crate::app::project) fn fixture_shape_presets(
    rows: &[&UiConfigSlot],
) -> Option<crate::UiShapePresets> {
    let row = |key: &str| rows.iter().copied().find(|row| row.key == key);
    let mapping = row(FIXTURE_MAPPING_ROW);
    let render_size = row(FIXTURE_RENDER_SIZE_ROW);
    let strip_order = row(FIXTURE_STRIP_ORDER_ROW);
    // No backing rows at all — a hand-built face or a pre-slot def; the
    // guided state renders nothing rather than inventing targets.
    if mapping.is_none() && render_size.is_none() && strip_order.is_none() {
        return None;
    }
    let has_map2d = mapping.is_some_and(|row| match &row.composite {
        Some(UiSlotComposite::Enum(composite)) => composite.active == "Map2d",
        _ => false,
    });
    Some(crate::UiShapePresets {
        mapping: mapping.and_then(|row| row.address.clone()),
        render_size: render_size.and_then(|row| row.address.clone()),
        strip_order: strip_order.and_then(|row| row.address.clone()),
        has_map2d,
    })
}

/// The producer side's section: the shader's `space` declaration, its
/// answer cell for the opposite dimension, and the D1 mismatch state.
///
/// `status_detail` is the node's status text when the node is in error —
/// the only place the declared-vs-entry mismatch surfaces (see
/// [`space_mismatch`]).
pub(in crate::app::project) fn shader_space_section(
    rows: &[&UiConfigSlot],
    status_detail: Option<&str>,
) -> Option<UiSpaceSection> {
    let row = rows
        .iter()
        .copied()
        .find(|row| row.key == SHADER_SPACE_ROW)?;
    let mut primary = enum_cell(row, UiSpaceCellRole::Primary, "Space", shader_space_label)?;
    // Presentation order: the tab pair reads `1D | 2D` (Yona's ruling) —
    // the MODEL declares TwoD first (it is the default and the
    // compat-anchor; never reorder it), so the derivation sorts the
    // choices for display.
    primary
        .choices
        .sort_by_key(|choice| choice.variant != "OneD");
    let declared_space = shader_declared_space(&primary.active);
    // The answer cell is whichever the ACTIVE variant declares: a 1D
    // shader answers 2D consumers, a 2D shader answers 1D ones. Only one
    // exists at a time — the other variant's payload is not in the tree.
    //
    // THE FACTORIZATION (v9): the 2D answer is the `Project` record's
    // flattened payload — the SHAPE enum row carries the four tiles, and
    // the `mirror`/`flip` bool rows become the cell's modifier toggles.
    // There is no `Default` variant anymore: the producer always
    // declares, and a fresh record IS plain extrude-x.
    let cells = [
        (UiSpaceCellRole::ProducerIn2d, "in_2d", "Projection"),
        (UiSpaceCellRole::ProducerIn1d, "in_1d", "To 1D consumers"),
    ]
    .into_iter()
    .filter_map(|(role, field, label)| {
        let answer_row = payload_field(row, field)?;
        if role == UiSpaceCellRole::ProducerIn2d {
            let shape_row = payload_field(answer_row, "shape")?;
            let mut cell = enum_cell(shape_row, role, label, projection_label)?;
            cell.modifiers = modifier_rows(answer_row);
            Some(cell)
        } else {
            // The 1D answer (`SpaceAnswer1`) is still the single
            // centre-scanline statement — untouched by the factorization.
            enum_cell(answer_row, role, label, projection_label)
        }
    })
    .collect();
    Some(UiSpaceSection {
        side: UiSpaceSide::Producer,
        primary,
        declared_space,
        cells,
        mismatch: declared_space.and_then(|declared| space_mismatch(declared, status_detail)),
    })
}

/// The consumer side's section: ONE dropdown cell over the fixture's
/// `consume` policy AND its `strip_order_meaningful` bit.
///
/// **G1 rework (plan-B P4b).** G1 ruled the `Auto`/`Policy` split
/// backwards as a surface: "use the dropdown with the 'default' option…
/// then we just have one control." So the primary cell IS the projection
/// choice — a synthetic `Auto` entry ("follow the source") plus the four
/// [`ConsumerCell2`] projections. The projection variants are the model's
/// static vocabulary rather than tree rows because under `Auto` the
/// `from_1d` payload is not in the tree at all, and the dropdown must
/// still offer them. `force` is absorbed by the pick gesture (an explicit
/// pick IS the override).
///
/// **Strip-order unification (post-G1b ruling).** The
/// `strip_order_meaningful` checkbox GATED the dropdown (a true bit means
/// `select_request_space` issues a wire-order 1D request and the
/// projection never fires) — two sibling controls, one silently disabling
/// the other. So the checkbox died and its semantics became the
/// dropdown's FIRST choice, the synthetic `AlongWire` ("along the wire"):
/// selected whenever the bit is true regardless of `consume`, and every
/// consumer pick now includes the bit's `SetValue` (true for
/// along-the-wire, false otherwise) alongside the existing consume ops.
/// Same two slots, same write path — a pure re-presentation, compatible
/// with D15 absorbing the bool later.
pub(in crate::app::project) fn fixture_space_section(
    rows: &[&UiConfigSlot],
) -> Option<UiSpaceSection> {
    let row = rows
        .iter()
        .copied()
        .find(|row| row.key == FIXTURE_CONSUME_ROW)?;
    let strip = rows
        .iter()
        .copied()
        .find(|row| row.key == FIXTURE_STRIP_ORDER_ROW)
        .and_then(strip_order_row);
    let wire_direction = rows
        .iter()
        .copied()
        .find(|row| row.key == FIXTURE_WIRE_REVERSED_ROW)
        .and_then(wire_direction_row);
    let primary = consumer_projection_cell(row, strip, wire_direction)?;
    Some(UiSpaceSection {
        side: UiSpaceSide::Consumer,
        primary,
        // A fixture states a policy, never a space: its own dimensionality
        // comes from its mapping, not from this section.
        declared_space: None,
        cells: Vec::new(),
        mismatch: None,
    })
}

/// The consumer dropdown's variants when the fixture has authored a
/// policy: the model's static [`ConsumerCell2`] vocabulary. Static rather
/// than read from the tree because the `Auto` state carries no payload
/// rows, and the dropdown offers the projections from either state.
const CONSUMER_PROJECTION_VARIANTS: [&str; 4] = ["ExtrudeX", "ExtrudeY", "Radial", "Angular"];

/// The synthetic ident of the consumer dropdown's along-the-wire choice.
/// Not a model variant: it stands for `strip_order_meaningful = true`, so
/// the web dispatches the bool `SetValue` for it rather than an enum
/// ensure.
pub(crate) const CONSUMER_ALONG_WIRE_VARIANT: &str = "AlongWire";

/// The one consumer cell: `along the wire` (the strip-order bit) + `Auto`
/// ("follow the source") + the projections, selected from the strip-order
/// bit first — a true bit gates everything else — then the active
/// `from_1d` when a policy is authored.
fn consumer_projection_cell(
    row: &UiConfigSlot,
    strip: Option<UiSpaceBoolRow>,
    wire_direction: Option<UiWireDirectionRow>,
) -> Option<UiSpaceCell> {
    let Some(UiSlotComposite::Enum(composite)) = &row.composite else {
        return None;
    };
    let from_1d = payload_field(row, "from_1d");
    let along_wire = strip.as_ref().is_some_and(|strip| strip.value);
    // The factored cell (v9): the active SHAPE lives on the flattened
    // `from_1d.Project.shape` payload row.
    let policy_shape = from_1d
        .and_then(|field| payload_field(field, "shape"))
        .and_then(|shape| match &shape.composite {
            Some(UiSlotComposite::Enum(from)) => Some(from.active.clone()),
            _ => None,
        });
    let active = if along_wire {
        CONSUMER_ALONG_WIRE_VARIANT.to_string()
    } else if composite.active == "Auto" {
        "Auto".to_string()
    } else {
        policy_shape.unwrap_or_else(|| "Auto".to_string())
    };
    // The along-the-wire choice is only offered when the bit's row exists
    // to write; a section without it keeps the follow/shape choices only.
    let choices = strip
        .as_ref()
        .map(|_| CONSUMER_ALONG_WIRE_VARIANT)
        .into_iter()
        .chain(std::iter::once("Auto"))
        .chain(CONSUMER_PROJECTION_VARIANTS)
        .map(|variant| UiSpaceChoice {
            variant: variant.to_string(),
            label: consumer_choice_label(variant),
            projection: variant_projection(variant),
            selected: variant == active,
        })
        .collect();
    Some(UiSpaceCell {
        role: UiSpaceCellRole::Primary,
        label: "Show 1D sources by".to_string(),
        active: active.clone(),
        active_label: consumer_choice_label(&active),
        choices,
        address: row.address.clone(),
        state: row.state.clone(),
        // The modifier toggles live on the flattened
        // `consume.Policy.from_1d.Project` payload rows; while the
        // fixture is in `Auto` (or along-the-wire, where the projection
        // is gated off) they are absent.
        modifiers: if along_wire {
            None
        } else {
            from_1d.and_then(modifier_rows)
        },
        // The along-the-wire state gets the [forward|reversed] row over
        // `wire_reversed` instead (the wire-reversed addendum).
        wire_direction: if along_wire { wire_direction } else { None },
        strip_order: strip,
    })
}

/// The factored `Project` payload's two modifier rows, read off a
/// projection enum row's flattened record body (`…Project.mirror` /
/// `…Project.flip`). The modifiers are two-variant MODE enums
/// (`MirrorMode`/`FlipMode` — kept extensible), projected here to the
/// on/off the two-card row renders; a pick dispatches the generic
/// `EnsurePresent <row>.<Normal|Mirrored|Flipped>` enum gesture at the
/// row's address. `None` unless both rows exist — half a modifier pair
/// would render a control that cannot say what it writes.
fn modifier_rows(project_row: &UiConfigSlot) -> Option<UiSpaceModifiers> {
    Some(UiSpaceModifiers {
        mirror: mode_row(payload_field(project_row, "mirror")?, "Mirrored")?,
        flip: mode_row(payload_field(project_row, "flip")?, "Flipped")?,
    })
}

/// A two-variant mode enum row as the shared on/off row shape: `value` =
/// the on-variant is active.
fn mode_row(row: &UiConfigSlot, on_ident: &str) -> Option<UiSpaceBoolRow> {
    let Some(UiSlotComposite::Enum(composite)) = &row.composite else {
        return None;
    };
    Some(UiSpaceBoolRow {
        value: composite.active == on_ident,
        address: row.address.clone(),
        state: row.state.clone(),
    })
}

/// A bool value row as the shared [`UiSpaceBoolRow`] shape. `None` when
/// the row is not a boolean value row.
fn bool_row(row: &UiConfigSlot) -> Option<UiSpaceBoolRow> {
    let UiConfigSlotBody::Value(value) = &row.body else {
        return None;
    };
    let UiSlotValueKind::Bool(value) = value.kind else {
        return None;
    };
    Some(UiSpaceBoolRow {
        value,
        address: row.address.clone(),
        state: row.state.clone(),
    })
}

/// The fixture's `wire_reversed` bool row as the along-the-wire
/// [forward|reversed] row (the wire-reversed addendum). `None` when the
/// row is not a boolean value row (a pre-field project tree).
fn wire_direction_row(row: &UiConfigSlot) -> Option<UiWireDirectionRow> {
    let reversed = bool_row(row)?;
    Some(UiWireDirectionRow {
        reversed: reversed.value,
        address: reversed.address,
        state: reversed.state,
    })
}

/// Project the `strip_order_meaningful` bool row into the cell's
/// strip-order payload. `None` when the row is not a boolean value row.
fn strip_order_row(row: &UiConfigSlot) -> Option<UiSpaceBoolRow> {
    bool_row(row)
}

/// Top-level config row keys a derived face's space section has CLAIMED,
/// keyed by what the face actually carries — the config-row twin of
/// `face_claimed_debug_rows`. Declaration-driven per face arm, never a
/// global name rule: another kind may legitimately declare a `space` slot
/// and its drawer must keep working.
pub(in crate::app::project) fn claimed_config_rows(
    face: &crate::UiNodeFace,
) -> &'static [&'static str] {
    match face {
        crate::UiNodeFace::Shader(face) if face.space.is_some() => &[SHADER_SPACE_ROW],
        crate::UiNodeFace::Fixture(face) if face.space.is_some() => &[
            FIXTURE_CONSUME_ROW,
            FIXTURE_STRIP_ORDER_ROW,
            FIXTURE_WIRE_REVERSED_ROW,
        ],
        _ => &[],
    }
}

/// The flattened payload field named `field` under an enum row
/// (`space.OneD.in_2d` is a field of the `space` row, keyed by its full
/// path — hence the terminal-segment match rather than an equality test).
fn payload_field<'a>(row: &'a UiConfigSlot, field: &str) -> Option<&'a UiConfigSlot> {
    let UiConfigSlotBody::Record(record) = &row.body else {
        return None;
    };
    record
        .fields
        .iter()
        .find(|candidate| terminal_field(&candidate.key) == field)
}

/// The bare field name a row's key ends in: `space.OneD.in_2d` → `in_2d`.
fn terminal_field(key: &str) -> &str {
    let field = key.rsplit('.').next().unwrap_or(key);
    field.split('[').next().unwrap_or(field)
}

/// Project an enum config row into a space cell. `None` when the row is
/// not an enum composite (nothing to choose between) — the section then
/// simply carries one fewer cell rather than inventing one.
fn enum_cell(
    row: &UiConfigSlot,
    role: UiSpaceCellRole,
    label: &str,
    variant_label: fn(&str) -> String,
) -> Option<UiSpaceCell> {
    let Some(UiSlotComposite::Enum(composite)) = &row.composite else {
        return None;
    };
    let choices = composite
        .variants
        .iter()
        .map(|variant| UiSpaceChoice {
            variant: variant.clone(),
            label: variant_label(variant),
            projection: variant_projection(variant),
            selected: *variant == composite.active,
        })
        .collect();
    Some(UiSpaceCell {
        role,
        label: label.to_string(),
        active: composite.active.clone(),
        active_label: variant_label(&composite.active),
        choices,
        address: row.address.clone(),
        state: row.state.clone(),
        modifiers: None,
        wire_direction: None,
        strip_order: None,
    })
}

/// The space a `ShaderSpace` variant declares.
fn shader_declared_space(variant: &str) -> Option<UiVisualSpace> {
    match variant {
        "TwoD" => Some(UiVisualSpace::TwoD),
        "OneD" => Some(UiVisualSpace::OneD),
        _ => None,
    }
}

/// Display label for a `ShaderSpace` variant.
fn shader_space_label(variant: &str) -> String {
    match variant {
        "TwoD" => "2D".to_string(),
        "OneD" => "1D".to_string(),
        other => other.to_string(),
    }
}

/// Display label for a consumer-dropdown choice: the synthetic
/// `AlongWire` (the strip-order bit) and `Auto` ("follow the source's own
/// projection") entries plus the projections.
fn consumer_choice_label(variant: &str) -> String {
    match variant {
        CONSUMER_ALONG_WIRE_VARIANT => "along the wire".to_string(),
        "Auto" => "follow the source".to_string(),
        other => projection_label(other),
    }
}

/// Display label for a projection-shape variant (the factored
/// [`crate::UiProjectionShape`] vocabulary), plus `SpaceAnswer1`'s lone
/// `Default` (the centre-scanline statement — the single-variant `in_1d`
/// cell renders as a statement, not tiles).
fn projection_label(variant: &str) -> String {
    match variant {
        "Default" => "default".to_string(),
        "ExtrudeX" => "extrude-x".to_string(),
        "ExtrudeY" => "extrude-y".to_string(),
        "Radial" => "radial".to_string(),
        "Angular" => "angular".to_string(),
        other => other.to_string(),
    }
}

/// The projection a shape tile would force in a live probe — the PLAIN
/// shape (the modifier toggles refine it). `None` for the deferring
/// choices and the primary cell's own variants.
fn variant_projection(variant: &str) -> Option<UiCellProjection> {
    match variant {
        "ExtrudeX" => Some(UiCellProjection::plain(UiProjectionShape::ExtrudeX)),
        "ExtrudeY" => Some(UiCellProjection::plain(UiProjectionShape::ExtrudeY)),
        "Radial" => Some(UiCellProjection::plain(UiProjectionShape::Radial)),
        "Angular" => Some(UiCellProjection::plain(UiProjectionShape::Angular)),
        _ => None,
    }
}

/// The D1 mismatch, recovered from the node's error status text.
///
/// **Debt, deliberately taken (plan-B P3 item 5).** The declaration IS the
/// entry contract (`lp_shader::ShaderEntrySpace`), so a mismatch is a
/// plain `LpsError::Validation` that reaches Studio as an opaque status
/// string — there is no structured error class anywhere on the path
/// (`shader_node.compilation_error` → node status detail → `UiNodeHeader
/// ::detail`). Matching the compiler's two mismatch messages is therefore
/// the only surface available; when an error class arrives, this function
/// is the single place that changes. A message that does not match leaves
/// the section unflagged and the error keeps rendering in the code
/// drawer's strip, which is where it lands today.
fn space_mismatch(declared: UiVisualSpace, status_detail: Option<&str>) -> Option<UiSpaceMismatch> {
    let detail = status_detail?;
    let entry = if detail.contains(MISMATCH_ONE_D_DECLARED) {
        UiVisualSpace::TwoD
    } else if detail.contains(MISMATCH_TWO_D_DECLARED) {
        UiVisualSpace::OneD
    } else {
        return None;
    };
    Some(UiSpaceMismatch {
        declared,
        entry,
        message: detail.to_string(),
    })
}

/// `lp_shader`'s message for "declared 1D, found the 2D entry".
const MISMATCH_ONE_D_DECLARED: &str = "declared 1D but defines `render_2d`";
/// `lp_shader`'s message for "declared 2D, found the 1D entry".
const MISMATCH_TWO_D_DECLARED: &str = "declared 2D but defines `render_1d`";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ProjectNodeAddress, ProjectSlotAddress, ProjectSlotRoot, UiSlotEnumComposite,
        UiSlotFieldState, UiSlotValue,
    };
    use lpc_model::SlotPath;

    fn address(path: &str) -> ProjectSlotAddress {
        ProjectSlotAddress::new(
            ProjectNodeAddress::parse("/demo.module/aurora.shader").expect("address"),
            ProjectSlotRoot::def(),
            SlotPath::parse(path).expect("path"),
        )
    }

    fn enum_row(
        key: &str,
        active: &str,
        variants: &[&str],
        fields: Vec<UiConfigSlot>,
    ) -> UiConfigSlot {
        UiConfigSlot::record(key, key, fields)
            .with_address(address(key))
            .with_composite(UiSlotComposite::Enum(UiSlotEnumComposite {
                active: active.to_string(),
                variants: variants.iter().map(|name| name.to_string()).collect(),
            }))
    }

    fn bool_row(key: &str, value: bool) -> UiConfigSlot {
        UiConfigSlot::value(key, key, UiSlotValue::bool(value)).with_address(address(key))
    }

    /// A factored `Project` answer row (v9): the enum row whose flattened
    /// payload carries the shape enum and the two mode-enum modifiers.
    fn project_row(prefix: &str, shape: &str, mirror: bool, flip: bool) -> UiConfigSlot {
        enum_row(
            prefix,
            "Project",
            &["Project"],
            vec![
                enum_row(
                    &format!("{prefix}.Project.shape"),
                    shape,
                    &["ExtrudeX", "ExtrudeY", "Radial", "Angular"],
                    Vec::new(),
                ),
                enum_row(
                    &format!("{prefix}.Project.mirror"),
                    if mirror { "Mirrored" } else { "Normal" },
                    &["Normal", "Mirrored"],
                    Vec::new(),
                ),
                enum_row(
                    &format!("{prefix}.Project.flip"),
                    if flip { "Flipped" } else { "Normal" },
                    &["Normal", "Flipped"],
                    Vec::new(),
                ),
            ],
        )
    }

    /// A 1D shader's section (v9): the declaration, its factored 2D
    /// answer — the SHAPE row as the tile cell, the modifier bools as the
    /// toggles beneath.
    #[test]
    fn a_one_d_shader_declares_its_space_and_its_factored_answer() {
        let row = enum_row(
            "space",
            "OneD",
            &["TwoD", "OneD"],
            vec![project_row("space.OneD.in_2d", "Radial", false, true)],
        );
        let section = shader_space_section(&[&row], None).expect("section");

        assert_eq!(section.side, UiSpaceSide::Producer);
        assert_eq!(section.declared_space, Some(UiVisualSpace::OneD));
        assert_eq!(section.primary.active_label, "1D");
        assert!(section.primary.is_choosable());
        let answer = section
            .cell(UiSpaceCellRole::ProducerIn2d)
            .expect("the 2D answer cell");
        assert_eq!(answer.active, "Radial");
        assert_eq!(answer.active_label, "radial");
        assert_eq!(
            answer
                .choices
                .iter()
                .map(|choice| choice.projection)
                .collect::<Vec<_>>(),
            vec![
                Some(UiCellProjection::plain(UiProjectionShape::ExtrudeX)),
                Some(UiCellProjection::plain(UiProjectionShape::ExtrudeY)),
                Some(UiCellProjection::plain(UiProjectionShape::Radial)),
                Some(UiCellProjection::plain(UiProjectionShape::Angular)),
            ],
            "four shape tiles, no Default (retired with v9), each naming \
             its PLAIN projection — the modifiers refine it"
        );
        assert_eq!(
            answer
                .address
                .as_ref()
                .map(|address| address.path.to_string()),
            Some("space.OneD.in_2d.Project.shape".to_string()),
            "tiles dispatch at the flattened shape enum row"
        );
        let modifiers = answer.modifiers.as_ref().expect("the modifier rows");
        assert!(!modifiers.mirror.value);
        assert!(modifiers.flip.value);
        assert_eq!(
            modifiers
                .flip
                .address
                .as_ref()
                .map(|address| address.path.to_string()),
            Some("space.OneD.in_2d.Project.flip".to_string()),
            "toggles dispatch at the flattened bool rows"
        );

        assert!(
            section.cell(UiSpaceCellRole::ProducerIn1d).is_none(),
            "the inactive variant's payload is not in the tree"
        );
        assert!(
            section.primary.strip_order.is_none(),
            "the strip-order row is the consumer side's"
        );
    }

    /// A 2D shader's `in_1d` cell has exactly one declared variant today —
    /// a statement, not a picker. Untouched by the factorization.
    #[test]
    fn a_two_d_shader_answers_one_d_consumers_with_a_single_statement() {
        let row = enum_row(
            "space",
            "TwoD",
            &["TwoD", "OneD"],
            vec![enum_row(
                "space.TwoD.in_1d",
                "Default",
                &["Default"],
                Vec::new(),
            )],
        );
        let section = shader_space_section(&[&row], None).expect("section");

        assert_eq!(section.declared_space, Some(UiVisualSpace::TwoD));
        let answer = section
            .cell(UiSpaceCellRole::ProducerIn1d)
            .expect("the 1D answer cell");
        assert!(!answer.is_choosable(), "one variant is not a choice");
        assert!(answer.modifiers.is_none(), "a statement has no toggles");
    }

    /// A DEFAULT fixture (strip-order true, consume `Auto`) selects the
    /// along-the-wire entry, carrying the wire's [forward|reversed] row
    /// and NO modifier toggles (the projection is gated off there).
    #[test]
    fn a_default_fixture_selects_along_the_wire() {
        let rows = [
            bool_row("strip_order_meaningful", true),
            bool_row("wire_reversed", true),
            enum_row("consume", "Auto", &["Auto", "Policy"], Vec::new()),
        ];
        let rows: Vec<&UiConfigSlot> = rows.iter().collect();
        let section = fixture_space_section(&rows).expect("section");

        assert_eq!(section.side, UiSpaceSide::Consumer);
        assert_eq!(section.declared_space, None, "a fixture states a policy");
        assert_eq!(section.primary.active, CONSUMER_ALONG_WIRE_VARIANT);
        assert_eq!(section.primary.active_label, "along the wire");
        assert_eq!(
            section.primary.choices.len(),
            6,
            "along the wire, Auto, and the four shapes"
        );
        let strip = section.primary.strip_order.as_ref().expect("strip row");
        assert!(strip.value);
        let wire = section
            .primary
            .wire_direction
            .as_ref()
            .expect("the wire direction row");
        assert!(wire.reversed);
        assert_eq!(
            wire.address
                .as_ref()
                .map(|address| address.path.to_string()),
            Some("wire_reversed".to_string()),
        );
        assert!(
            section.primary.modifiers.is_none(),
            "no modifier toggles for a projection that cannot fire"
        );
        assert!(section.cells.is_empty(), "the primary IS the only cell");
    }

    /// With the bit false, the choice list falls back to the consume
    /// policy: `Auto` reads "follow the source", and no wire row shows.
    #[test]
    fn a_strip_order_false_auto_fixture_selects_follow_the_source() {
        let rows = [
            bool_row("strip_order_meaningful", false),
            bool_row("wire_reversed", false),
            enum_row("consume", "Auto", &["Auto", "Policy"], Vec::new()),
        ];
        let rows: Vec<&UiConfigSlot> = rows.iter().collect();
        let section = fixture_space_section(&rows).expect("section");
        assert_eq!(section.primary.active, "Auto");
        assert_eq!(section.primary.active_label, "follow the source");
        assert!(section.primary.wire_direction.is_none());
        assert!(
            section.primary.modifiers.is_none(),
            "Auto carries no payload rows, so no toggles yet"
        );
    }

    /// A true strip-order bit wins over an authored policy — the engine
    /// never reaches the projection when the bit is true.
    #[test]
    fn along_the_wire_wins_over_an_authored_policy() {
        let rows = [
            bool_row("strip_order_meaningful", true),
            enum_row(
                "consume",
                "Policy",
                &["Auto", "Policy"],
                vec![project_row(
                    "consume.Policy.from_1d",
                    "Radial",
                    false,
                    false,
                )],
            ),
        ];
        let rows: Vec<&UiConfigSlot> = rows.iter().collect();
        let section = fixture_space_section(&rows).expect("section");
        assert_eq!(section.primary.active, CONSUMER_ALONG_WIRE_VARIANT);
        assert!(
            section.primary.modifiers.is_none(),
            "no toggles for a projection that cannot fire"
        );
    }

    /// An authored policy (bit false) selects its factored shape in the
    /// same choice list and carries the modifier toggles.
    #[test]
    fn a_policy_fixture_selects_its_shape_with_modifiers() {
        let rows = [
            bool_row("strip_order_meaningful", false),
            enum_row(
                "consume",
                "Policy",
                &["Auto", "Policy"],
                vec![
                    project_row("consume.Policy.from_1d", "Angular", true, false),
                    bool_row("consume.Policy.force", true),
                ],
            ),
        ];
        let rows: Vec<&UiConfigSlot> = rows.iter().collect();
        let section = fixture_space_section(&rows).expect("section");

        assert_eq!(section.primary.active, "Angular");
        assert_eq!(section.primary.active_label, "angular");
        assert!(section.primary.is_choosable());
        let modifiers = section.primary.modifiers.as_ref().expect("toggles");
        assert!(modifiers.mirror.value);
        assert!(!modifiers.flip.value);
        assert_eq!(
            modifiers
                .mirror
                .address
                .as_ref()
                .map(|address| address.path.to_string()),
            Some("consume.Policy.from_1d.Project.mirror".to_string()),
        );
    }

    /// Without a strip-order row there is nothing for the along-the-wire
    /// choice to write, so it is not offered.
    #[test]
    fn a_missing_strip_row_drops_the_along_the_wire_choice() {
        let rows = [enum_row("consume", "Auto", &["Auto", "Policy"], Vec::new())];
        let rows: Vec<&UiConfigSlot> = rows.iter().collect();
        let section = fixture_space_section(&rows).expect("section");
        assert_eq!(section.primary.choices.len(), 5);
        assert_eq!(section.primary.active, "Auto");
        assert!(section.primary.strip_order.is_none());
    }

    /// D1: the compiler's mismatch message becomes a structured pair. The
    /// declared side comes from the SLOT (what the project says), the
    /// entry side from the message (what the GLSL says).
    #[test]
    fn a_declared_one_d_shader_defining_render_2d_flags_the_mismatch() {
        let row = enum_row(
            "space",
            "OneD",
            &["TwoD", "OneD"],
            vec![project_row("space.OneD.in_2d", "ExtrudeX", false, false)],
        );
        let detail = "shader compile: declared 1D but defines `render_2d`: a 1D-declared \
                      shader's entry is `vec4 render_1d(float pos)`";
        let section = shader_space_section(&[&row], Some(detail)).expect("section");

        let mismatch = section.mismatch.expect("the mismatch is on the section");
        assert_eq!(mismatch.declared, UiVisualSpace::OneD);
        assert_eq!(mismatch.entry, UiVisualSpace::TwoD);
        assert_eq!(mismatch.message, detail, "the raw text stays available");
    }

    /// An unrelated compile error is NOT a space mismatch — the section
    /// stays clean and the code drawer keeps the error.
    #[test]
    fn an_unrelated_compile_error_leaves_the_section_unflagged() {
        let row = enum_row("space", "TwoD", &["TwoD", "OneD"], Vec::new());
        let section = shader_space_section(
            &[&row],
            Some("shader compile: 3:11: undeclared identifier `tim`"),
        )
        .expect("section");
        assert!(section.mismatch.is_none());
    }

    /// Claiming is declaration-driven: a face with no section claims
    /// nothing, so a kind that happens to declare a `space` slot keeps its
    /// drawer rows.
    #[test]
    fn claimed_rows_follow_the_face_that_carries_a_section() {
        let mut shader = crate::UiShaderFace {
            preview: crate::UiProducedProduct::visual("output"),
            controls: Vec::new(),
            agent: None,
            code_drawer: None,
            space: None,
        };
        assert!(claimed_config_rows(&crate::UiNodeFace::Shader(shader.clone())).is_empty());
        shader.space = Some(UiSpaceSection {
            side: UiSpaceSide::Producer,
            primary: UiSpaceCell {
                role: UiSpaceCellRole::Primary,
                label: "Space".to_string(),
                active: "TwoD".to_string(),
                active_label: "2D".to_string(),
                choices: Vec::new(),
                address: None,
                state: UiSlotFieldState::editable(),
                modifiers: None,
                wire_direction: None,
                strip_order: None,
            },
            declared_space: Some(UiVisualSpace::TwoD),
            cells: Vec::new(),
            mismatch: None,
        });
        assert_eq!(
            claimed_config_rows(&crate::UiNodeFace::Shader(shader)),
            &[SHADER_SPACE_ROW]
        );
    }
}
