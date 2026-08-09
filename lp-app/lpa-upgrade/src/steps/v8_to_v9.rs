//! Format 8 → 9: the projection vocabulary FACTORED (post-G2 ruling,
//! dimensionality plan-B — "no one uses it yet, lets fix it right. its a
//! version bump, oh well").
//!
//! The break: `SpaceAnswer2` (`ShaderDef.space.OneD.in_2d`) and
//! `ConsumerCell2` (`FixtureDef.consume.Policy.from_1d`) collapsed their
//! per-shape variants into ONE flat record —
//! `Project { shape: ExtrudeX|ExtrudeY|Radial|Angular, mirror: bool,
//! flip: bool }` — and the `Default` variant was retired outright (the
//! producer always declares; a fresh record IS what `Default` resolved
//! to). A format-8 cell no longer parses.
//!
//! ## What this step does
//!
//! Rewrites every persisted answer cell to its factored spelling, in the
//! canonical writer's explicit form (all three payload fields present):
//!
//! | v8 cell     | v9 record                                    |
//! |-------------|----------------------------------------------|
//! | `Default`   | `Project { ExtrudeX }` (what it resolved to) |
//! | `Extrude`   | `Project { ExtrudeX }`                       |
//! | `Mirror`    | `Project { ExtrudeX, mirror, flip }`         |
//! | `Radial`    | `Project { Radial }`                         |
//! | `Angular`   | `Project { Angular }`                        |
//!
//! No other flags appear: no released format-8 build could author a
//! direction/fold payload (those were branch-local, never persisted
//! off-branch), so the bare kinds above are the whole v8 vocabulary.
//! Behavior-preserving by construction — each right-hand side is the
//! engine map the left-hand side already ran. The `Mirror` row carries
//! BOTH modifiers because v8's mirror was the OUTWARD fold
//! (`u′ = |2x−1|`, bright at the edges) and the factored `mirror`
//! modifier alone is the inward fold (`1 − |2x−1|`): the flip restores
//! the outward run. (The ruling's shorthand table wrote
//! `Mirror→Project{ExtrudeX, mirror}`; the same ruling's bit-identity
//! principle wins over the shorthand.)
//!
//! Cells are found by node kind, never by blind key search: a `Shader`
//! def's `space.in_2d` (when the space is `OneD`) and a `Fixture` def's
//! `consume.from_1d` (when the consume is `Policy`). `project.json`'s own
//! `format` field is bumped `8` → `9` the same way every step bumps its
//! manifest.

use crate::json::JsonNode;
use crate::json_file_edit::edit_json_files;
use crate::project_files::{ProjectFiles, is_manifest_path};
use crate::upgrade_error::UpgradeError;
use crate::upgrade_report::UpgradeReport;

const FROM: u32 = 8;
const TO: u32 = 9;

pub(crate) fn apply(
    files: &mut ProjectFiles,
    report: &mut UpgradeReport,
) -> Result<(), UpgradeError> {
    edit_json_files(files, report, |path, document, report| {
        if is_manifest_path(path) {
            bump_manifest_format(path, document, report);
        }
        if document.has_string("kind", "Shader") {
            factor_shader_space(path, document, report);
        }
        if document.has_string("kind", "Fixture") {
            factor_fixture_consume(path, document, report);
        }
        Ok(())
    })
}

/// The manifest's own version stamp, `8` → `9`.
fn bump_manifest_format(path: &str, document: &mut JsonNode, report: &mut UpgradeReport) {
    if document.get("format").and_then(JsonNode::as_u32) == Some(FROM) {
        document.set("format", JsonNode::u32(TO));
        report.note(format!("{path}: format {FROM} → {TO}"));
    }
}

/// A shader def's `space.OneD.in_2d` answer cell.
fn factor_shader_space(path: &str, document: &mut JsonNode, report: &mut UpgradeReport) {
    let Some(space) = document.get_mut("space") else {
        return;
    };
    if !space.has_string("kind", "OneD") {
        return;
    }
    let Some(cell) = space.get_mut("in_2d") else {
        return;
    };
    factor_cell(path, "space.OneD.in_2d", cell, report);
}

/// A fixture def's `consume.Policy.from_1d` cell.
fn factor_fixture_consume(path: &str, document: &mut JsonNode, report: &mut UpgradeReport) {
    let Some(consume) = document.get_mut("consume") else {
        return;
    };
    if !consume.has_string("kind", "Policy") {
        return;
    }
    let Some(cell) = consume.get_mut("from_1d") else {
        return;
    };
    factor_cell(path, "consume.Policy.from_1d", cell, report);
}

/// Rewrite one v8 answer cell in place. A cell already in the factored
/// form (or anything unrecognized) is left untouched — the chain must be
/// re-runnable over partially-migrated trees without inventing content.
fn factor_cell(path: &str, slot: &str, cell: &mut JsonNode, report: &mut UpgradeReport) {
    let Some(kind) = cell.get("kind").and_then(JsonNode::as_str) else {
        return;
    };
    let (shape, mirror, flip) = match kind.as_str() {
        // `Default` resolved to the system extrude.
        "Default" | "Extrude" => ("ExtrudeX", false, false),
        // v8's mirror was the OUTWARD fold `|2x−1|` — factored, that is
        // the inward `mirror` modifier flipped (see the module doc).
        "Mirror" => ("ExtrudeX", true, true),
        "Radial" => ("Radial", false, false),
        "Angular" => ("Angular", false, false),
        _ => return,
    };
    *cell = factored_project(shape, mirror, flip);
    report.record_changed(path);
    report.note(format!(
        "{path}: {slot} `{kind}` → `Project {{ shape: {shape}{}{} }}` (the v9 \
         shape × mirror × flip factorization)",
        if mirror { ", mirror" } else { "" },
        if flip { ", flip" } else { "" }
    ));
}

/// The canonical v9 spelling of a factored cell — explicit payload
/// fields, matching the canonical slot writer byte for byte. The
/// modifiers are two-variant ENUMS (`MirrorMode`/`FlipMode` — kept
/// extensible), so they serialize as tagged objects like every Slotted
/// enum.
fn factored_project(shape: &str, mirror: bool, flip: bool) -> JsonNode {
    let mode = |on: bool, on_kind: &str| {
        JsonNode::Object(vec![(
            "kind".to_string(),
            JsonNode::string(if on { on_kind } else { "Normal" }),
        )])
    };
    JsonNode::Object(vec![
        ("kind".to_string(), JsonNode::string("Project")),
        (
            "shape".to_string(),
            JsonNode::Object(vec![("kind".to_string(), JsonNode::string(shape))]),
        ),
        ("mirror".to_string(), mode(mirror, "Mirrored")),
        ("flip".to_string(), mode(flip, "Flipped")),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn upgrade(files: &mut ProjectFiles) -> UpgradeReport {
        let mut report = UpgradeReport::new(FROM);
        apply(files, &mut report).expect("upgrades");
        report
    }

    fn shader(cell: &str) -> Vec<u8> {
        format!(
            "{{\n  \"kind\": \"Shader\",\n  \"source\": \"main.glsl\",\n  \"space\": {{\n    \
             \"kind\": \"OneD\",\n    \"in_2d\": {{\n      \"kind\": \"{cell}\"\n    }}\n  \
             }}\n}}"
        )
        .into_bytes()
    }

    fn in_2d_of(files: &ProjectFiles, path: &str) -> JsonNode {
        let document =
            JsonNode::parse(std::str::from_utf8(files.get(path).unwrap()).unwrap()).unwrap();
        document.get("space").unwrap().get("in_2d").unwrap().clone()
    }

    /// The whole v8 vocabulary, mapped: Default/Extrude → plain
    /// extrude-x, Mirror → extrude-x + mirror, Radial/Angular → the same
    /// shape — each right-hand side the engine map the v8 cell already
    /// ran.
    #[test]
    fn every_v8_cell_maps_to_its_factored_equivalent() {
        for (old, shape, mirror, flip) in [
            ("Default", "ExtrudeX", false, false),
            ("Extrude", "ExtrudeX", false, false),
            // v8's mirror = the outward fold = mirror + flip (the module
            // doc's bit-identity note).
            ("Mirror", "ExtrudeX", true, true),
            ("Radial", "Radial", false, false),
            ("Angular", "Angular", false, false),
        ] {
            let mut files: ProjectFiles = [("shader.json", shader(old))].into_iter().collect();
            upgrade(&mut files);
            assert_eq!(
                in_2d_of(&files, "shader.json"),
                factored_project(shape, mirror, flip),
                "{old}"
            );
        }
    }

    /// A fixture's `consume.Policy.from_1d` gets the same treatment, and
    /// the `force` bit rides along untouched.
    #[test]
    fn a_fixture_policy_cell_is_factored_and_force_survives() {
        let mut files: ProjectFiles = [(
            "fixture.json",
            b"{\n  \"kind\": \"Fixture\",\n  \"consume\": {\n    \"kind\": \"Policy\",\n    \
              \"from_1d\": {\n      \"kind\": \"Mirror\"\n    },\n    \"force\": true\n  \
              }\n}"
                .to_vec(),
        )]
        .into_iter()
        .collect();
        upgrade(&mut files);
        let text = std::str::from_utf8(files.get("fixture.json").unwrap()).unwrap();
        let document = JsonNode::parse(text).unwrap();
        let consume = document.get("consume").unwrap();
        assert_eq!(
            *consume.get("from_1d").unwrap(),
            factored_project("ExtrudeX", true, true)
        );
        assert!(consume.has_string("kind", "Policy"));
        assert_eq!(consume.get("force").unwrap().as_str(), None);
        assert!(text.contains("\"force\": true"), "{text}");
    }

    /// An `Auto` fixture has no payload rows — nothing to rewrite; a 2D
    /// shader has no `in_2d` cell. Both pass through byte-identical
    /// (minus the manifest bump).
    #[test]
    fn cells_that_do_not_exist_are_not_invented() {
        let mut files: ProjectFiles = [
            (
                "fixture.json",
                b"{\n  \"kind\": \"Fixture\",\n  \"consume\": \"Auto\"\n}".to_vec(),
            ),
            (
                "shader.json",
                b"{\n  \"kind\": \"Shader\",\n  \"source\": \"main.glsl\"\n}".to_vec(),
            ),
        ]
        .into_iter()
        .collect();
        let report = upgrade(&mut files);
        assert!(
            report.changed_files.is_empty(),
            "{:?}",
            report.changed_files
        );
    }

    /// Re-running over an already-factored tree changes nothing — the
    /// chain must be safely re-runnable.
    #[test]
    fn an_already_factored_cell_is_left_alone() {
        let mut files: ProjectFiles = [(
            "shader.json",
            b"{\n  \"kind\": \"Shader\",\n  \"space\": {\n    \"kind\": \"OneD\",\n    \
              \"in_2d\": {\n      \"kind\": \"Project\",\n      \"shape\": {\n        \
              \"kind\": \"Radial\"\n      },\n      \"mirror\": {\n        \"kind\": \
              \"Normal\"\n      },\n      \"flip\": {\n        \"kind\": \"Normal\"\n      \
              }\n    }\n  }\n}"
                .to_vec(),
        )]
        .into_iter()
        .collect();
        let report = upgrade(&mut files);
        assert!(
            report.changed_files.is_empty(),
            "{:?}",
            report.changed_files
        );
    }

    #[test]
    fn only_the_manifest_format_is_bumped() {
        let mut files: ProjectFiles = [(
            "project.json",
            b"{\n  \"format\": 8,\n  \"name\": \"x\"\n}".to_vec(),
        )]
        .into_iter()
        .collect();
        upgrade(&mut files);
        assert_eq!(
            files.get("project.json"),
            Some(b"{\n  \"format\": 9,\n  \"name\": \"x\"\n}\n".as_slice())
        );
    }
}
