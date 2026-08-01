//! Project-side lookups the shader agent's run start needs.
//!
//! Small data carriers only — the resolution logic lives on
//! [`ProjectController`](super::project_controller::ProjectController)
//! (`agent_shader_target`, `agent_fixture_defs`, `agent_engine_status`)
//! because it reads private controller state (node tree, def-artifact map).

use lpa_agent::{EngineStatusKind, EngineVerdict, ParamUpsert};
use lpc_model::{LpValue, Revision, SlotEdit, SlotMapKey, SlotName, SlotPath};

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
    }
}

/// The `SlotEdit` list one `upsert_param` dispatches, in order: ensure the
/// `consumed[name]` entry exists (the server constructs the record default
/// — kind/value included, f32 in v1), then per present field the same ops
/// the human gestures send (`AssignValue` on the `label` value slot;
/// `EnsurePresent` + `AssignValue` on each option's `.some`). All edits
/// ride ONE `MutationCmdBatch` of `PutSlotEdit`s on the def artifact, the
/// same batch shape as `apply_asset_body`.
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
    if let Some(panel) = upsert.panel {
        option("panel", LpValue::Bool(panel));
    }
    if let Some(unit) = &upsert.unit {
        option("unit", LpValue::String(unit.clone()));
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
            panel: Some(true),
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
                SlotEdit::ensure_present(parse(r#"consumed["speed"].panel.some"#)),
                SlotEdit::assign_value(
                    parse(r#"consumed["speed"].panel.some"#),
                    LpValue::Bool(true),
                ),
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
}
