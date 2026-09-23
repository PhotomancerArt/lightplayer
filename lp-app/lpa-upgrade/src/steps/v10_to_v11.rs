//! Format 10 → 11: a gradient cycle can be pinned to one of its palettes.
//!
//! The break: `GradientConfig` storage grew a fifth field, `pinned` — an
//! `i32`, `-1` for "nothing pinned", otherwise the index of the one palette
//! a cycle shows instead of walking its set. The reader requires all five
//! fields, so a format-10 gradient config no longer parses.
//!
//! ## What this step does
//!
//! Adds `"pinned": -1` to every gradient config in the package, directly
//! after its `fade_seconds` so the scalars stay together ahead of the `set`
//! — nothing else moves, and every existing config reads exactly as it did
//! (unpinned). `project.json`'s `format` bumps `10` → `11` the same way
//! every step bumps its manifest.
//!
//! Gradient configs are found by **shape**, never by the key they sit under:
//! an object whose `kind` is `"static"` or `"cycle"` and which carries `set`,
//! `step_seconds` and `fade_seconds`. Today that is a shader slot's
//! `gradient`, but a palette value can also ride a bus default or a literal
//! binding, and the step must not care which.

use crate::json::JsonNode;
use crate::json_file_edit::edit_json_files;
use crate::project_files::{ProjectFiles, is_manifest_path};
use crate::upgrade_error::UpgradeError;
use crate::upgrade_report::UpgradeReport;

const FROM: u32 = 10;
const TO: u32 = 11;

pub(crate) fn apply(
    files: &mut ProjectFiles,
    report: &mut UpgradeReport,
) -> Result<(), UpgradeError> {
    edit_json_files(files, report, |path, document, report| {
        if is_manifest_path(path) {
            bump_manifest_format(path, document, report);
        }
        let added = add_unpinned(document);
        if added > 0 {
            report.note(format!(
                "{path}: {added} gradient config(s) gain `pinned: -1` (unpinned)"
            ));
        }
        Ok(())
    })
}

/// The manifest's own version stamp, `10` → `11`.
fn bump_manifest_format(path: &str, document: &mut JsonNode, report: &mut UpgradeReport) {
    if document.get("format").and_then(JsonNode::as_u32) == Some(FROM) {
        document.set("format", JsonNode::u32(TO));
        report.note(format!("{path}: format {FROM} → {TO}"));
    }
}

/// Walk `node`, giving every gradient config without a `pinned` field an
/// unpinned one. Returns how many it touched. A config that already has
/// `pinned` passes through — the chain must be re-runnable over
/// partially-migrated trees.
fn add_unpinned(node: &mut JsonNode) -> usize {
    let mut added = 0;
    if is_gradient_config(node) && node.get("pinned").is_none() {
        let members = node.object_mut().expect("a gradient config is an object");
        let at = members
            .iter()
            .position(|(name, _)| name == "fade_seconds")
            .map_or(members.len(), |index| index + 1);
        members.insert(at, ("pinned".to_owned(), JsonNode::Scalar("-1".to_owned())));
        added += 1;
    }
    match node {
        JsonNode::Object(members) => {
            for (_, child) in members {
                added += add_unpinned(child);
            }
        }
        JsonNode::Array(items) => {
            for child in items {
                added += add_unpinned(child);
            }
        }
        JsonNode::Scalar(_) => {}
    }
    added
}

/// The storage shape of a `GradientConfig`, by meaning rather than by where
/// it sits.
fn is_gradient_config(node: &JsonNode) -> bool {
    (node.has_string("kind", "static") || node.has_string("kind", "cycle"))
        && matches!(node.get("set"), Some(JsonNode::Array(_)))
        && node.get("step_seconds").is_some_and(JsonNode::is_number)
        && node.get("fade_seconds").is_some_and(JsonNode::is_number)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn upgrade(files: &mut ProjectFiles) -> UpgradeReport {
        let mut report = UpgradeReport::new(FROM);
        apply(files, &mut report).expect("upgrades");
        report
    }

    fn shader(pinned: bool) -> Vec<u8> {
        let pin = if pinned {
            "\n        \"pinned\": -1,"
        } else {
            ""
        };
        format!(
            "{{\n  \"kind\": \"Shader\",\n  \"consumed\": {{\n    \"palette\": {{\n      \
             \"kind\": \"palette\",\n      \"gradient\": {{\n        \"kind\": \"cycle\",\n        \
             \"step_seconds\": 6.0,\n        \"fade_seconds\": 1.32,{pin}\n        \"set\": [\n          \
             \"a\",\n          \"b\"\n        ]\n      }}\n    }}\n  }}\n}}"
        )
        .into_bytes()
    }

    /// The whole change: `pinned: -1` lands right after `fade_seconds`, and
    /// nothing else in the document moves.
    #[test]
    fn a_gradient_config_gains_an_unpinned_field_after_its_fade() {
        let mut files: ProjectFiles = [("shader.json", shader(false))].into_iter().collect();
        let report = upgrade(&mut files);
        let expected = String::from_utf8(shader(true)).unwrap() + "\n";
        assert_eq!(
            std::str::from_utf8(files.get("shader.json").unwrap()).unwrap(),
            expected
        );
        assert_eq!(report.changed_files, vec!["shader.json".to_string()]);
    }

    /// Shape-gated: an object with a `kind` of `"cycle"` but none of the
    /// gradient fields is something else, and is left alone.
    #[test]
    fn a_lookalike_kind_without_the_gradient_fields_is_left_alone() {
        let stranger = b"{\n  \"kind\": \"cycle\",\n  \"period\": 2.0,\n  \"set\": []\n}".to_vec();
        let mut files: ProjectFiles = [("other.json", stranger)].into_iter().collect();
        let report = upgrade(&mut files);
        assert!(
            report.changed_files.is_empty(),
            "{:?}",
            report.changed_files
        );
    }

    /// Re-running over an already-migrated tree changes nothing.
    #[test]
    fn an_already_pinned_config_is_left_alone() {
        let mut files: ProjectFiles = [("shader.json", shader(true))].into_iter().collect();
        let report = upgrade(&mut files);
        assert!(
            report.changed_files.is_empty(),
            "{:?}",
            report.changed_files
        );
    }

    #[test]
    fn the_manifest_format_is_bumped() {
        let mut files: ProjectFiles = [(
            "project.json",
            b"{\n  \"format\": 10,\n  \"name\": \"x\"\n}".to_vec(),
        )]
        .into_iter()
        .collect();
        upgrade(&mut files);
        assert_eq!(
            files.get("project.json"),
            Some(b"{\n  \"format\": 11,\n  \"name\": \"x\"\n}\n".as_slice())
        );
    }
}
