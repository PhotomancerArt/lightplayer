//! Format 9 → 10: the output wire map is spelled `ports` (Q31, the D45
//! no-universe/no-channel vocabulary ruling — "channel" reads as the
//! 512-limited DMX unit, which most LED control ports do not share, so the
//! authored surface says **port**).
//!
//! The break: `OutputDef.channels` (JSON key `"channels"`, entries =
//! `OutputChannelDef`) renamed to `ports` (`OutputPortDef`). A format-9
//! output no longer parses — a v10 build refuses the old key loudly rather
//! than producing an output with no wires.
//!
//! ## What this step does
//!
//! In every `kind: Output` document, renames the top-level `"channels"`
//! key to `"ports"` — value untouched, key order preserved (the rewrite
//! must stay byte-minimal: nothing but the key and the manifest stamp
//! changes). `project.json`'s `format` bumps `9` → `10` the same way every
//! step bumps its manifest.
//!
//! Cells are found by node kind, never by blind key search: `"channels"`
//! keys elsewhere (none exist in practice, but a future bus-channel
//! surface must not be caught by an output rename).

use crate::json::JsonNode;
use crate::json_file_edit::edit_json_files;
use crate::project_files::{ProjectFiles, is_manifest_path};
use crate::upgrade_error::UpgradeError;
use crate::upgrade_report::UpgradeReport;

const FROM: u32 = 9;
const TO: u32 = 10;

pub(crate) fn apply(
    files: &mut ProjectFiles,
    report: &mut UpgradeReport,
) -> Result<(), UpgradeError> {
    edit_json_files(files, report, |path, document, report| {
        if is_manifest_path(path) {
            bump_manifest_format(path, document, report);
        }
        if document.has_string("kind", "Output") {
            rename_channels_to_ports(path, document, report);
        }
        Ok(())
    })
}

/// The manifest's own version stamp, `9` → `10`.
fn bump_manifest_format(path: &str, document: &mut JsonNode, report: &mut UpgradeReport) {
    if document.get("format").and_then(JsonNode::as_u32) == Some(FROM) {
        document.set("format", JsonNode::u32(TO));
        report.note(format!("{path}: format {FROM} → {TO}"));
    }
}

/// `"channels"` → `"ports"` on one output document, in place. A document
/// already using `ports` (or carrying neither key) passes through — the
/// chain must be re-runnable over partially-migrated trees.
fn rename_channels_to_ports(path: &str, document: &mut JsonNode, report: &mut UpgradeReport) {
    if document.get("channels").is_none() || document.get("ports").is_some() {
        return;
    }
    document.rename_key("channels", "ports");
    report.record_changed(path);
    report.note(format!(
        "{path}: output `channels` → `ports` (the v10 port vocabulary)"
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn upgrade(files: &mut ProjectFiles) -> UpgradeReport {
        let mut report = UpgradeReport::new(FROM);
        apply(files, &mut report).expect("upgrades");
        report
    }

    fn output(key: &str) -> Vec<u8> {
        format!(
            "{{\n  \"kind\": \"Output\",\n  \"{key}\": {{\n    \"0\": {{\n      \"endpoint\": \
             \"ws281x:local:IO18\",\n      \"count\": 100\n    }}\n  }},\n  \"bindings\": {{\n    \
             \"input\": {{\n      \"source\": \"bus:control.out\"\n    }}\n  }}\n}}"
        )
        .into_bytes()
    }

    /// The whole rename: the key changes, nothing else moves — entries,
    /// sibling keys, and formatting all survive byte-for-byte.
    #[test]
    fn an_output_channels_map_becomes_ports_in_place() {
        let mut files: ProjectFiles = [("output.json", output("channels"))].into_iter().collect();
        upgrade(&mut files);
        let expected = String::from_utf8(output("ports")).unwrap() + "\n";
        assert_eq!(
            std::str::from_utf8(files.get("output.json").unwrap()).unwrap(),
            expected
        );
    }

    /// Kind-gated: a non-output document carrying a `channels` key is not
    /// an output wire map and must pass through untouched.
    #[test]
    fn non_output_documents_are_left_alone() {
        let stranger =
            b"{\n  \"kind\": \"Mixer\",\n  \"channels\": {\n    \"0\": {}\n  }\n}".to_vec();
        let mut files: ProjectFiles = [("mixer.json", stranger.clone())].into_iter().collect();
        let report = upgrade(&mut files);
        assert!(
            report.changed_files.is_empty(),
            "{:?}",
            report.changed_files
        );
    }

    /// Re-running over an already-renamed tree changes nothing.
    #[test]
    fn an_already_renamed_output_is_left_alone() {
        let mut files: ProjectFiles = [("output.json", output("ports"))].into_iter().collect();
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
            b"{\n  \"format\": 9,\n  \"name\": \"x\"\n}".to_vec(),
        )]
        .into_iter()
        .collect();
        upgrade(&mut files);
        assert_eq!(
            files.get("project.json"),
            Some(b"{\n  \"format\": 10,\n  \"name\": \"x\"\n}\n".as_slice())
        );
    }
}
