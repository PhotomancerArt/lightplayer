//! "New project from this…": compose a fresh pattern project around an
//! export vendored out of a library package (module authoring unit, P5).
//!
//! The gesture is the card-side twin of the picker's import. Import says
//! *bring this into what I am working on*; this one says *give me a
//! workbench built around it* — a rig to judge the effect on, and the
//! export already designated so the new project is publishable from its
//! first frame.
//!
//! **Round-one composition** (the phase file's pre-approved simplification):
//! the project IS P4's 1D pattern template with its `effect/` folder
//! replaced by the source's chosen export folder. Consequences worth
//! naming, because a later round will want to revisit them:
//!
//! - the vendored folder keeps the template's `effect` NAME, not the
//!   source export's, so the template's root `module.json` (which
//!   references `./effect/module.json`) needs no rewriting and the
//!   manifest's `exports` stays `["effect"]`;
//! - the rig is always the 1D one. Choosing 1D vs 2D from the export's own
//!   content is T2 (shader space) territory, and guessing it here would be
//!   a guess the author cannot see, let alone correct.
//!
//! Everything else — the relative-refs-survive-re-rooting property, the
//! R14 provenance stamp — is the import path's, reused verbatim from
//! [`crate::app::project::node::import_pattern`]: one vendoring, two
//! gestures.

use lpc_model::{SlotShapeRegistry, pattern_project_files_1d};

use crate::UiError;
use crate::app::project::node::import_pattern::{
    collect_export_folder, source_manifest, stamp_module_provenance,
};

/// The folder name the composed project's export lands under — the
/// template's own, see the module docs.
const COMPOSED_EXPORT_FOLDER: &str = "effect";

/// Compose the files of a new pattern project built around `export` of
/// `source_files`.
///
/// `name` is the label the library slugs, dates, and dedupes the package
/// from; it is also what the container manifest's `name` says.
pub fn project_files_from_export(
    source_files: &[(String, Vec<u8>)],
    export: &str,
    name: &str,
) -> Result<Vec<(String, Vec<u8>)>, UiError> {
    let export = export.trim();
    if export.is_empty() {
        return Err(UiError::UnsupportedAction(
            "starting a project from a pattern names an export folder".to_string(),
        ));
    }
    let registry = SlotShapeRegistry::default();
    let vendored = collect_export_folder(source_files, export)?;
    let body = stamp_module_provenance(
        &vendored.body,
        source_manifest(source_files).as_ref(),
        &registry,
    )?;

    let template = pattern_project_files_1d(name, &registry).map_err(|error| {
        UiError::UnsupportedAction(format!("could not build the pattern rig: {error}"))
    })?;

    let template_export_prefix = format!("{COMPOSED_EXPORT_FOLDER}/");
    let mut files: Vec<(String, Vec<u8>)> = template
        .into_iter()
        .filter(|(path, _)| !path.starts_with(&template_export_prefix))
        .collect();
    files.push((format!("{template_export_prefix}module.json"), body));
    for (relative, bytes) in vendored.assets {
        files.push((format!("{template_export_prefix}{relative}"), bytes));
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::{ProjectKind, ProjectManifest};

    /// A source pattern package: the 1D template itself, which is exactly
    /// the shape a user's pattern project has.
    fn source_package() -> Vec<(String, Vec<u8>)> {
        pattern_project_files_1d("Aurora", &SlotShapeRegistry::default()).expect("template builds")
    }

    fn file<'a>(files: &'a [(String, Vec<u8>)], path: &str) -> &'a [u8] {
        files
            .iter()
            .find(|(name, _)| name == path)
            .unwrap_or_else(|| panic!("{path} is in the composition"))
            .1
            .as_slice()
    }

    /// The composed project is a runnable workbench: rig at the root, the
    /// vendored export in `effect/`, and the manifest already designating
    /// it — so P3's exports rail is there before the first gesture.
    #[test]
    fn the_composition_is_a_rig_plus_the_vendored_export_pre_designated() {
        let source = source_package();
        let files = project_files_from_export(&source, "effect", "Aurora remix").expect("composes");

        let manifest =
            ProjectManifest::read_json(core::str::from_utf8(file(&files, "project.json")).unwrap())
                .expect("manifest parses");
        assert_eq!(
            manifest.project_kind(),
            ProjectKind::Pattern {
                exports: vec!["effect".to_string()]
            }
        );
        assert_eq!(manifest.name.as_deref(), Some("Aurora remix"));

        // The rig came from the template, the export from the source.
        for path in ["module.json", "clock.json", "strip_300.json"] {
            assert!(
                files.iter().any(|(name, _)| name == path),
                "the 1D rig ships {path}"
            );
        }
        assert_eq!(
            file(&files, "effect/module.json"),
            file(&source, "effect/module.json"),
            "the vendored module arrives byte-identical (it already had provenance)"
        );
        assert_eq!(
            file(&files, "effect/shader.glsl"),
            file(&source, "effect/shader.glsl")
        );

        // Nothing of the template's OWN export survived alongside it: one
        // `effect/` folder, and it is the source's.
        let exported: Vec<&str> = files
            .iter()
            .map(|(name, _)| name.as_str())
            .filter(|name| name.starts_with("effect/"))
            .collect();
        assert_eq!(
            exported,
            vec![
                "effect/module.json",
                "effect/shader.glsl",
                "effect/shader.json"
            ],
        );
        // Every path is unique — a duplicate would install unpredictably.
        let mut names: Vec<&String> = files.iter().map(|(name, _)| name).collect();
        names.sort();
        let before = names.len();
        names.dedup();
        assert_eq!(names.len(), before, "no path is written twice");
    }

    /// The vendored folder's internal refs are relative and must survive
    /// the move unedited — that is what makes re-rooting safe at all. A ref
    /// that had picked up a leading `/` (or a `..`) on the way would name a
    /// file outside the folder and resolve to nothing.
    #[test]
    fn the_vendored_folders_internal_refs_stay_relative_and_unedited() {
        let files = project_files_from_export(&source_package(), "effect", "R").expect("composes");
        let text = core::str::from_utf8(file(&files, "effect/module.json")).unwrap();
        assert!(text.contains("\"ref\": \"shader.json\""), "{text}");
        assert!(!text.contains("\"/"), "no ref became absolute: {text}");
        assert!(
            !text.contains(".."),
            "no ref reaches outside the folder: {text}"
        );
    }

    #[test]
    fn an_export_that_is_not_there_refuses() {
        let error = project_files_from_export(&source_package(), "nope", "R")
            .expect_err("unknown export refuses");
        assert!(format!("{error:?}").contains("nope"), "{error:?}");
    }

    /// An unprovenanced export inherits the SOURCE project's attribution on
    /// the way into the new project (R14) — the copy still says who wrote
    /// the pattern.
    #[test]
    fn an_unprovenanced_export_arrives_stamped() {
        let mut source = source_package();
        for (path, bytes) in source.iter_mut() {
            if path == "project.json" {
                let mut manifest =
                    ProjectManifest::read_json(core::str::from_utf8(bytes).unwrap()).unwrap();
                manifest.author = Some("Yona".to_string());
                *bytes = manifest.write_json().into_bytes();
            }
            if path == "effect/module.json" {
                *bytes = br#"{
  "kind": "Module",
  "nodes": {
    "shader": { "ref": "./shader.json" }
  }
}"#
                .to_vec();
            }
        }

        let files = project_files_from_export(&source, "effect", "R").expect("composes");
        let def = lpc_model::NodeDef::from_json_str(
            core::str::from_utf8(file(&files, "effect/module.json")).unwrap(),
        )
        .expect("module parses");
        let provenance = def
            .as_module()
            .expect("module")
            .provenance
            .data
            .clone()
            .expect("stamped on the way in");
        assert_eq!(provenance.author.data.unwrap().value(), "Yona");
    }
}
