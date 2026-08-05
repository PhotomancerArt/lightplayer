//! The read → edit → write-if-changed loop every step runs.

use crate::json::JsonNode;
use crate::project_files::ProjectFiles;
use crate::upgrade_error::UpgradeError;
use crate::upgrade_report::UpgradeReport;

/// Apply `edit` to every `*.json` file in the package, rewriting **only**
/// the ones it actually changed.
///
/// Rewriting untouched files would churn: the authored corpus is not
/// canonically formatted (`projects/test/zook-dome-1500/project.json` is
/// 1-space indented, phasor records are sometimes inline, optional keys come
/// and go), and a migration diff a human cannot read is a migration a human
/// cannot review.
pub(crate) fn edit_json_files<F>(
    files: &mut ProjectFiles,
    report: &mut UpgradeReport,
    mut edit: F,
) -> Result<(), UpgradeError>
where
    F: FnMut(&str, &mut JsonNode, &mut UpgradeReport) -> Result<(), UpgradeError>,
{
    let paths: Vec<String> = files
        .paths()
        .filter(|path| path.ends_with(".json"))
        .map(String::from)
        .collect();

    for path in paths {
        let bytes = files.get(&path).unwrap_or_default();
        let text = std::str::from_utf8(bytes).map_err(|e| UpgradeError::Malformed {
            file: path.clone(),
            detail: e.to_string(),
        })?;
        let original = JsonNode::parse(text).map_err(|e| UpgradeError::Malformed {
            file: path.clone(),
            detail: e.detail,
        })?;

        let mut edited = original.clone();
        edit(&path, &mut edited, report)?;
        if edited != original {
            files.replace(&path, edited.to_pretty_bytes());
            report.record_changed(&path);
        }
    }
    Ok(())
}
