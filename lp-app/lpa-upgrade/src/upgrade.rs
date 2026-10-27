//! Running the step chain from a project's authored format to the current
//! one.

use crate::format_class::{FormatClass, UPGRADE_FLOOR, classify};
use crate::project_files::ProjectFiles;
use crate::steps::{STEPS, UpgradeStep};
use crate::upgrade_error::UpgradeError;
use crate::upgrade_report::UpgradeReport;
use lpc_model::PROJECT_FORMAT_VERSION;

/// The migration chain, oldest step first.
///
/// The last step's `to` is asserted equal to [`PROJECT_FORMAT_VERSION`] by a
/// test, which is what makes a future format bump fail CI until somebody
/// writes its step.
pub fn upgrade_steps() -> &'static [UpgradeStep] {
    STEPS
}

/// Migrate `files` in place to [`PROJECT_FORMAT_VERSION`].
///
/// Only files a step actually changed are rewritten; everything else — GLSL,
/// SVG, mappings, artifacts with nothing to migrate — comes back byte-for-byte
/// identical.
///
/// Errors, and does not touch `files`' contents, unless the project is
/// [`FormatClass::Upgradable`]. That includes a project already at the
/// current format: classify first, then upgrade only what needs it, so a
/// caller can tell "nothing to do" from "done".
///
/// All-or-nothing: a refusal on the eleventh file leaves the first ten as
/// they were. A caller that writes a half-migrated package back to disk has
/// destroyed the only copy that still loaded *somewhere*.
pub fn upgrade_to_current(files: &mut ProjectFiles) -> Result<UpgradeReport, UpgradeError> {
    upgrade_to(files, PROJECT_FORMAT_VERSION)
}

/// Migrate `files` in place to format `target`, which may be older than
/// [`PROJECT_FORMAT_VERSION`]: the chain runs from the project's format and
/// stops after the step that writes `target`.
///
/// The same contract as [`upgrade_to_current`] (all-or-nothing, only
/// changed files rewritten); a project already at or past `target`, or a
/// `target` the chain does not reach, is refused with the classifier's
/// verdict and nothing touched.
pub fn upgrade_to(files: &mut ProjectFiles, target: u32) -> Result<UpgradeReport, UpgradeError> {
    let class = classify(files);
    let FormatClass::Upgradable { found } = class else {
        return Err(UpgradeError::NotUpgradable(class));
    };

    // A chain that does not actually reach the target would otherwise
    // report a cheerful success over an unchanged project. That is exactly
    // what a format bump landed without its step looks like, so it fails
    // here as well as in the chain-tip test.
    let chain: Vec<&UpgradeStep> = STEPS
        .iter()
        .filter(|step| step.from >= found && step.to <= target)
        .collect();
    let reaches_target = chain.first().is_some_and(|step| step.from == found)
        && chain.last().is_some_and(|step| step.to == target);
    if !reaches_target {
        return Err(UpgradeError::NotUpgradable(class));
    }

    let mut working = files.clone();
    let mut report = UpgradeReport::new(found);
    for step in chain {
        (step.apply)(&mut working, &mut report)?;
        report.to = step.to;
    }
    *files = working;
    Ok(report)
}

/// The format the chain ends at. Equal to [`PROJECT_FORMAT_VERSION`] by
/// construction — see [`upgrade_steps`].
pub fn chain_tip() -> u32 {
    STEPS.last().map_or(UPGRADE_FLOOR, |step| step.to)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn the_chain_ends_at_the_current_format() {
        // The whole point of this crate in one assertion: bump
        // PROJECT_FORMAT_VERSION without adding a step and CI goes red.
        assert_eq!(chain_tip(), PROJECT_FORMAT_VERSION);
    }

    #[test]
    fn the_current_format_has_a_history_snapshot() {
        // Companion to the chain-tip test above, closing the other half of
        // the ADR README's deferred follow-up ("CI check that a
        // PROJECT_FORMAT_VERSION bump lands with a schemas/history/v<N-1>/
        // snapshot" — open since bump 2, `docs/adr/2026-07-05-artifact-
        // format-version-and-schema-snapshots.md`). A step without its
        // `just format-bump` snapshot means the goldens in this crate were
        // authored against nothing: the fixtures a future vN→vN+1 step
        // needs come FROM that directory.
        let outgoing = PROJECT_FORMAT_VERSION - 1;
        let history_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../schemas/history")
            .join(format!("v{outgoing}"));
        assert!(
            history_dir.is_dir(),
            "schemas/history/v{outgoing}/ is missing for PROJECT_FORMAT_VERSION \
             {PROJECT_FORMAT_VERSION} — run `just format-bump` before bumping the \
             constant (expected directory: {})",
            history_dir.display()
        );
    }

    #[test]
    fn the_chain_starts_at_the_floor_and_has_no_gaps() {
        assert_eq!(
            STEPS.first().expect("at least one step").from,
            UPGRADE_FLOOR
        );
        for pair in STEPS.windows(2) {
            assert_eq!(pair[0].to, pair[1].from);
        }
        for step in STEPS {
            assert_eq!(step.to, step.from + 1, "{step:?} must be a single bump");
        }
    }

    #[test]
    fn an_already_current_project_is_not_upgraded() {
        let mut files: ProjectFiles = [(
            "project.json",
            format!("{{\"format\": {PROJECT_FORMAT_VERSION}}}").into_bytes(),
        )]
        .into_iter()
        .collect();
        assert_eq!(
            upgrade_to_current(&mut files),
            Err(UpgradeError::NotUpgradable(FormatClass::Current))
        );
    }

    /// `upgrade_to` stops after the step that writes its target, and
    /// refuses a target the project is already at.
    #[test]
    fn upgrade_to_stops_at_its_target() {
        let before: ProjectFiles = [
            ("project.json", br#"{"format": 9}"#.to_vec()),
            ("module.json", br#"{"kind":"Module","nodes":{}}"#.to_vec()),
        ]
        .into_iter()
        .collect();
        let mut files = before.clone();
        let report = upgrade_to(&mut files, 10).expect("9 -> 10");
        assert_eq!((report.from, report.to), (9, 10));
        let manifest = String::from_utf8(files.manifest().expect("manifest").to_vec()).unwrap();
        assert!(manifest.contains("\"format\": 10"), "{manifest}");

        let at_ten = files.clone();
        assert!(upgrade_to(&mut files, 10).is_err(), "already at 10");
        assert_eq!(files, at_ten);
        assert!(
            upgrade_to(&mut files, PROJECT_FORMAT_VERSION + 1).is_err(),
            "past the chain's tip"
        );
        assert_eq!(files, at_ten);
    }

    #[test]
    fn a_refusal_part_way_through_rolls_the_whole_project_back() {
        // `a.json` migrates cleanly; `z.json` refuses. Sorted order puts the
        // clean one first, so a non-atomic run would leave it rewritten.
        let before: ProjectFiles = [
            ("project.json", br#"{"format": 4}"#.to_vec()),
            (
                "a.json",
                br#"{"kind":"Shader","consumed":{"t":{"kind":"value","default_bind":"bus:time"}}}"#
                    .to_vec(),
            ),
            (
                "z.json",
                br#"{"kind":"Texture","bindings":{"t":{"source":"bus:time"}}}"#.to_vec(),
            ),
        ]
        .into_iter()
        .collect();

        let mut files = before.clone();
        let error = upgrade_to_current(&mut files).expect_err("must refuse");
        assert!(matches!(error, UpgradeError::Refused { .. }), "{error}");
        assert_eq!(files, before);
    }

    #[test]
    fn refusals_leave_the_files_alone() {
        for manifest in [
            String::from(r#"{"format": 1}"#),
            String::from(r#"{"kind": "Project"}"#),
            String::from(r#"{"format": 999}"#),
            String::from("not json"),
        ] {
            let before: ProjectFiles = [("project.json", manifest.clone().into_bytes())]
                .into_iter()
                .collect();
            let mut files = before.clone();
            let error = upgrade_to_current(&mut files).expect_err("must refuse");
            assert!(matches!(error, UpgradeError::NotUpgradable(_)), "{error}");
            assert_eq!(files, before);
        }
    }
}
