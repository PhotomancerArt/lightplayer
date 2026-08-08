//! Vendoring mechanics for [`NodeImportOp`](super::NodeImportOp): lift one
//! export folder out of a library package and re-root it under the open
//! project's `modules/` directory (module authoring unit, P5).
//!
//! Two properties this file exists to keep true:
//!
//! - **The folder moves whole and unedited.** An export's internal
//!   references are relative (`"./shader.json"`, `"./shader.glsl"`), so
//!   re-rooting `fire/**` to `modules/fire/**` preserves every one of them
//!   by construction. Nothing here rewrites a ref, and nothing should: the
//!   moment a vendoring path starts editing paths inside the copy, a folder
//!   that resolved in its home project stops resolving in yours.
//! - **Provenance survives the copy.** R14: an export with no `provenance`
//!   of its own inherits the SOURCE project's manifest attribution as it
//!   leaves, so the copy can still say who wrote it. Injected through the
//!   canonical def writer — never by splicing JSON text, which would both
//!   churn the bytes and break the first time a field it does not know
//!   about is authored.

use lpc_model::{ModuleDef, NodeDef, OptionSlot, ProjectManifest, ProvenanceDef, ValueSlot};

use crate::UiError;

/// The directory vendored modules land in. A dedicated folder (rather than
/// the project root, where the pattern templates put their OWN export)
/// keeps "what I wrote" and "what I imported" legible at a glance in the
/// file tree — the copy is still fully the user's to edit.
pub const VENDOR_DIR: &str = "modules";

/// One export folder, lifted and ready to send as a `CreateNode`.
#[derive(Clone, Debug, PartialEq)]
pub struct VendoredExport {
    /// The module def's bytes (provenance already stamped, if it needed it).
    pub body: Vec<u8>,
    /// Every other file in the folder, as (path relative to the folder,
    /// bytes) — sorted, so a vendoring is byte-reproducible.
    pub assets: Vec<(String, Vec<u8>)>,
}

impl VendoredExport {
    /// The def file path this export takes under `key`, project-relative.
    pub fn def_path(key: &str) -> String {
        format!("./{VENDOR_DIR}/{key}/module.json")
    }

    /// Asset paths re-rooted under `key`, project-relative. The folder's own
    /// internal refs are untouched — see the module docs.
    pub fn asset_paths(&self, key: &str) -> Vec<(String, Vec<u8>)> {
        self.assets
            .iter()
            .map(|(relative, bytes)| (format!("./{VENDOR_DIR}/{key}/{relative}"), bytes.clone()))
            .collect()
    }
}

/// Collect `export`'s folder out of a package's file list.
///
/// `files` is the `(relative path, bytes)` shape every library read hands
/// back ([`crate::app::library::PackageHandle::read_all_files`]).
pub fn collect_export_folder(
    files: &[(String, Vec<u8>)],
    export: &str,
) -> Result<VendoredExport, UiError> {
    let prefix = format!("{export}/");
    let mut body = None;
    let mut assets = Vec::new();
    for (path, bytes) in files {
        let Some(relative) = path.strip_prefix(&prefix) else {
            continue;
        };
        if relative == "module.json" {
            body = Some(bytes.clone());
        } else {
            assets.push((relative.to_string(), bytes.clone()));
        }
    }
    let body = body.ok_or_else(|| {
        UiError::UnsupportedAction(format!(
            "{export} is listed as an export but has no {export}/module.json to import"
        ))
    })?;
    assets.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(VendoredExport { body, assets })
}

/// The source project's container manifest, parsed out of its file list.
/// `None` when the package has no readable `project.json` — a vendoring
/// still proceeds, just without an attribution to inherit.
pub fn source_manifest(files: &[(String, Vec<u8>)]) -> Option<ProjectManifest> {
    let (_, bytes) = files.iter().find(|(path, _)| path == "project.json")?;
    ProjectManifest::read_json(core::str::from_utf8(bytes).ok()?).ok()
}

/// R14: stamp `manifest`'s attribution onto a module def that carries none.
///
/// Returns the bytes UNCHANGED when there is nothing to do — the export
/// already states its own provenance, the source project states none, or
/// the def is not a module. Rewriting in that case would churn bytes for
/// no gain, and a copy whose bytes match its source is the easiest kind to
/// reason about.
pub fn stamp_module_provenance(
    body: &[u8],
    manifest: Option<&ProjectManifest>,
    registry: &lpc_model::SlotShapeRegistry,
) -> Result<Vec<u8>, UiError> {
    let Some(manifest) = manifest else {
        return Ok(body.to_vec());
    };
    let inherited = ProvenanceDef {
        author: optional(manifest.author.as_deref()),
        version: optional(manifest.version.as_deref()),
        license: optional(manifest.license.as_deref()),
        created: optional(manifest.created.as_deref()),
    };
    if inherited.is_empty() {
        return Ok(body.to_vec());
    }
    let text = core::str::from_utf8(body).map_err(|_| {
        UiError::UnsupportedAction("this export's module.json is not UTF-8".to_string())
    })?;
    // Parse through the MODEL's static registry, not a synced one: a synced
    // registry describes shapes for editing and carries no creatable
    // factories, so it cannot read an authored def back (the copy-node
    // path's lesson). Writing goes through the caller's registry, exactly
    // as `create_node` does.
    let mut def = NodeDef::from_json_str(text).map_err(|error| {
        UiError::UnsupportedAction(format!("this export's module.json did not parse: {error}"))
    })?;
    let NodeDef::Module(ModuleDef { provenance, .. }) = &mut def else {
        return Ok(body.to_vec());
    };
    if provenance.data.is_some() {
        return Ok(body.to_vec());
    }
    *provenance = OptionSlot::some(inherited);
    def.write_json(registry)
        .map(String::into_bytes)
        .map_err(|error| {
            UiError::Project(format!(
                "cannot serialize the imported module definition: {error}"
            ))
        })
}

fn optional(value: Option<&str>) -> OptionSlot<ValueSlot<String>> {
    match value.map(str::trim).filter(|text| !text.is_empty()) {
        Some(text) => OptionSlot::some(ValueSlot::new(text.to_string())),
        None => OptionSlot::none(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(entries: &[(&str, &str)]) -> Vec<(String, Vec<u8>)> {
        entries
            .iter()
            .map(|(path, body)| ((*path).to_string(), body.as_bytes().to_vec()))
            .collect()
    }

    const UNPROVENANCED: &str = r#"{
  "kind": "Module",
  "nodes": {
    "shader": { "ref": "./shader.json" }
  }
}"#;

    #[test]
    fn the_export_folder_arrives_whole_and_only_that_folder() {
        let source = files(&[
            ("project.json", "{}"),
            ("module.json", "{}"),
            ("fire/module.json", UNPROVENANCED),
            ("fire/shader.json", "{}"),
            ("fire/shader.glsl", "void main(){}"),
            ("fireplace/module.json", "{}"),
            ("ice/module.json", "{}"),
        ]);

        let vendored = collect_export_folder(&source, "fire").expect("fire collects");
        assert_eq!(vendored.body, UNPROVENANCED.as_bytes());
        let names: Vec<&str> = vendored
            .assets
            .iter()
            .map(|(path, _)| path.as_str())
            .collect();
        assert_eq!(
            names,
            vec!["shader.glsl", "shader.json"],
            "a sibling folder whose name merely starts with `fire` is not part of it",
        );
    }

    /// The whole correctness claim of re-rooting: paths gain a prefix and
    /// nothing else, so the folder's own relative refs still resolve.
    #[test]
    fn re_rooting_only_prefixes_paths() {
        let vendored = VendoredExport {
            body: b"{}".to_vec(),
            assets: vec![
                ("shader.glsl".to_string(), b"a".to_vec()),
                ("nested/palette.json".to_string(), b"b".to_vec()),
            ],
        };
        assert_eq!(
            VendoredExport::def_path("fire_2"),
            "./modules/fire_2/module.json"
        );
        let paths: Vec<String> = vendored
            .asset_paths("fire_2")
            .into_iter()
            .map(|(path, _)| path)
            .collect();
        assert_eq!(
            paths,
            vec![
                "./modules/fire_2/shader.glsl".to_string(),
                "./modules/fire_2/nested/palette.json".to_string(),
            ]
        );
    }

    #[test]
    fn a_missing_module_json_is_a_refusal_naming_the_export() {
        let error = collect_export_folder(&files(&[("fire/shader.json", "{}")]), "fire")
            .expect_err("no module.json");
        assert!(
            format!("{error:?}").contains("fire/module.json"),
            "{error:?}"
        );
    }

    #[test]
    fn an_unprovenanced_export_inherits_the_source_projects_attribution() {
        let manifest = ProjectManifest::read_json(
            r#"{
  "format": 5,
  "name": "aurora",
  "author": "Yona",
  "version": "3",
  "license": "CC0-1.0",
  "created": "2026-08-01"
}
"#,
        )
        .expect("manifest parses");
        let registry = lpc_model::SlotShapeRegistry::default();

        let stamped =
            stamp_module_provenance(UNPROVENANCED.as_bytes(), Some(&manifest), &registry).unwrap();
        let def = NodeDef::from_json_str(core::str::from_utf8(&stamped).unwrap()).unwrap();
        let provenance = def
            .as_module()
            .expect("module")
            .provenance
            .data
            .clone()
            .expect("provenance stamped");
        assert_eq!(provenance.author.data.unwrap().value(), "Yona");
        assert_eq!(provenance.version.data.unwrap().value(), "3");
        assert_eq!(provenance.license.data.unwrap().value(), "CC0-1.0");
        assert_eq!(provenance.created.data.unwrap().value(), "2026-08-01");
        // The nodes map came through untouched — the stamp adds a field,
        // it does not re-author the module.
        assert!(
            def.as_module()
                .unwrap()
                .nodes
                .entries
                .contains_key("shader")
        );
    }

    /// An export that already says who wrote it keeps saying it, byte for
    /// byte: the source project's manifest never overwrites an authored
    /// attribution.
    #[test]
    fn an_authored_provenance_is_never_overwritten_and_the_bytes_do_not_move() {
        let authored = r#"{
  "kind": "Module",
  "nodes": {
    "shader": { "ref": "./shader.json" }
  },
  "provenance": {
    "author": "Someone Else"
  }
}"#;
        let manifest =
            ProjectManifest::read_json("{\n  \"format\": 5,\n  \"author\": \"Yona\"\n}\n")
                .expect("manifest parses");
        let registry = lpc_model::SlotShapeRegistry::default();
        assert_eq!(
            stamp_module_provenance(authored.as_bytes(), Some(&manifest), &registry).unwrap(),
            authored.as_bytes(),
        );
    }

    /// A source project with no attribution of its own has nothing to
    /// inherit, so the copy is byte-identical to the original.
    #[test]
    fn nothing_to_inherit_leaves_the_bytes_alone() {
        let registry = lpc_model::SlotShapeRegistry::default();
        let bare = ProjectManifest::read_json("{\n  \"format\": 5\n}\n").unwrap();
        assert_eq!(
            stamp_module_provenance(UNPROVENANCED.as_bytes(), Some(&bare), &registry).unwrap(),
            UNPROVENANCED.as_bytes()
        );
        assert_eq!(
            stamp_module_provenance(UNPROVENANCED.as_bytes(), None, &registry).unwrap(),
            UNPROVENANCED.as_bytes()
        );
    }

    #[test]
    fn the_source_manifest_reads_out_of_the_file_list() {
        let source = files(&[(
            "project.json",
            "{\n  \"format\": 5,\n  \"name\": \"aurora\"\n}\n",
        )]);
        assert_eq!(
            source_manifest(&source).and_then(|manifest| manifest.name),
            Some("aurora".to_string())
        );
        assert!(source_manifest(&files(&[("module.json", "{}")])).is_none());
    }
}
