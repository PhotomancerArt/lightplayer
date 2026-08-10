//! Project-side lookups the shader agent's run start needs.
//!
//! Small data carriers only — the resolution logic lives on
//! [`ProjectController`](super::project_controller::ProjectController)
//! (`agent_shader_target`, `agent_fixture_defs`, `agent_engine_status`)
//! because it reads private controller state (node tree, def-artifact map).

use lpa_agent::{DeclaredSpace, EngineStatusKind, EngineVerdict, ParamUpsert, SpaceDeclaration};
use lpc_model::{
    LpValue, PhasorConfig, Revision, SlotEdit, SlotMapKey, SlotName, SlotPath, ToLpValue, Waveform,
};

use crate::app::project::node::node_space_section::SHADER_SPACE_ROW;
use crate::{ProjectNodeStatusTone, ProjectNodeStatusView, UiShaderError};

/// The shader node behind one source artifact, resolved for a run start.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AgentShaderTarget {
    /// The node's stable address display (the session-key half).
    pub node_address: String,
    /// Human-readable node label (system-prompt context).
    pub node_label: String,
    /// The declared uniform bindings with their authored defaults.
    pub bindings: Vec<AgentShaderBinding>,
    /// The node's declared space, which the system prompt's entry-point
    /// line is derived from (telling a 1D node its entry is `render_2d`
    /// breaks it on the agent's first edit).
    pub space: DeclaredSpace,
}

/// Read a shader node's declared space off its `space` enum slot.
///
/// Falls back to [`DeclaredSpace::TwoD`] when the slot is absent or
/// unreadable — which is the MODEL's own default, so a node with no
/// declaration gets the prompt it would have gotten before this existed.
pub(crate) fn declared_space(variant: Option<&str>) -> DeclaredSpace {
    match variant {
        Some("OneD") => DeclaredSpace::OneD,
        _ => DeclaredSpace::TwoD,
    }
}

/// The engine's latest status for one shader node, as a verdict the agent
/// bridge serves: the status Revision (`change_frame`) plus the parsed
/// classification. Written into the bridge cell on every pull.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentEngineStatus {
    /// The engine frame the status last changed at.
    pub revision: Revision,
    pub verdict: EngineVerdict,
}

/// Map one node status view to the agent-facing verdict. Error tones parse
/// through [`UiShaderError`] (one prefix-strip implementation for the error
/// strip AND the agent); warnings stay `Ok` with their text; neutral states
/// (pending/created) are `Unknown`.
pub(crate) fn engine_verdict(status: &ProjectNodeStatusView) -> EngineVerdict {
    match status.tone {
        ProjectNodeStatusTone::Error => match status.detail.as_deref() {
            Some(detail) => {
                let parsed = UiShaderError::parse(detail);
                EngineVerdict {
                    status: EngineStatusKind::Error,
                    message: Some(parsed.message),
                    line_col: parsed.line_col,
                }
            }
            None => EngineVerdict {
                status: EngineStatusKind::Error,
                message: Some(status.label.clone()),
                line_col: None,
            },
        },
        ProjectNodeStatusTone::Good | ProjectNodeStatusTone::Warning => EngineVerdict {
            status: EngineStatusKind::Ok,
            message: status.detail.clone(),
            line_col: None,
        },
        ProjectNodeStatusTone::Neutral => EngineVerdict {
            status: EngineStatusKind::Unknown,
            message: None,
            line_col: None,
        },
        // The agent gets the build gap as a message, not as an error: no
        // amount of shader editing fixes a missing node runtime.
        ProjectNodeStatusTone::Disabled => EngineVerdict {
            status: EngineStatusKind::Unknown,
            message: status.detail.clone(),
            line_col: None,
        },
    }
}

/// The `SlotEdit` list one `upsert_param` dispatches, in order: ensure the
/// `consumed[name]` entry exists (the server constructs the record default
/// — kind/value included, f32 in v1), then per present field the same ops
/// the human gestures send (`AssignValue` on the `label`/`kind` value
/// slots; `EnsurePresent` + `AssignValue` on each option's `.some`). A
/// `kind: "phasor"` upsert also (re)writes the whole `phasor.some`
/// [`PhasorConfig`] in one `AssignValue` — it is a leaf struct, not a
/// further-decomposed slot — built from `period_seconds`/`waveform`/
/// `phase_offset` over [`PhasorConfig::default`] for whatever the call
/// omitted; the tool layer rejects those three fields unless `kind` is
/// `"phasor"` in the same call, so partial phasor edits always restate the
/// kind. All edits ride ONE `MutationCmdBatch` of `PutSlotEdit`s on the def
/// artifact, the same batch shape as `apply_asset_body`.
pub(crate) fn param_upsert_edits(upsert: &ParamUpsert) -> Vec<SlotEdit> {
    fn field(path: &SlotPath, name: &str) -> SlotPath {
        path.child(SlotName::parse(name).expect("static field name"))
    }

    let entry = SlotPath::parse("consumed")
        .expect("static path")
        .child_key(SlotMapKey::String(upsert.name.clone()));
    let mut edits = vec![SlotEdit::ensure_present(entry.clone())];
    if let Some(label) = &upsert.label {
        edits.push(SlotEdit::assign_value(
            field(&entry, "label"),
            LpValue::String(label.clone()),
        ));
    }
    if let Some(kind) = &upsert.kind {
        edits.push(SlotEdit::assign_value(
            field(&entry, "kind"),
            LpValue::String(kind.clone()),
        ));
    }
    let mut option = |name: &str, value: LpValue| {
        let some = field(&field(&entry, name), "some");
        edits.push(SlotEdit::ensure_present(some.clone()));
        edits.push(SlotEdit::assign_value(some, value));
    };
    for (name, value) in [
        ("default", upsert.default),
        ("min", upsert.min),
        ("max", upsert.max),
        ("step", upsert.step),
    ] {
        if let Some(value) = value {
            option(name, LpValue::F32(value));
        }
    }
    if let Some(unit) = &upsert.unit {
        option("unit", LpValue::String(unit.clone()));
    }
    if upsert.kind.as_deref() == Some("phasor") {
        let config = PhasorConfig {
            period_seconds: upsert
                .period_seconds
                .unwrap_or(PhasorConfig::default().period_seconds),
            waveform: upsert
                .waveform
                .as_deref()
                .and_then(Waveform::parse)
                .unwrap_or_default(),
            phase_offset: upsert.phase_offset.unwrap_or(0.0),
        };
        option("phasor", config.to_lp_value());
    }
    edits
}

/// The `SlotEdit` list one `declare_space` dispatches — THE single
/// spelling of "how a space declaration is written", shared with the
/// dimensionality section rather than a second write path.
///
/// **One writer.** The section's tiles carry slot ADDRESSES and dispatch
/// the generic `EnsurePresent <enum row>.<Variant>` enum gesture
/// (`node_space_section` derives the addresses; `space_section.rs`
/// dispatches). This function spells the SAME ops for the addresses the
/// agent cannot read off the tree — writing `space.OneD` and its payload
/// in one call means the payload rows do not exist yet to be addressed.
/// `agent_edits_match_the_dimensionality_tiles` in
/// [`node_space_section`](super::node) pins every path here against the
/// derivation's own addresses, so the two cannot drift apart silently.
///
/// The ops, in the order a user clicking the section would send them:
/// declare the space, then (1D only, per present field) the projection's
/// shape and its two modifier modes. `EnsurePresent` on an already-active
/// variant is a no-op, so re-declaring `1d` keeps the projection the user
/// already authored — exactly what re-clicking the 1D tab does. All edits
/// ride ONE `MutationCmdBatch` of `PutSlotEdit`s on the def artifact, the
/// same batch shape as [`param_upsert_edits`].
pub(crate) fn space_declaration_edits(declaration: &SpaceDeclaration) -> Vec<SlotEdit> {
    /// The shader def's declaration row — the row the producer-side
    /// section reads and claims.
    fn space_row() -> SlotPath {
        SlotPath::parse(SHADER_SPACE_ROW).expect("static path")
    }
    fn field(path: &SlotPath, name: &str) -> SlotPath {
        path.child(SlotName::parse(name).expect("static field name"))
    }

    // The declaration itself: `EnsurePresent space.<TwoD|OneD>`, the very
    // op the primary cell's tab pair dispatches.
    let variant = field(&space_row(), declaration.space.variant_ident());
    let mut edits = vec![SlotEdit::ensure_present(variant.clone())];

    // A 2D declaration has nothing further to author: `SpaceAnswer1` is a
    // single-variant statement, which is why the section renders it as
    // text rather than tiles. The tool layer refuses projection fields
    // here, so there is nothing to drop.
    if declaration.space != DeclaredSpace::OneD {
        return edits;
    }

    // The factored `Project` payload (v9), flattened exactly as the
    // section addresses it: `space.OneD.in_2d.Project.{shape,mirror,flip}`.
    let project = field(&field(&variant, "in_2d"), "Project");
    if let Some(shape) = declaration.shape {
        edits.push(SlotEdit::ensure_present(field(
            &field(&project, "shape"),
            shape.variant_ident(),
        )));
    }
    // The modifiers are two-variant MODE enums, not bools, so they take
    // the same `EnsurePresent <row>.<Variant>` gesture the section's
    // two-card rows send — never a bool `SetValue`.
    for (name, on, on_ident) in [
        ("mirror", declaration.mirror, "Mirrored"),
        ("flip", declaration.flip, "Flipped"),
    ] {
        if let Some(on) = on {
            let ident = if on { on_ident } else { "Normal" };
            edits.push(SlotEdit::ensure_present(field(
                &field(&project, name),
                ident,
            )));
        }
    }
    edits
}

/// One declared uniform of the target shader.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AgentShaderBinding {
    /// Uniform path (e.g. `time`).
    pub name: String,
    /// GLSL type name as the generated uniform header declares it.
    pub ty: String,
    /// Authored default value display, when one exists (values are
    /// bus-driven at runtime).
    pub value: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(path: &str) -> SlotPath {
        SlotPath::parse(path).expect("test path")
    }

    #[test]
    fn full_upsert_produces_the_exact_slot_edit_list() {
        let upsert = ParamUpsert {
            name: "speed".into(),
            label: Some("Speed".into()),
            default: Some(1.0),
            min: Some(0.0),
            max: Some(4.0),
            step: Some(1.0),
            unit: Some("x".into()),
            ..ParamUpsert::default()
        };
        assert_eq!(
            param_upsert_edits(&upsert),
            vec![
                SlotEdit::ensure_present(parse(r#"consumed["speed"]"#)),
                SlotEdit::assign_value(
                    parse(r#"consumed["speed"].label"#),
                    LpValue::String("Speed".into()),
                ),
                SlotEdit::ensure_present(parse(r#"consumed["speed"].default.some"#)),
                SlotEdit::assign_value(
                    parse(r#"consumed["speed"].default.some"#),
                    LpValue::F32(1.0),
                ),
                SlotEdit::ensure_present(parse(r#"consumed["speed"].min.some"#)),
                SlotEdit::assign_value(parse(r#"consumed["speed"].min.some"#), LpValue::F32(0.0)),
                SlotEdit::ensure_present(parse(r#"consumed["speed"].max.some"#)),
                SlotEdit::assign_value(parse(r#"consumed["speed"].max.some"#), LpValue::F32(4.0)),
                SlotEdit::ensure_present(parse(r#"consumed["speed"].step.some"#)),
                SlotEdit::assign_value(parse(r#"consumed["speed"].step.some"#), LpValue::F32(1.0)),
                SlotEdit::ensure_present(parse(r#"consumed["speed"].unit.some"#)),
                SlotEdit::assign_value(
                    parse(r#"consumed["speed"].unit.some"#),
                    LpValue::String("x".into()),
                ),
            ]
        );
    }

    #[test]
    fn sparse_upserts_only_touch_present_fields() {
        let upsert = ParamUpsert {
            name: "speed".into(),
            default: Some(2.0),
            ..ParamUpsert::default()
        };
        assert_eq!(
            param_upsert_edits(&upsert),
            vec![
                SlotEdit::ensure_present(parse(r#"consumed["speed"]"#)),
                SlotEdit::ensure_present(parse(r#"consumed["speed"].default.some"#)),
                SlotEdit::assign_value(
                    parse(r#"consumed["speed"].default.some"#),
                    LpValue::F32(2.0),
                ),
            ]
        );
    }

    #[test]
    fn kind_upsert_assigns_the_kind_field_directly() {
        // `kind` is a required ValueSlot (like `label`), not an Option — no
        // `ensure_present` needed.
        let upsert = ParamUpsert {
            name: "phase".into(),
            kind: Some("seconds".into()),
            ..ParamUpsert::default()
        };
        assert_eq!(
            param_upsert_edits(&upsert),
            vec![
                SlotEdit::ensure_present(parse(r#"consumed["phase"]"#)),
                SlotEdit::assign_value(
                    parse(r#"consumed["phase"].kind"#),
                    LpValue::String("seconds".into()),
                ),
            ]
        );
    }

    #[test]
    fn phasor_kind_writes_the_whole_config_in_one_leaf_assign() {
        let upsert = ParamUpsert {
            name: "phase".into(),
            kind: Some("phasor".into()),
            period_seconds: Some(2.5),
            waveform: Some("sine".into()),
            phase_offset: Some(0.25),
            ..ParamUpsert::default()
        };
        assert_eq!(
            param_upsert_edits(&upsert),
            vec![
                SlotEdit::ensure_present(parse(r#"consumed["phase"]"#)),
                SlotEdit::assign_value(
                    parse(r#"consumed["phase"].kind"#),
                    LpValue::String("phasor".into()),
                ),
                SlotEdit::ensure_present(parse(r#"consumed["phase"].phasor.some"#)),
                SlotEdit::assign_value(
                    parse(r#"consumed["phase"].phasor.some"#),
                    PhasorConfig {
                        period_seconds: 2.5,
                        waveform: Waveform::Sine,
                        phase_offset: 0.25,
                    }
                    .to_lp_value(),
                ),
            ]
        );
    }

    /// A full 1D declaration writes the space and every present
    /// projection field, in the order a user clicking the section sends
    /// them. Modifiers are enum ensures, never bool assigns.
    #[test]
    fn a_full_one_d_declaration_produces_the_exact_slot_edit_list() {
        let declaration = SpaceDeclaration {
            space: DeclaredSpace::OneD,
            shape: Some(lpa_agent::ProjectionShapeTag::Radial),
            mirror: Some(true),
            flip: Some(false),
        };
        assert_eq!(
            space_declaration_edits(&declaration),
            vec![
                SlotEdit::ensure_present(parse("space.OneD")),
                SlotEdit::ensure_present(parse("space.OneD.in_2d.Project.shape.Radial")),
                SlotEdit::ensure_present(parse("space.OneD.in_2d.Project.mirror.Mirrored")),
                SlotEdit::ensure_present(parse("space.OneD.in_2d.Project.flip.Normal")),
            ]
        );
    }

    /// Omitted projection fields are left alone — the `upsert_param`
    /// posture ("only the fields you pass are written"). Re-declaring 1D
    /// therefore keeps whatever projection the user already authored.
    #[test]
    fn a_bare_one_d_declaration_only_writes_the_space() {
        let declaration = SpaceDeclaration {
            space: DeclaredSpace::OneD,
            ..SpaceDeclaration::default()
        };
        assert_eq!(
            space_declaration_edits(&declaration),
            vec![SlotEdit::ensure_present(parse("space.OneD"))]
        );
    }

    /// A 2D declaration is the bare variant ensure: `SpaceAnswer1` has one
    /// variant, so there is nothing to author on that side.
    #[test]
    fn a_two_d_declaration_is_the_bare_variant_ensure() {
        assert_eq!(
            space_declaration_edits(&SpaceDeclaration {
                space: DeclaredSpace::TwoD,
                ..SpaceDeclaration::default()
            }),
            vec![SlotEdit::ensure_present(parse("space.TwoD"))]
        );
    }

    /// Each shape tag maps to the model's variant ident verbatim — the
    /// idents are slot path SEGMENTS, so a typo would write a path that
    /// silently rejects.
    #[test]
    fn every_shape_tag_writes_its_model_variant_ident() {
        for (tag, ident) in [
            (lpa_agent::ProjectionShapeTag::ExtrudeX, "ExtrudeX"),
            (lpa_agent::ProjectionShapeTag::ExtrudeY, "ExtrudeY"),
            (lpa_agent::ProjectionShapeTag::Radial, "Radial"),
            (lpa_agent::ProjectionShapeTag::Angular, "Angular"),
        ] {
            let edits = space_declaration_edits(&SpaceDeclaration {
                space: DeclaredSpace::OneD,
                shape: Some(tag),
                ..SpaceDeclaration::default()
            });
            assert_eq!(
                edits[1],
                SlotEdit::ensure_present(parse(&format!("space.OneD.in_2d.Project.shape.{ident}"))),
                "{tag:?} must address the model's `{ident}` variant"
            );
        }
    }

    #[test]
    fn phasor_kind_with_no_shaping_fields_writes_the_default_config() {
        let upsert = ParamUpsert {
            name: "phase".into(),
            kind: Some("phasor".into()),
            ..ParamUpsert::default()
        };
        let edits = param_upsert_edits(&upsert);
        assert_eq!(
            edits.last(),
            Some(&SlotEdit::assign_value(
                parse(r#"consumed["phase"].phasor.some"#),
                PhasorConfig::default().to_lp_value(),
            ))
        );
    }
}
