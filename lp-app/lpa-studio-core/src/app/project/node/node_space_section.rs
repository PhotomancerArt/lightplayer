//! Derivation of the `space` section both visual-side faces carry
//! ([`UiSpaceSection`]) — the two-sided space model lifted out of the
//! advanced drawer and onto the card (dimensionality plan-B P3).
//!
//! One derivation, two sides. The shader's `space` enum
//! (`TwoD { in_1d } | OneD { in_2d }`) and the fixture's `consume` enum
//! (`Auto | Policy { from_1d, force }`) land in the SAME DTO, so D13's
//! "the two sections are visual mirrors" is a data fact the web cannot
//! accidentally break. (The fixture's `strip_order_meaningful` bit was
//! ruled OFF this surface 2026-08-09 — the patching vision's declared
//! fixture space will absorb it; until then the raw bool lives in the
//! advanced drawer, unclaimed.)
//!
//! Everything is read off the already-projected config rows — the same
//! rows the advanced drawer would render, which is exactly why the face
//! then CLAIMS them ([`claimed_config_rows`]): the section IS their
//! surface now, and two controls writing one slot is the defect this
//! avoids. No gesture is invented here either: a cell carries its enum
//! row's address so a choice is the `EnsurePresent <enum>.<Variant>` the
//! generic variant field already dispatches, and a flag carries its bool
//! row's address for the ordinary `SetValue`.
//!
//! Enum payload rows arrive FLATTENED (`SlotController` hoists a variant's
//! record fields to the enum row's own record body), so the shader's
//! answer cell is a field of the `space` row keyed `space.OneD.in_2d` —
//! there is no intermediate variant row to descend through.

use crate::{
    UiCellProjection, UiConfigSlot, UiConfigSlotBody, UiMirrorDirection, UiProjectionDirection,
    UiSlotComposite, UiSpaceCell, UiSpaceCellRole, UiSpaceChoice, UiSpaceDirection,
    UiSpaceMismatch, UiSpaceSection, UiSpaceSide, UiVisualSpace,
};

/// The shader def's producer-side declaration row.
pub(in crate::app::project) const SHADER_SPACE_ROW: &str = "space";
/// The fixture def's consumer-side policy row.
pub(in crate::app::project) const FIXTURE_CONSUME_ROW: &str = "consume";

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
    let primary = enum_cell(row, UiSpaceCellRole::Primary, "Space", shader_space_label)?;
    let declared_space = shader_declared_space(&primary.active);
    // The answer cell is whichever the ACTIVE variant declares: a 1D
    // shader answers 2D consumers, a 2D shader answers 1D ones. Only one
    // exists at a time — the other variant's payload is not in the tree.
    let cells = [
        (UiSpaceCellRole::ProducerIn2d, "in_2d", "Default projection"),
        (UiSpaceCellRole::ProducerIn1d, "in_1d", "To 1D consumers"),
    ]
    .into_iter()
    .filter_map(|(role, field, label)| {
        let answer_row = payload_field(row, field)?;
        let mut cell = enum_cell(answer_row, role, label, projection_label)?;
        // The two-section shape+direction design (G1b ruling 4): when the
        // ACTIVE shape is directional, its flattened `direction` payload
        // row (`…in_2d.Extrude.direction`) becomes the cell's direction
        // row — a real address for the segmented control to dispatch at.
        cell.direction = direction_cell(answer_row);
        Some(cell)
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
/// `consume` policy. (The strip-order bit was ruled off this surface
/// 2026-08-09; its raw bool renders in the advanced drawer until the
/// patching vision's declared fixture space absorbs it.)
///
/// **G1 rework (plan-B P4b).** G1 ruled the `Auto`/`Policy` split
/// backwards as a surface: "use the dropdown with the 'default' option…
/// then we just have one control." So the primary cell IS the projection
/// choice — a synthetic `Auto` entry ("follow the source") plus the four
/// [`ConsumerCell2`] projections. The projection variants are the model's
/// static vocabulary rather than tree rows because under `Auto` the
/// `from_1d` payload is not in the tree at all, and the dropdown must
/// still offer them. The web dispatches per choice: `Auto` is the plain
/// `EnsurePresent consume.Auto`; a projection is the
/// `EnsurePresent consume.Policy` → `EnsurePresent
/// consume.Policy.from_1d.<V>` → `SetValue consume.Policy.force = true`
/// sequence — every op one the generic drawer rows already send, so this
/// stays a presentation of the same write path. `force` is absorbed by
/// that gesture (an explicit pick IS the override), so the flag is no
/// longer emitted.
pub(in crate::app::project) fn fixture_space_section(
    rows: &[&UiConfigSlot],
) -> Option<UiSpaceSection> {
    let row = rows
        .iter()
        .copied()
        .find(|row| row.key == FIXTURE_CONSUME_ROW)?;
    let primary = consumer_projection_cell(row)?;
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
const CONSUMER_PROJECTION_VARIANTS: [&str; 4] = ["Extrude", "Radial", "Angular", "Mirror"];

/// The one consumer cell: `Auto` ("follow the source") + the projections,
/// selected from the active `from_1d` when a policy is authored.
fn consumer_projection_cell(row: &UiConfigSlot) -> Option<UiSpaceCell> {
    let Some(UiSlotComposite::Enum(composite)) = &row.composite else {
        return None;
    };
    let from_1d = payload_field(row, "from_1d");
    let active = if composite.active == "Auto" {
        "Auto".to_string()
    } else {
        from_1d
            .and_then(|field| match &field.composite {
                Some(UiSlotComposite::Enum(from)) => Some(from.active.clone()),
                _ => None,
            })
            .unwrap_or_else(|| "Auto".to_string())
    };
    let choices = std::iter::once("Auto")
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
        // The direction row lives under `consume.Policy.from_1d.<Shape>`
        // (G1b ruling 4). While the fixture is still in `Auto` the payload
        // rows are not in the tree, so there is no row to derive — it
        // appears once a directional shape is picked, which is fine.
        direction: from_1d.and_then(direction_cell),
    })
}

/// The ACTIVE shape's flattened `direction` payload row under a
/// projection enum row, as the cell's direction row (G1b ruling 4).
/// `None` when the active shape carries no direction payload
/// (default/radial/angular, or a pre-directional tree).
fn direction_cell(row: &UiConfigSlot) -> Option<UiSpaceDirection> {
    let direction_row = payload_field(row, "direction")?;
    let Some(UiSlotComposite::Enum(composite)) = &direction_row.composite else {
        return None;
    };
    Some(UiSpaceDirection {
        active: composite.active.clone(),
        variants: composite.variants.clone(),
        address: direction_row.address.clone(),
        state: direction_row.state.clone(),
    })
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
        crate::UiNodeFace::Fixture(face) if face.space.is_some() => &[FIXTURE_CONSUME_ROW],
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
        direction: None,
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

/// Display label for a consumer-dropdown choice: the synthetic `Auto`
/// entry ("follow the source's own projection") plus the projections.
fn consumer_choice_label(variant: &str) -> String {
    match variant {
        "Auto" => "follow the source".to_string(),
        other => projection_label(other),
    }
}

/// Display label for a projection-answer variant (`SpaceAnswer1`,
/// `SpaceAnswer2`, `ConsumerCell2` all share this vocabulary).
///
/// `Default` reads differently on the two answer cells — "consumer
/// decides" for a 1D source's 2D answer, "centre scanline" for a 2D
/// source's 1D one — but the single-variant `in_1d` cell renders as a
/// statement rather than a picker anyway
/// ([`UiSpaceCell::is_choosable`]), so one label covers both without
/// splitting the vocabulary per cell.
fn projection_label(variant: &str) -> String {
    match variant {
        "Default" => "default".to_string(),
        "Extrude" => "extrude".to_string(),
        "Radial" => "radial".to_string(),
        "Angular" => "angular".to_string(),
        "Mirror" => "mirror".to_string(),
        other => other.to_string(),
    }
}

/// The projection a variant would force in a live tile probe. `None` for
/// `Default` (which defers rather than projecting) and for the primary
/// cell's own variants. The picker's tiles are SHAPE tiles (G1b ruling 4:
/// two sections, never a flattened 8-tile grid), so a directional shape
/// probes at the default `Right` here; the direction row refines it.
fn variant_projection(variant: &str) -> Option<UiCellProjection> {
    match variant {
        "Extrude" => Some(UiCellProjection::Extrude(UiProjectionDirection::Right)),
        "Radial" => Some(UiCellProjection::Radial),
        "Angular" => Some(UiCellProjection::Angular),
        "Mirror" => Some(UiCellProjection::Mirror(UiMirrorDirection::OutwardX)),
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

    /// A 1D shader's section: the declaration, its 2D answer cell, and a
    /// projection per choice for the tile picker.
    #[test]
    fn a_one_d_shader_declares_its_space_and_its_two_d_answer() {
        let row = enum_row(
            "space",
            "OneD",
            &["TwoD", "OneD"],
            vec![enum_row(
                "space.OneD.in_2d",
                "Radial",
                &["Default", "Extrude", "Radial", "Angular", "Mirror"],
                Vec::new(),
            )],
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
                None,
                Some(UiCellProjection::Extrude(UiProjectionDirection::Right)),
                Some(UiCellProjection::Radial),
                Some(UiCellProjection::Angular),
                Some(UiCellProjection::Mirror(UiMirrorDirection::OutwardX)),
            ],
            "every projecting choice names the cell a live tile forces"
        );
        assert!(
            section.cell(UiSpaceCellRole::ProducerIn1d).is_none(),
            "the inactive variant's payload is not in the tree"
        );
    }

    /// A 2D shader's `in_1d` cell has exactly one declared variant today —
    /// a statement, not a picker.
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
        assert_eq!(
            answer
                .address
                .as_ref()
                .map(|address| address.path.to_string()),
            Some("space.TwoD.in_1d".to_string()),
            "the cell dispatches at the enum row it was derived from"
        );
    }

    /// The consumer section is ONE dropdown (P4b): `Auto` renders as the
    /// selected "follow the source" entry, with the four projections
    /// offered even though `Auto` carries no payload rows in the tree.
    #[test]
    fn an_auto_fixture_is_one_dropdown_selecting_follow_the_source() {
        let rows = [
            // The strip-order bool rides along UNCLAIMED (ruled off this
            // surface 2026-08-09): the derivation must ignore it.
            bool_row("strip_order_meaningful", true),
            enum_row("consume", "Auto", &["Auto", "Policy"], Vec::new()),
        ];
        let rows: Vec<&UiConfigSlot> = rows.iter().collect();
        let section = fixture_space_section(&rows).expect("section");

        assert_eq!(section.side, UiSpaceSide::Consumer);
        assert_eq!(section.declared_space, None, "a fixture states a policy");
        assert_eq!(section.primary.active, "Auto");
        assert_eq!(section.primary.active_label, "follow the source");
        assert_eq!(
            section.primary.choices.len(),
            5,
            "Auto plus the four static projections"
        );
        assert!(
            section.primary.is_choosable(),
            "the dropdown is live from the Auto state"
        );
        assert_eq!(
            section
                .primary
                .address
                .as_ref()
                .map(|address| address.path.to_string()),
            Some("consume".to_string()),
            "dispatch targets the consume enum row"
        );
        assert!(section.cells.is_empty(), "the primary IS the only cell");
    }

    /// An authored policy selects its `from_1d` in the same dropdown, and
    /// the force bit is absorbed by the gesture rather than surfaced as a
    /// flag (an explicit pick IS the override).
    #[test]
    fn a_policy_fixture_selects_its_projection_in_the_one_dropdown() {
        let rows = [enum_row(
            "consume",
            "Policy",
            &["Auto", "Policy"],
            vec![
                enum_row(
                    "consume.Policy.from_1d",
                    "Mirror",
                    &["Extrude", "Radial", "Angular", "Mirror"],
                    Vec::new(),
                ),
                bool_row("consume.Policy.force", true),
            ],
        )];
        let rows: Vec<&UiConfigSlot> = rows.iter().collect();
        let section = fixture_space_section(&rows).expect("section");

        assert_eq!(section.primary.active, "Mirror");
        assert_eq!(section.primary.active_label, "mirror");
        assert!(section.primary.is_choosable());
    }

    /// G1b ruling 4's second section: an ACTIVE directional shape's
    /// flattened `direction` payload row becomes the cell's direction row
    /// — real address, real active variant. Radial (direction-free) and a
    /// pre-directional tree (no payload row) derive none.
    #[test]
    fn a_directional_answer_carries_its_direction_row() {
        let row = enum_row(
            "space",
            "OneD",
            &["TwoD", "OneD"],
            vec![enum_row(
                "space.OneD.in_2d",
                "Mirror",
                &["Default", "Extrude", "Radial", "Angular", "Mirror"],
                vec![enum_row(
                    "space.OneD.in_2d.Mirror.direction",
                    "InwardY",
                    &["InwardX", "OutwardX", "InwardY", "OutwardY"],
                    Vec::new(),
                )],
            )],
        );
        let section = shader_space_section(&[&row], None).expect("section");
        let answer = section
            .cell(UiSpaceCellRole::ProducerIn2d)
            .expect("the 2D answer cell");
        let direction = answer.direction.as_ref().expect("the direction row");
        assert_eq!(direction.active, "InwardY");
        assert_eq!(
            direction.variants,
            vec!["InwardX", "OutwardX", "InwardY", "OutwardY"],
            "the row carries the SHAPE's own vocabulary, read from the tree"
        );
        assert_eq!(
            direction
                .address
                .as_ref()
                .map(|address| address.path.to_string()),
            Some("space.OneD.in_2d.Mirror.direction".to_string()),
            "the row dispatches at the flattened direction enum row"
        );

        let radial = enum_row(
            "space",
            "OneD",
            &["TwoD", "OneD"],
            vec![enum_row(
                "space.OneD.in_2d",
                "Radial",
                &["Default", "Extrude", "Radial", "Angular", "Mirror"],
                Vec::new(),
            )],
        );
        let section = shader_space_section(&[&radial], None).expect("section");
        assert!(
            section
                .cell(UiSpaceCellRole::ProducerIn2d)
                .expect("cell")
                .direction
                .is_none(),
            "a direction-free shape has no direction row"
        );
    }

    /// The consumer mirror: an authored directional policy carries the
    /// direction row under `consume.Policy.from_1d.<Shape>`; an `Auto`
    /// fixture has no payload rows and therefore no row yet (fine — it
    /// appears once a directional shape is picked).
    #[test]
    fn a_directional_consumer_policy_carries_its_direction_row() {
        let rows = [enum_row(
            "consume",
            "Policy",
            &["Auto", "Policy"],
            vec![enum_row(
                "consume.Policy.from_1d",
                "Extrude",
                &["Extrude", "Radial", "Angular", "Mirror"],
                vec![enum_row(
                    "consume.Policy.from_1d.Extrude.direction",
                    "Left",
                    &["Right", "Left", "Down", "Up"],
                    Vec::new(),
                )],
            )],
        )];
        let rows: Vec<&UiConfigSlot> = rows.iter().collect();
        let section = fixture_space_section(&rows).expect("section");
        let direction = section
            .primary
            .direction
            .as_ref()
            .expect("the direction row");
        assert_eq!(direction.active, "Left");
        assert_eq!(
            direction
                .address
                .as_ref()
                .map(|address| address.path.to_string()),
            Some("consume.Policy.from_1d.Extrude.direction".to_string()),
        );

        let auto = [enum_row("consume", "Auto", &["Auto", "Policy"], Vec::new())];
        let auto: Vec<&UiConfigSlot> = auto.iter().collect();
        assert!(
            fixture_space_section(&auto)
                .expect("section")
                .primary
                .direction
                .is_none(),
            "Auto carries no payload rows, so no direction row yet"
        );
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
            vec![enum_row(
                "space.OneD.in_2d",
                "Default",
                &["Default"],
                Vec::new(),
            )],
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
                direction: None,
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
