//! Derivation of the `space` section both visual-side faces carry
//! ([`UiSpaceSection`]) — the two-sided space model lifted out of the
//! advanced drawer and onto the card (dimensionality plan-B P3).
//!
//! One derivation, two sides. The shader's `space` enum
//! (`TwoD { in_1d } | OneD { in_2d }`) and the fixture's `consume` enum
//! (`Auto | Policy { from_1d, force }`) plus `strip_order_meaningful` land
//! in the SAME DTO, so D13's "the two sections are visual mirrors" is a
//! data fact the web cannot accidentally break.
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
    UiCellProjection, UiConfigSlot, UiConfigSlotBody, UiSlotComposite, UiSlotValueKind,
    UiSpaceCell, UiSpaceCellRole, UiSpaceChoice, UiSpaceFlag, UiSpaceFlagRole, UiSpaceMismatch,
    UiSpaceSection, UiSpaceSide, UiVisualSpace,
};

/// The shader def's producer-side declaration row.
pub(in crate::app::project) const SHADER_SPACE_ROW: &str = "space";
/// The fixture def's consumer-side policy row.
pub(in crate::app::project) const FIXTURE_CONSUME_ROW: &str = "consume";
/// The fixture def's "does strip order mean something?" row (vision D3).
pub(in crate::app::project) const FIXTURE_STRIP_ORDER_ROW: &str = "strip_order_meaningful";

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
        enum_cell(payload_field(row, field)?, role, label, projection_label)
    })
    .collect();
    Some(UiSpaceSection {
        side: UiSpaceSide::Producer,
        primary,
        declared_space,
        cells,
        flags: Vec::new(),
        mismatch: declared_space.and_then(|declared| space_mismatch(declared, status_detail)),
    })
}

/// The consumer side's section: the fixture's `consume` policy (with
/// `Auto` as the unexpanded state — a unit variant contributes no payload
/// rows at all) plus the strip-order bit.
///
/// Derives as soon as EITHER row is present: the strip-order question is a
/// section-worthy declaration on its own (D3), and a fixture whose
/// `consume` row has not landed should still be able to answer it.
pub(in crate::app::project) fn fixture_space_section(
    rows: &[&UiConfigSlot],
) -> Option<UiSpaceSection> {
    let row = rows
        .iter()
        .copied()
        .find(|row| row.key == FIXTURE_CONSUME_ROW)?;
    let primary = enum_cell(
        row,
        UiSpaceCellRole::Primary,
        "Consume",
        consumer_space_label,
    )?;
    let cells = payload_field(row, "from_1d")
        .and_then(|field| {
            enum_cell(
                field,
                UiSpaceCellRole::ConsumerFrom1d,
                "From 1D sources",
                projection_label,
            )
        })
        .into_iter()
        .collect();
    let mut flags = Vec::new();
    if let Some(strip) = rows
        .iter()
        .copied()
        .find(|row| row.key == FIXTURE_STRIP_ORDER_ROW)
        .and_then(|row| {
            bool_flag(
                row,
                UiSpaceFlagRole::StripOrderMeaningful,
                "Strip order means something",
            )
        })
    {
        flags.push(strip);
    }
    if let Some(force) = payload_field(row, "force")
        .and_then(|field| bool_flag(field, UiSpaceFlagRole::ForcePolicy, "Force"))
    {
        flags.push(force);
    }
    Some(UiSpaceSection {
        side: UiSpaceSide::Consumer,
        primary,
        // A fixture states a policy, never a space: its own dimensionality
        // comes from its mapping, not from this section.
        declared_space: None,
        cells,
        flags,
        mismatch: None,
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
        crate::UiNodeFace::Fixture(face) if face.space.is_some() => {
            &[FIXTURE_CONSUME_ROW, FIXTURE_STRIP_ORDER_ROW]
        }
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
    })
}

/// Project a bool config row into a space flag. `None` when the row is not
/// a boolean value row.
fn bool_flag(row: &UiConfigSlot, role: UiSpaceFlagRole, label: &str) -> Option<UiSpaceFlag> {
    let UiConfigSlotBody::Value(value) = &row.body else {
        return None;
    };
    let UiSlotValueKind::Bool(value) = value.kind else {
        return None;
    };
    Some(UiSpaceFlag {
        role,
        label: label.to_string(),
        value,
        address: row.address.clone(),
        state: row.state.clone(),
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

/// Display label for a `VisualConsumerSpace` variant.
fn consumer_space_label(variant: &str) -> String {
    match variant {
        "Auto" => "auto".to_string(),
        "Policy" => "policy".to_string(),
        other => other.to_string(),
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
/// cell's own variants.
fn variant_projection(variant: &str) -> Option<UiCellProjection> {
    match variant {
        "Extrude" => Some(UiCellProjection::Extrude),
        "Radial" => Some(UiCellProjection::Radial),
        "Angular" => Some(UiCellProjection::Angular),
        "Mirror" => Some(UiCellProjection::Mirror),
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
                Some(UiCellProjection::Extrude),
                Some(UiCellProjection::Radial),
                Some(UiCellProjection::Angular),
                Some(UiCellProjection::Mirror),
            ],
            "every projecting choice names the cell a live tile forces"
        );
        assert!(
            section.cell(UiSpaceCellRole::ProducerIn1d).is_none(),
            "the inactive variant's payload is not in the tree"
        );
        assert!(section.flags.is_empty(), "flags are the consumer side's");
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

    /// `Auto` is the unexpanded consumer state: a unit variant carries no
    /// payload rows, so the section is the primary cell plus the
    /// strip-order flag and nothing else.
    #[test]
    fn an_auto_fixture_section_is_the_primary_cell_and_the_strip_order_flag() {
        let rows = [
            bool_row("strip_order_meaningful", true),
            enum_row("consume", "Auto", &["Auto", "Policy"], Vec::new()),
        ];
        let rows: Vec<&UiConfigSlot> = rows.iter().collect();
        let section = fixture_space_section(&rows).expect("section");

        assert_eq!(section.side, UiSpaceSide::Consumer);
        assert_eq!(section.declared_space, None, "a fixture states a policy");
        assert_eq!(section.primary.active_label, "auto");
        assert!(section.cells.is_empty());
        assert_eq!(section.flags.len(), 1);
        let strip = section
            .flag(UiSpaceFlagRole::StripOrderMeaningful)
            .expect("the strip-order flag");
        assert!(strip.value);
        assert_eq!(
            strip
                .address
                .as_ref()
                .map(|address| address.path.to_string()),
            Some("strip_order_meaningful".to_string()),
        );
    }

    /// An authored policy expands into its default-projection cell and the
    /// inline force bit.
    #[test]
    fn a_policy_fixture_expands_into_its_cell_and_force_flag() {
        let rows = [
            bool_row("strip_order_meaningful", false),
            enum_row(
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
            ),
        ];
        let rows: Vec<&UiConfigSlot> = rows.iter().collect();
        let section = fixture_space_section(&rows).expect("section");

        let cell = section
            .cell(UiSpaceCellRole::ConsumerFrom1d)
            .expect("the from-1D cell");
        assert_eq!(cell.active_label, "mirror");
        assert_eq!(cell.choices.len(), 4, "no Default on the consumer side");
        assert!(cell.is_choosable());
        let force = section
            .flag(UiSpaceFlagRole::ForcePolicy)
            .expect("the force flag");
        assert!(force.value);
        assert!(
            !section
                .flag(UiSpaceFlagRole::StripOrderMeaningful)
                .expect("the strip-order flag")
                .value
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
            },
            declared_space: Some(UiVisualSpace::TwoD),
            cells: Vec::new(),
            flags: Vec::new(),
            mismatch: None,
        });
        assert_eq!(
            claimed_config_rows(&crate::UiNodeFace::Shader(shader)),
            &[SHADER_SPACE_ROW]
        );
    }
}
