//! What each New-menu template actually writes into the new package.
//!
//! The compositions themselves live in `lpc-model`
//! ([`pattern_project_files_1d`] / [`pattern_project_files_2d`], beside
//! `starter_project_files`) because they are pure authored data with no
//! Studio in them — `lp-cli` could scaffold the same tree tomorrow. This
//! module is the thin Studio-side seam: pick the composition for a
//! [`ProjectTemplate`], hand it the shape registry, and surface a
//! serialization failure as a [`UiError`].
//!
//! [`ProjectTemplate::Blank`] generates **nothing** — `None`, not an empty
//! `Vec`. `LibraryStore::install_package` writes its minimal manifest and
//! root module only when the incoming file list is empty, so the blank
//! create must stay the store's own path, byte-for-byte as before.
//!
//! Sibling of [`super::board_project`], the other per-create generator: a
//! template is the same idea without a board.

use lpc_model::{SlotShapeRegistry, pattern_project_files_1d, pattern_project_files_2d};

use super::home_op::ProjectTemplate;
use crate::UiError;

/// The files `template` contributes to a freshly created package, or
/// `None` when the library's own blank scaffold is the answer.
pub fn template_project_files(
    template: ProjectTemplate,
) -> Result<Option<Vec<(String, Vec<u8>)>>, UiError> {
    // The shape registry is a pure catalog of authored slot shapes with no
    // project in it, so the default one is the whole seam — the same call
    // `lp create` makes before writing the starter project, and the same
    // one `ProjectController` starts from.
    let registry = SlotShapeRegistry::default();
    let files = match template {
        ProjectTemplate::Blank => return Ok(None),
        ProjectTemplate::Pattern1d => pattern_project_files_1d("Pattern", &registry),
        ProjectTemplate::Pattern2d => pattern_project_files_2d("Pattern", &registry),
    };
    // The composition's only failure mode is a def that will not serialize
    // — a bug in this build, not something the user did — so it surfaces
    // as a refusal naming the template rather than a silent blank project.
    files.map(Some).map_err(|error| {
        UiError::UnsupportedAction(format!(
            "could not build the {} template: {error}",
            template.label()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Blank generates nothing at all: the store's own scaffold is what a
    /// blank project has always been, and an empty `Vec` here would be a
    /// different (and broken) thing than `None`.
    #[test]
    fn the_blank_template_generates_no_files() {
        assert_eq!(
            template_project_files(ProjectTemplate::Blank).unwrap(),
            None
        );
    }

    /// Both pattern templates produce a package whose manifest already
    /// designates the export — the whole point of P4 (P3's exports rail
    /// then appears with no further gesture).
    #[test]
    fn both_pattern_templates_arrive_pre_designated() {
        for template in [ProjectTemplate::Pattern1d, ProjectTemplate::Pattern2d] {
            let files = template_project_files(template)
                .unwrap()
                .unwrap_or_else(|| panic!("{template:?} generates files"));
            let (_, bytes) = files
                .iter()
                .find(|(path, _)| path == "project.json")
                .expect("the container manifest deploys");
            let manifest =
                lpc_model::ProjectManifest::read_json(std::str::from_utf8(bytes).unwrap())
                    .expect("manifest parses");
            assert_eq!(
                manifest.project_kind(),
                lpc_model::ProjectKind::Pattern {
                    exports: vec!["effect".to_string()]
                },
                "{template:?}"
            );
            assert!(files.iter().any(|(path, _)| path == "effect/module.json"));
        }
    }

    /// The generated mapping documents are real: they parse AND resolve to
    /// the lamp counts the rig names. (`lpc-model` cannot check this — it
    /// carries no mapping dependency — so the assertion lives here, the
    /// `board_project` precedent.)
    #[test]
    fn the_generated_rigs_resolve_to_their_lamp_counts() {
        let files = template_project_files(ProjectTemplate::Pattern1d)
            .unwrap()
            .unwrap();
        for (path, expected) in [
            ("strip_300.map2d.json", 300),
            ("matrix_32x16.map2d.json", 512),
        ] {
            let (_, bytes) = files
                .iter()
                .find(|(name, _)| name == path)
                .unwrap_or_else(|| panic!("template ships {path}"));
            let doc = lpc_mapping::Map2dDoc::from_json(std::str::from_utf8(bytes).unwrap())
                .unwrap_or_else(|e| panic!("{path} parses: {e}"));
            let resolved =
                lpc_mapping::resolve(&doc).unwrap_or_else(|e| panic!("{path} resolves: {e}"));
            assert_eq!(resolved.lamps.len(), expected, "{path}");
        }
    }
}
