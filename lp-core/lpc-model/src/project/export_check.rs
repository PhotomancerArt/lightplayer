//! Export lint, static half: is each exported module folder *self-contained*?
//!
//! A library project (`kind = "pattern"` / `"rig"`, `docs/design/modules.md`)
//! names module folders in its manifest's `exports` list; another project
//! imports one by vendoring that folder wholesale. Whether that vendored copy
//! still works depends entirely on whether the folder is closed over its own
//! references — a `../common/simplex.glsl` next door does not travel with it.
//!
//! This module answers that statically, from bytes alone: **no I/O and no
//! engine**. The caller supplies the `(path, bytes)` pairs of the export
//! folder subtrees ([`ExportFileSet`]) plus the manifest's export list, and
//! gets back plain [`ExportFinding`]s. That keeps the same check runnable from
//! Studio (over a library snapshot), from `lp-cli`, and from pack CI (T3)
//! without any of them agreeing on a filesystem.
//!
//! The three static findings (module authoring vision D5/D6/D8):
//!
//! | Finding | Severity |
//! |---|---|
//! | a file ref resolves outside the export folder | error |
//! | the export folder is missing, or has no `module.json` at its root | error |
//! | the exported module carries no provenance (license especially) | warning |
//!
//! Refs are file-relative and location-independent by construction
//! (modules.md §6), so "escapes the folder" is decidable by resolving each
//! ref against its containing file and comparing prefixes — exactly what
//! [`crate::resolve_artifact_specifier`] already does for the loader.
//!
//! The verdict vocabulary ([`ExportSeverity`], [`ExportFinding`],
//! [`ExportLintReport`]) lives here rather than in Studio because the *graph*
//! half of the lint (`lpa-studio-core`'s `export_lint`) feeds the same report;
//! studio-core depends on this crate, not the other way round.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::{LpPath, LpPathBuf, NodeDef, resolve_artifact_specifier};

/// How badly an [`ExportFinding`] wants attention.
///
/// Ordered so `max()` picks the worse one ([`ExportLintReport::worst`]).
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ExportSeverity {
    /// The export would still ship; it is just poorer for it.
    Warning,
    /// The export is broken as a unit — an importer would get a copy that
    /// cannot load or cannot run.
    Error,
}

impl ExportSeverity {
    pub fn is_error(self) -> bool {
        matches!(self, Self::Error)
    }
}

/// One thing wrong (or merely thin) about one exported module.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportFinding {
    /// The export folder name this is about (the manifest `exports` entry).
    pub export: String,
    pub severity: ExportSeverity,
    /// One sentence naming what was found, then the remedy — the tone
    /// `PackageHealth`'s headline+remedy pair set for package problems.
    pub message: String,
    /// Project-relative path of the file the finding sits on, when one file
    /// is to blame.
    pub path: Option<String>,
}

impl ExportFinding {
    pub fn error(export: &str, message: String, path: Option<String>) -> Self {
        Self {
            export: export.to_string(),
            severity: ExportSeverity::Error,
            message,
            path,
        }
    }

    pub fn warning(export: &str, message: String, path: Option<String>) -> Self {
        Self {
            export: export.to_string(),
            severity: ExportSeverity::Warning,
            message,
            path,
        }
    }
}

/// The whole verdict on a project's exports: both halves' findings in one
/// list.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExportLintReport {
    pub findings: Vec<ExportFinding>,
}

impl ExportLintReport {
    pub fn new(findings: Vec<ExportFinding>) -> Self {
        Self { findings }
    }

    pub fn is_empty(&self) -> bool {
        self.findings.is_empty()
    }

    /// The worst severity present, or `None` for a clean report.
    pub fn worst(&self) -> Option<ExportSeverity> {
        self.findings.iter().map(|finding| finding.severity).max()
    }

    /// Findings about one export folder, in report order.
    pub fn for_export<'a>(&'a self, export: &'a str) -> impl Iterator<Item = &'a ExportFinding> {
        self.findings
            .iter()
            .filter(move |finding| finding.export == export)
    }

    /// Fold another half's findings in (the graph half joins this way).
    pub fn extend(&mut self, findings: impl IntoIterator<Item = ExportFinding>) {
        self.findings.extend(findings);
    }
}

/// The export folder subtree as the static check sees it: project-relative
/// paths to file bytes.
///
/// Paths are normalized to a leading `/` on insert, so a caller holding
/// `"chase/module.json"` (the library's `read_all_files` shape) and one
/// holding `"/chase/module.json"` (the artifact-location shape) agree.
/// Directories are not represented — a folder exists exactly when some file
/// is under it.
#[derive(Clone, Debug, Default)]
pub struct ExportFileSet<'a> {
    files: BTreeMap<LpPathBuf, &'a [u8]>,
}

impl<'a> ExportFileSet<'a> {
    pub fn new() -> Self {
        Self {
            files: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, path: &str, bytes: &'a [u8]) {
        self.files.insert(normalize(path), bytes);
    }

    pub fn get(&self, path: &LpPath) -> Option<&'a [u8]> {
        self.files.get(&path.to_path_buf()).copied()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Whether any file lives under `dir` (an absolute, slash-free-of-trailing
    /// directory path like `/chase`).
    pub fn any_under(&self, dir: &LpPath) -> bool {
        let prefix = format!("{}/", dir.as_str());
        self.files
            .keys()
            .any(|path| path.as_str().starts_with(&prefix))
    }
}

impl<'a, P: AsRef<str>> FromIterator<(P, &'a [u8])> for ExportFileSet<'a> {
    fn from_iter<I: IntoIterator<Item = (P, &'a [u8])>>(iter: I) -> Self {
        let mut set = Self::new();
        for (path, bytes) in iter {
            set.insert(path.as_ref(), bytes);
        }
        set
    }
}

fn normalize(path: &str) -> LpPathBuf {
    if path.starts_with('/') {
        LpPathBuf::from(path)
    } else {
        LpPathBuf::from(format!("/{path}"))
    }
}

/// Check every export folder named in `exports` against `files`.
///
/// `files` must carry the *complete* subtree of each export folder (missing
/// bytes read as a missing file, which is itself an error finding).
pub fn check_exports(exports: &[String], files: &ExportFileSet<'_>) -> ExportLintReport {
    let mut findings = Vec::new();
    for export in exports {
        findings.extend(check_export(export, files));
    }
    ExportLintReport::new(findings)
}

/// Check one export folder. See [`check_exports`].
pub fn check_export(export: &str, files: &ExportFileSet<'_>) -> Vec<ExportFinding> {
    let mut findings = Vec::new();
    let root = normalize(export);
    let module_path = root.join("module.json");

    let Some(module_bytes) = files.get(module_path.as_path()) else {
        let message = if files.any_under(root.as_path()) {
            format!(
                "`{export}` is not a module folder: it has no `module.json` at its root. \
                 Exports name module folders — add `{export}/module.json`, or drop `{export}` \
                 from the project's `exports` list."
            )
        } else {
            format!(
                "Export `{export}` names a folder that is not in this project. \
                 Create `{export}/module.json`, or drop `{export}` from the project's \
                 `exports` list."
            )
        };
        findings.push(ExportFinding::error(export, message, None));
        return findings;
    };

    // Reachability walk from the folder's own root module: every node
    // artifact the module pulls in, and every asset those artifacts name.
    // Files nobody references are not this check's business.
    let mut visited: BTreeSet<LpPathBuf> = BTreeSet::new();
    let mut queue: Vec<LpPathBuf> = alloc::vec![module_path.clone()];
    visited.insert(module_path.clone());

    while let Some(path) = queue.pop() {
        let bytes = if path == module_path {
            module_bytes
        } else {
            match files.get(path.as_path()) {
                Some(bytes) => bytes,
                None => {
                    findings.push(ExportFinding::error(
                        export,
                        format!(
                            "`{}` references `{}`, which is not in the export folder. \
                             Add the missing file or fix the reference.",
                            export,
                            path.as_str()
                        ),
                        Some(path.as_str().to_string()),
                    ));
                    continue;
                }
            }
        };

        let Ok(text) = core::str::from_utf8(bytes) else {
            findings.push(ExportFinding::error(
                export,
                format!(
                    "`{}` is not valid UTF-8, so it cannot be checked. \
                     Re-save it as UTF-8 text.",
                    path.as_str()
                ),
                Some(path.as_str().to_string()),
            ));
            continue;
        };
        let def = match NodeDef::from_json_str(text) {
            Ok(def) => def,
            Err(error) => {
                findings.push(ExportFinding::error(
                    export,
                    format!(
                        "`{}` does not parse as a node artifact ({error}). \
                         Fix the file — an export cannot be checked, or vendored, \
                         while part of it is unreadable.",
                        path.as_str()
                    ),
                    Some(path.as_str().to_string()),
                ));
                continue;
            }
        };

        if path == module_path {
            let Some(module) = def.as_module() else {
                findings.push(ExportFinding::error(
                    export,
                    format!(
                        "`{export}/module.json` is a `{}` node, not a module. \
                         An export names a module folder; make its root artifact \
                         `kind = \"Module\"`.",
                        def.kind_name()
                    ),
                    Some(module_path.as_str().to_string()),
                ));
                continue;
            };
            findings.extend(check_provenance(export, module, module_path.as_path()));
        }

        // Child node refs (`{ "ref": "./clock.json" }`): walked, so the whole
        // subtree of the export gets checked.
        for site in def.invocation_sites() {
            let Some(specifier) = site.invocation.ref_specifier() else {
                continue;
            };
            match resolve_artifact_specifier(path.as_path(), &specifier) {
                Ok(target) => {
                    if !is_inside(&target, &root) {
                        findings.push(escape_finding(export, &path, target.as_str(), &root));
                        continue;
                    }
                    if visited.insert(target.clone()) {
                        queue.push(target);
                    }
                }
                Err(error) => {
                    findings.push(unresolvable_finding(export, &path, &error.to_string()));
                }
            }
        }

        // Asset refs (`*.glsl` shader sources, `*.map2d.json` mappings):
        // checked for escape but never walked — they are not node artifacts.
        match def.referenced_asset_paths(path.as_path()) {
            Ok(assets) => {
                for asset in assets {
                    if !is_inside(&asset, &root) {
                        findings.push(escape_finding(export, &path, asset.as_str(), &root));
                    }
                }
            }
            Err(error) => {
                findings.push(unresolvable_finding(export, &path, &error.to_string()));
            }
        }
    }

    findings
}

/// `provenance` on the exported module, with `license` singled out: an
/// exported module is something someone else picks up, and a pattern with no
/// stated license is a pattern nobody can safely reuse.
fn check_provenance(
    export: &str,
    module: &crate::ModuleDef,
    module_path: &LpPath,
) -> Vec<ExportFinding> {
    let path = Some(module_path.as_str().to_string());
    let Some(provenance) = module.provenance.data.as_ref() else {
        return alloc::vec![ExportFinding::warning(
            export,
            format!(
                "Exported module `{export}` states no provenance. \
                 Add `provenance` (author, version, license, created) to \
                 `{export}/module.json` so people who import it know where it \
                 came from and what they may do with it."
            ),
            path,
        )];
    };
    if provenance.is_empty() {
        return alloc::vec![ExportFinding::warning(
            export,
            format!(
                "Exported module `{export}` states no provenance. \
                 Add `provenance` (author, version, license, created) to \
                 `{export}/module.json` so people who import it know where it \
                 came from and what they may do with it."
            ),
            path,
        )];
    }
    let licensed = provenance
        .license
        .data
        .as_ref()
        .is_some_and(|slot| !slot.value().trim().is_empty());
    if !licensed {
        return alloc::vec![ExportFinding::warning(
            export,
            format!(
                "Exported module `{export}` states no license. \
                 Add `provenance.license` to `{export}/module.json` — without it \
                 nobody who imports `{export}` knows what they may do with it."
            ),
            path,
        )];
    }
    Vec::new()
}

fn escape_finding(export: &str, from: &LpPath, target: &str, root: &LpPath) -> ExportFinding {
    ExportFinding::error(
        export,
        format!(
            "`{}` references `{target}`, which is outside `{}`. \
             An export travels as its folder alone — move the file inside \
             `{}`, or copy it in.",
            from.as_str(),
            root.as_str(),
            root.as_str()
        ),
        Some(from.as_str().to_string()),
    )
}

fn unresolvable_finding(export: &str, from: &LpPath, error: &str) -> ExportFinding {
    ExportFinding::error(
        export,
        format!(
            "`{}` has a reference that does not resolve to a file inside the \
             export ({error}). Rewrite it as a folder-relative path.",
            from.as_str()
        ),
        Some(from.as_str().to_string()),
    )
}

/// Whether `path` lies strictly under the export folder `root`.
fn is_inside(path: &LpPathBuf, root: &LpPathBuf) -> bool {
    let prefix = format!("{}/", root.as_str());
    path.as_str().starts_with(&prefix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    const MODULE: &str = r#"{
  "kind": "Module",
  "nodes": { "shader": { "ref": "./shader.json" } },
  "provenance": { "author": "Yona", "license": "CC0-1.0" }
}"#;

    const SHADER: &str = r#"{
  "kind": "Shader",
  "source": "shader.glsl",
  "render_order": 0,
  "float_mode": "fixed"
}"#;

    const GLSL: &str = "void main() {}";

    fn exports() -> Vec<String> {
        vec![String::from("chase")]
    }

    fn set<'a>(files: &'a [(&'a str, &'a [u8])]) -> ExportFileSet<'a> {
        files.iter().map(|(path, bytes)| (*path, *bytes)).collect()
    }

    /// A self-contained, licensed export folder produces nothing at all.
    #[test]
    fn export_check_clean_folder_reports_nothing() {
        let files: &[(&str, &[u8])] = &[
            ("chase/module.json", MODULE.as_bytes()),
            ("chase/shader.json", SHADER.as_bytes()),
            ("chase/shader.glsl", GLSL.as_bytes()),
        ];
        let report = check_exports(&exports(), &set(files));
        assert!(report.is_empty(), "{:?}", report.findings);
        assert_eq!(report.worst(), None);
    }

    /// The leading-slash form (artifact locations) and the bare form
    /// (`read_all_files`) are the same file set.
    #[test]
    fn export_check_accepts_absolute_and_relative_paths() {
        let files: &[(&str, &[u8])] = &[
            ("/chase/module.json", MODULE.as_bytes()),
            ("/chase/shader.json", SHADER.as_bytes()),
            ("/chase/shader.glsl", GLSL.as_bytes()),
        ];
        assert!(check_exports(&exports(), &set(files)).is_empty());
    }

    /// D5: an asset ref that climbs out of the folder is an error naming the
    /// referring file and the escaping target.
    #[test]
    fn export_check_flags_asset_ref_escaping_the_folder() {
        let shader = r#"{
  "kind": "Shader",
  "source": "../common/simplex.glsl",
  "render_order": 0,
  "float_mode": "fixed"
}"#;
        let files: &[(&str, &[u8])] = &[
            ("chase/module.json", MODULE.as_bytes()),
            ("chase/shader.json", shader.as_bytes()),
        ];
        let report = check_exports(&exports(), &set(files));
        assert_eq!(report.worst(), Some(ExportSeverity::Error));
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.message.contains("simplex.glsl"))
            .unwrap_or_else(|| panic!("escape finding: {:?}", report.findings));
        assert_eq!(finding.export, "chase");
        assert_eq!(finding.path.as_deref(), Some("/chase/shader.json"));
        assert!(
            finding.message.contains("/common/simplex.glsl"),
            "{finding:?}"
        );
    }

    /// The same rule for a child *node* ref, which the walk follows.
    #[test]
    fn export_check_flags_node_ref_escaping_the_folder() {
        let module = r#"{
  "kind": "Module",
  "nodes": { "clock": { "ref": "../common/clock.json" } },
  "provenance": { "license": "CC0-1.0" }
}"#;
        let files: &[(&str, &[u8])] = &[("chase/module.json", module.as_bytes())];
        let report = check_exports(&exports(), &set(files));
        assert_eq!(report.findings.len(), 1, "{:?}", report.findings);
        assert_eq!(report.findings[0].severity, ExportSeverity::Error);
        assert!(
            report.findings[0].message.contains("/common/clock.json"),
            "{:?}",
            report.findings[0]
        );
    }

    /// A ref that escapes from a *nested* artifact is caught too — the walk
    /// resolves each ref against its own containing file.
    #[test]
    fn export_check_flags_escape_from_a_nested_artifact() {
        let inner = r#"{
  "kind": "Module",
  "nodes": { "shader": { "ref": "../../outside/shader.json" } }
}"#;
        let module = r#"{
  "kind": "Module",
  "nodes": { "inner": { "ref": "./inner/module.json" } },
  "provenance": { "license": "CC0-1.0" }
}"#;
        let files: &[(&str, &[u8])] = &[
            ("chase/module.json", module.as_bytes()),
            ("chase/inner/module.json", inner.as_bytes()),
        ];
        let report = check_exports(&exports(), &set(files));
        assert_eq!(report.findings.len(), 1, "{:?}", report.findings);
        assert_eq!(
            report.findings[0].path.as_deref(),
            Some("/chase/inner/module.json")
        );
    }

    /// D6: an `exports` entry naming a folder that is not there at all.
    #[test]
    fn export_check_flags_missing_folder() {
        let files: &[(&str, &[u8])] = &[("other/module.json", MODULE.as_bytes())];
        let report = check_exports(&exports(), &set(files));
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].severity, ExportSeverity::Error);
        assert!(
            report.findings[0].message.contains("not in this project"),
            "{:?}",
            report.findings[0]
        );
    }

    /// D6: a folder that exists but has no `module.json` root — a folder of
    /// loose files is not a module.
    #[test]
    fn export_check_flags_folder_without_module_json() {
        let files: &[(&str, &[u8])] = &[("chase/shader.json", SHADER.as_bytes())];
        let report = check_exports(&exports(), &set(files));
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].severity, ExportSeverity::Error);
        assert!(
            report.findings[0].message.contains("module.json"),
            "{:?}",
            report.findings[0]
        );
    }

    /// A folder root that is a node but not a Module is the same error class.
    #[test]
    fn export_check_flags_non_module_root_artifact() {
        let files: &[(&str, &[u8])] = &[("chase/module.json", SHADER.as_bytes())];
        let report = check_exports(&exports(), &set(files));
        assert_eq!(report.findings[0].severity, ExportSeverity::Error);
        assert!(
            report.findings[0].message.contains("not a module"),
            "{:?}",
            report.findings[0]
        );
    }

    /// D8: provenance absent entirely.
    #[test]
    fn export_check_warns_when_provenance_is_absent() {
        let module = r#"{ "kind": "Module", "nodes": {} }"#;
        let files: &[(&str, &[u8])] = &[("chase/module.json", module.as_bytes())];
        let report = check_exports(&exports(), &set(files));
        assert_eq!(report.worst(), Some(ExportSeverity::Warning));
        assert_eq!(report.findings.len(), 1);
        assert!(
            report.findings[0].message.contains("provenance"),
            "{:?}",
            report.findings[0]
        );
        assert_eq!(
            report.findings[0].path.as_deref(),
            Some("/chase/module.json")
        );
    }

    /// D8: provenance present but the *license* missing — the field that
    /// actually gates reuse gets its own message.
    #[test]
    fn export_check_warns_when_license_is_missing() {
        let module = r#"{
  "kind": "Module",
  "nodes": {},
  "provenance": { "author": "Yona", "version": "0.1" }
}"#;
        let files: &[(&str, &[u8])] = &[("chase/module.json", module.as_bytes())];
        let report = check_exports(&exports(), &set(files));
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].severity, ExportSeverity::Warning);
        assert!(
            report.findings[0].message.contains("license"),
            "{:?}",
            report.findings[0]
        );
    }

    /// A referenced file that is simply not in the subtree is an error too —
    /// otherwise a broken export reads as clean.
    #[test]
    fn export_check_flags_a_referenced_file_that_is_missing() {
        let files: &[(&str, &[u8])] = &[("chase/module.json", MODULE.as_bytes())];
        let report = check_exports(&exports(), &set(files));
        assert_eq!(report.findings.len(), 1, "{:?}", report.findings);
        assert!(
            report.findings[0].message.contains("shader.json"),
            "{:?}",
            report.findings[0]
        );
    }

    /// Unreadable JSON inside the export is an error, not a silent pass.
    #[test]
    fn export_check_flags_unparseable_artifact() {
        let files: &[(&str, &[u8])] = &[
            ("chase/module.json", MODULE.as_bytes()),
            ("chase/shader.json", b"{ not json"),
        ];
        let report = check_exports(&exports(), &set(files));
        assert_eq!(report.findings[0].severity, ExportSeverity::Error);
        assert_eq!(
            report.findings[0].path.as_deref(),
            Some("/chase/shader.json")
        );
    }

    /// Two exports are checked independently and their findings keep their
    /// own `export` tag.
    #[test]
    fn export_check_tags_findings_per_export() {
        let bare = r#"{ "kind": "Module", "nodes": {} }"#;
        let files: &[(&str, &[u8])] = &[
            ("chase/module.json", MODULE.as_bytes()),
            ("chase/shader.json", SHADER.as_bytes()),
            ("chase/shader.glsl", GLSL.as_bytes()),
            ("sparkle/module.json", bare.as_bytes()),
        ];
        let report = check_exports(
            &vec![String::from("chase"), String::from("sparkle")],
            &set(files),
        );
        assert_eq!(report.findings.len(), 1, "{:?}", report.findings);
        assert_eq!(report.findings[0].export, "sparkle");
        assert_eq!(report.for_export("chase").count(), 0);
        assert_eq!(report.for_export("sparkle").count(), 1);
    }

    /// A cycle between two module artifacts terminates (the walk is
    /// visited-guarded), and reports nothing but the provenance thinness.
    #[test]
    fn export_check_terminates_on_a_reference_cycle() {
        let a = r#"{
  "kind": "Module",
  "nodes": { "b": { "ref": "./b.json" } },
  "provenance": { "license": "CC0-1.0" }
}"#;
        let b = r#"{ "kind": "Module", "nodes": { "a": { "ref": "./module.json" } } }"#;
        let files: &[(&str, &[u8])] = &[
            ("chase/module.json", a.as_bytes()),
            ("chase/b.json", b.as_bytes()),
        ];
        assert!(check_exports(&exports(), &set(files)).is_empty());
    }

    #[test]
    fn export_report_worst_takes_the_error() {
        let report = ExportLintReport::new(vec![
            ExportFinding::warning("chase", String::from("thin"), None),
            ExportFinding::error("chase", String::from("broken"), None),
        ]);
        assert_eq!(report.worst(), Some(ExportSeverity::Error));
        assert!(ExportLintReport::default().worst().is_none());
    }
}
