//! Format 5 → 6: `float_mode` is an optional pin, not a required choice.
//!
//! `ShaderDef.float_mode` and `ComputeShaderDef.float_mode` went from
//! `ValueSlot<FloatMode>` (always present, defaulting to `Fixed`) to
//! `OptionSlot<ValueSlot<FloatMode>>`, where **absence means Auto**: the
//! target's native representation. On every backend shipping today that is
//! Q32 on a CPU and `F32Gpu` on the GPU tier — exactly what a `"fixed"`
//! shader already got.
//!
//! So a format-5 `"float_mode": "fixed"` is the pre-posture default spelled
//! out: a pin that pins nothing. This step deletes it. `"float"` is a real
//! pin — the author asked for numerics the target would not have chosen —
//! and passes through untouched.
//!
//! ## Behavior preservation
//!
//! Dropping `"fixed"` is behavior-preserving *because* Auto resolves to the
//! native representation, and every current backend is Q32-native. The
//! upgrade is a spelling change, not a numerics change: the same shader
//! compiles to the same code on the same board before and after.
//!
//! Note which direction that argument runs. It licenses dropping `"fixed"`,
//! and nothing else — it does not license adding `"fixed"` to shaders that
//! omit the key, nor rewriting `"float"` into anything.
//!
//! ## Keying off meaning
//!
//! The signal is the *value*, on a node whose *kind* has this field —
//! never the name alone. A `float_mode` value the format never had is a
//! shape this step refuses rather than guesses at.

use crate::json::JsonNode;
use crate::json_file_edit::edit_json_files;
use crate::project_files::{ProjectFiles, is_manifest_path};
use crate::upgrade_error::UpgradeError;
use crate::upgrade_report::UpgradeReport;

const FROM: u32 = 5;
const TO: u32 = 6;

/// The authored key this step migrates.
const FLOAT_MODE: &str = "float_mode";

/// The value that was the format-5 default spelled out, and so pins nothing.
const REDUNDANT_PIN: &str = "fixed";

/// The value that is a real pin and must survive.
const REAL_PIN: &str = "float";

/// The two node kinds that carry a `float_mode` slot at format 5.
const SHADER_KINDS: &[&str] = &["Shader", "ComputeShader"];

pub(crate) fn apply(
    files: &mut ProjectFiles,
    report: &mut UpgradeReport,
) -> Result<(), UpgradeError> {
    edit_json_files(files, report, |path, document, report| {
        if is_manifest_path(path) {
            bump_manifest_format(path, document, report);
        }
        drop_redundant_pins(path, document, report)
    })
}

/// R1: the manifest's own version stamp.
fn bump_manifest_format(path: &str, document: &mut JsonNode, report: &mut UpgradeReport) {
    if document.get("format").and_then(JsonNode::as_u32) == Some(FROM) {
        document.set("format", JsonNode::u32(TO));
        report.note(format!("{path}: format {FROM} → {TO}"));
    }
}

/// R2: walk every object, and on each one carrying a shader `kind`, drop a
/// `float_mode` that says `fixed`.
///
/// Walking rather than only inspecting the file root is what covers a node
/// inlined inside another artifact (a playlist entry's `node`).
fn drop_redundant_pins(
    path: &str,
    node: &mut JsonNode,
    report: &mut UpgradeReport,
) -> Result<(), UpgradeError> {
    match node {
        JsonNode::Object(_) => {
            if node_kind(node).is_some_and(|kind| SHADER_KINDS.contains(&kind.as_str())) {
                drop_redundant_pin(path, node, report)?;
            }
            let members = node.object_mut().expect("object");
            for (_, child) in members.iter_mut() {
                drop_redundant_pins(path, child, report)?;
            }
            Ok(())
        }
        JsonNode::Array(items) => {
            for item in items.iter_mut() {
                drop_redundant_pins(path, item, report)?;
            }
            Ok(())
        }
        JsonNode::Scalar(_) => Ok(()),
    }
}

fn drop_redundant_pin(
    path: &str,
    artifact: &mut JsonNode,
    report: &mut UpgradeReport,
) -> Result<(), UpgradeError> {
    let Some(pin) = artifact.get(FLOAT_MODE) else {
        return Ok(());
    };
    // A non-string here never loaded at format 5, and neither the old nor the
    // new model can say what it meant.
    let Some(pin) = pin.as_str() else {
        return Err(refuse(path, "a value that is not a string"));
    };
    match pin.as_str() {
        REDUNDANT_PIN => {
            artifact.remove(FLOAT_MODE);
            report.note(format!(
                "{path}: dropped `\"{FLOAT_MODE}\": \"{REDUNDANT_PIN}\"` (the format-5 default; \
                 an unpinned shader runs the target's native representation, which is the same \
                 thing on every current target)"
            ));
            Ok(())
        }
        // A real pin, left verbatim.
        REAL_PIN => Ok(()),
        other => Err(refuse(path, &format!("`{other}`"))),
    }
}

fn refuse(path: &str, found: &str) -> UpgradeError {
    UpgradeError::Refused {
        file: String::from(path),
        reason: format!(
            "`{FLOAT_MODE}` is {found}, but format 5 only ever had `{REDUNDANT_PIN}` or \
             `{REAL_PIN}` — fix it by hand and re-open the project"
        ),
    }
}

fn node_kind(node: &JsonNode) -> Option<String> {
    node.get("kind").and_then(JsonNode::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fixed_pin_is_dropped() {
        let migrated = migrate_one(
            "shader.json",
            r#"{
  "kind": "Shader",
  "source": "shader.glsl",
  "float_mode": "fixed",
  "consumed": {
    "speed": {
      "kind": "value",
      "value": "f32"
    }
  }
}"#,
        )
        .expect("migrates");
        assert_eq!(
            migrated,
            r#"{
  "kind": "Shader",
  "source": "shader.glsl",
  "consumed": {
    "speed": {
      "kind": "value",
      "value": "f32"
    }
  }
}
"#
        );
    }

    #[test]
    fn a_float_pin_is_kept_verbatim() {
        // The whole reason this step reads the value instead of the key: a
        // `float` shader asked for numerics the target would not have picked,
        // and dropping it would silently move it back to Q32.
        assert_eq!(
            migrate_one(
                "shader.json",
                r#"{
  "kind": "Shader",
  "float_mode": "float"
}"#
            ),
            None
        );
    }

    #[test]
    fn a_shader_with_no_pin_is_already_at_the_new_shape() {
        assert_eq!(
            migrate_one("shader.json", r#"{"kind":"Shader","source":"s.glsl"}"#),
            None
        );
    }

    #[test]
    fn a_compute_shader_pin_is_dropped_too() {
        let migrated = migrate_one(
            "compute.json",
            r#"{"kind":"ComputeShader","float_mode":"fixed","produced":{}}"#,
        )
        .expect("migrates");
        assert_eq!(
            migrated,
            "{\n  \"kind\": \"ComputeShader\",\n  \"produced\": {}\n}\n"
        );
    }

    #[test]
    fn a_non_shader_node_keeps_its_own_float_mode_key() {
        // Nothing else authors this key today, but the rule is "this field on
        // this kind", not "this name anywhere" — the same discipline v4→v5
        // needed for its `time` slots.
        assert_eq!(
            migrate_one("fixture.json", r#"{"kind":"Fixture","float_mode":"fixed"}"#),
            None
        );
    }

    #[test]
    fn an_inlined_shader_node_is_migrated_in_place() {
        let migrated = migrate_one(
            "playlist.json",
            r#"{"kind":"Playlist","entries":{"1":{"node":{"kind":"Shader","float_mode":"fixed"}}}}"#,
        )
        .expect("migrates");
        assert!(!migrated.contains("float_mode"), "{migrated}");
    }

    #[test]
    fn an_unknown_pin_is_refused_rather_than_guessed_at() {
        let refused = apply_to("shader.json", r#"{"kind":"Shader","float_mode":"q32"}"#)
            .expect_err("refuses");
        let UpgradeError::Refused { file, reason } = refused else {
            panic!("expected a refusal, got {refused:?}");
        };
        assert_eq!(file, "shader.json");
        assert!(reason.contains("`q32`"), "{reason}");

        let refused =
            apply_to("shader.json", r#"{"kind":"Shader","float_mode":true}"#).expect_err("refuses");
        assert!(
            matches!(&refused, UpgradeError::Refused { reason, .. } if reason.contains("not a string")),
            "{refused:?}"
        );
    }

    #[test]
    fn only_the_manifest_format_is_bumped() {
        // `*.map2d.json` carries its own unrelated `format` key.
        assert_eq!(
            migrate_one("fyeah.map2d.json", r#"{"format":1,"objects":[]}"#),
            None
        );
        assert_eq!(
            migrate_one("project.json", "{\n  \"format\": 5,\n  \"name\": \"x\"\n}"),
            Some(String::from("{\n  \"format\": 6,\n  \"name\": \"x\"\n}\n"))
        );
    }

    /// Runs the step over a one-file package; `None` means byte-identical.
    fn migrate_one(path: &str, source: &str) -> Option<String> {
        let (files, report) = apply_to(path, source).expect("migrates");
        if report.changed_files.is_empty() {
            assert_eq!(files.get(path), Some(source.as_bytes()));
            return None;
        }
        assert_eq!(report.changed_files, vec![String::from(path)]);
        Some(String::from_utf8(files.get(path).unwrap().to_vec()).unwrap())
    }

    fn apply_to(path: &str, source: &str) -> Result<(ProjectFiles, UpgradeReport), UpgradeError> {
        let mut files: ProjectFiles = [(path, source.as_bytes().to_vec())].into_iter().collect();
        let mut report = UpgradeReport::new(FROM);
        apply(&mut files, &mut report)?;
        report.to = TO;
        Ok((files, report))
    }
}
