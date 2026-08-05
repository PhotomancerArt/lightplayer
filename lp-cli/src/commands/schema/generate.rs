//! `lp-cli schema gen`: emit the checked-in `schemas/` tree.
//!
//! Generates, from the model's static slot shape catalog and the board
//! manifest serde types:
//!
//! - `project.schema.json` — the `project.json` container manifest (not a
//!   node envelope): `format` pinned to
//!   [`lpc_model::PROJECT_FORMAT_VERSION`], optional `uid`/`name`/`target`,
//!   nothing else (mirrors the loader gate in `lpc-registry`, which
//!   hard-refuses a missing/malformed manifest and rejects mismatched
//!   formats). ⚠️ `author`/`version`/`license`/`created` are real
//!   `ProjectManifest` fields the schema does not yet declare — a
//!   pre-existing gap, not introduced here (P02 of the gallery-rework plan
//!   only adds `target`).
//! - `module.schema.json` — the `module.json` root module node artifact:
//!   top-level `kind: "Module"` const plus the compiled `ModuleDef` shape.
//! - `node.schema.json` — any authored node artifact: `oneOf` over every
//!   registered node kind, discriminated by top-level `kind`.
//! - `hardware.schema.json` — the plain-serde board manifest
//!   ([`lpc_hardware::HardwareManifestFile`]) via `schemars`.
//! - `board-display.schema.json` — the board display sidecar
//!   ([`lpa_boards::BoardDisplayFile`], `*.display.json`) via `schemars`.
//! - `shapes/<shape-name>.json` — the serialized [`SlotShape`] for each
//!   registered static shape (the source-of-truth dump a future format
//!   upgrader consumes), plus `shapes/_index.json` mapping registry shape
//!   name → raw shape id.
//!
//! Outputs are byte-stable: every JSON object is rewritten with explicitly
//! sorted keys, pretty-printed, with a trailing newline, so consecutive runs
//! are byte-identical and CI can diff against the checked-in files.
//!
//! `$id` convention: `https://lightplayer.dev/schemas/<file>` on the three
//! JSON Schema documents. Shape dumps are model serializations, not JSON
//! Schemas, so they carry no `$id`/`$schema` header.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use lpc_hardware::HardwareManifestFile;
use lpc_model::schema_gen::{compile_registered_slot_shape_schema, compile_slot_shape_schema};
use lpc_model::{
    ModuleDef, NodeArtifact, PROJECT_FORMAT_VERSION, Revision, SlotEnumEncoding, SlotMeta,
    SlotShape, SlotShapeRegistry, SlotVariantShape, StaticSlotShape,
};
use serde_json::{Map, Value, json};

use super::args::GenArgs;

/// Base for the `$id` of each generated JSON Schema document.
const SCHEMA_ID_BASE: &str = "https://lightplayer.dev/schemas/";

/// JSON Schema dialect every generated schema declares.
const SCHEMA_DIALECT: &str = "https://json-schema.org/draft/2020-12/schema";

pub fn handle_gen(args: GenArgs) -> Result<()> {
    let out_dir = match args.out {
        Some(dir) => dir,
        None => find_repo_root()?.join("schemas"),
    };
    let outputs = generate_outputs()?;
    if args.check {
        check_outputs(&out_dir, &outputs)
    } else {
        write_outputs(&out_dir, &outputs)
    }
}

/// Generate every output file as `relative path → contents`.
///
/// Pure in-memory so `--check` and the determinism test share the exact bytes
/// `gen` writes.
fn generate_outputs() -> Result<BTreeMap<String, String>> {
    let registry = populated_registry()?;
    let mut outputs = BTreeMap::new();

    outputs.insert(
        String::from("project.schema.json"),
        render_schema(project_schema()?, "project.schema.json")?,
    );
    outputs.insert(
        String::from("module.schema.json"),
        render_schema(module_schema(&registry)?, "module.schema.json")?,
    );
    outputs.insert(
        String::from("node.schema.json"),
        render_schema(node_schema(&registry)?, "node.schema.json")?,
    );
    outputs.insert(
        String::from("hardware.schema.json"),
        render_schema(hardware_schema()?, "hardware.schema.json")?,
    );
    outputs.insert(
        String::from("board-display.schema.json"),
        render_schema(board_display_schema()?, "board-display.schema.json")?,
    );

    let mut index = Map::new();
    for id in lpc_model::slot_shapes::static_slot_shape_ids() {
        let name = lpc_model::slot_shapes::static_slot_shape_name(*id)
            .ok_or_else(|| anyhow!("static shape {id} has no registry name"))?;
        let shape = registry
            .get(id)
            .ok_or_else(|| anyhow!("shape {name} ({id}) missing from populated registry"))?;
        let value = serde_json::to_value(shape)
            .with_context(|| format!("serializing slot shape {name}"))?;
        outputs.insert(
            format!("shapes/{}.json", shape_file_stem(name)),
            render(value)?,
        );
        index.insert(String::from(name), json!(id.raw()));
    }
    outputs.insert(
        String::from("shapes/_index.json"),
        render(Value::Object(index))?,
    );

    Ok(outputs)
}

/// Registry carrying every statically cataloged shape (all node kinds plus
/// the shared/value shapes they reference), registered under its catalog name
/// so compiled `$defs` keys and shape dump file names are the human names.
///
/// `SlotShapeRegistry::default()` serves static shapes only through the
/// `SlotShapeLookup` fallback; the schema compiler resolves `Ref`s through the
/// registry's own entries, so the catalog is materialized here explicitly.
fn populated_registry() -> Result<SlotShapeRegistry> {
    let mut registry = SlotShapeRegistry::default();
    for id in lpc_model::slot_shapes::static_slot_shape_ids() {
        let entry = SlotShapeRegistry::static_catalog_entry(*id)
            .ok_or_else(|| anyhow!("static catalog has no shape for id {id}"))?;
        let shape = entry.value().clone();
        match entry.name() {
            Some(name) => {
                registry.register_shape_named_with_version(Revision::default(), *id, name, shape)
            }
            None => registry.register_shape_with_version(Revision::default(), *id, shape),
        }
        .map_err(|error| anyhow!("registering static shape {id}: {error}"))?;
    }
    Ok(registry)
}

/// Schema for the `project.json` container manifest.
///
/// The container is NOT a node envelope (docs/design/modules.md §1/§6): it
/// carries the workspace identity — `format` pinned to the current
/// [`PROJECT_FORMAT_VERSION`], optional `uid`, `name`, and `target` — and
/// nothing else (`additionalProperties: false` mirrors the strict streaming
/// reader in `lpc_model::ProjectManifest`, which rejects unknown fields).
fn project_schema() -> Result<Value> {
    Ok(json!({
        "title": "LightPlayer project container manifest",
        "description": "Workspace identity of a project folder: format \
    version, library uid, and display name. Not a node artifact — the root \
    module node lives in module.json.",
        "type": "object",
        "properties": {
            "format": {
                "title": "Authored format version",
                "const": PROJECT_FORMAT_VERSION,
            },
            "uid": {
                "title": "Stable project identity (prj_…, base-62)",
                "type": "string",
            },
            "name": {
                "title": "Human-readable project name",
                "type": "string",
            },
            "target": {
                "title": "Advisory board target",
                "description": "A board catalog id in the registry's \
    vendor/product vocabulary (e.g. espressif/esp32-c6-devkitc-1), the same \
    strings as RegisteredDevice.board_id. Advisory only — the engine never \
    reads it.",
                "type": "string",
            },
        },
        "required": ["format"],
        "additionalProperties": false,
    }))
}

/// Schema for the `module.json` root module node artifact.
///
/// Compiled as a single-variant `kind`-tagged enum over `ModuleDef`, so the
/// discriminator/flattening semantics come from the same compiler that
/// produces `node.schema.json`.
fn module_schema(registry: &SlotShapeRegistry) -> Result<Value> {
    let root_shape = SlotShape::Enum {
        meta: SlotMeta::empty(),
        encoding: SlotEnumEncoding::tagged_kind(),
        variants: vec![
            SlotVariantShape::new("Module", SlotShape::reference(ModuleDef::SHAPE_ID))
                .map_err(|error| anyhow!("module variant name: {error}"))?,
        ],
    };
    let compiled = compile_slot_shape_schema(registry, &root_shape);
    let Value::Object(mut envelope) = compiled else {
        bail!("compiled module schema is not a JSON object");
    };

    // Unwrap the single oneOf branch into the document root, keeping the
    // compiler's envelope keys.
    let branches = envelope.remove("oneOf");
    let Some(Value::Array(mut branches)) = branches else {
        bail!("compiled module schema has no oneOf envelope");
    };
    let (Some(Value::Object(mut root)), true) = (branches.pop(), branches.is_empty()) else {
        bail!("compiled module schema is not a single-variant oneOf");
    };
    for key in ["$schema", "$defs"] {
        if let Some(value) = envelope.remove(key) {
            root.insert(String::from(key), value);
        }
    }
    root.insert(String::from("required"), json!(["kind"]));
    Ok(Value::Object(root))
}

/// Schema for any authored node artifact: the registered `NodeArtifact` root
/// shape, a `kind`-tagged enum over every registered node kind.
fn node_schema(registry: &SlotShapeRegistry) -> Result<Value> {
    compile_registered_slot_shape_schema(registry, NodeArtifact::SHAPE_ID)
        .ok_or_else(|| anyhow!("NodeArtifact shape missing from populated registry"))
}

/// Schema for `hardware.json` board manifests. The manifest is plain serde
/// (no slot shapes), so this comes from the schemars derives behind
/// `lpc-hardware`'s `schema-gen` feature.
fn hardware_schema() -> Result<Value> {
    let schema = schemars::schema_for!(HardwareManifestFile);
    serde_json::to_value(&schema).context("serializing hardware manifest schema")
}

/// Schema for `*.display.json` board display sidecars. Plain serde like the
/// hardware manifest; the types live app-side in `lpa-boards` so the runtime
/// manifest (compiled into firmware) never carries display data.
fn board_display_schema() -> Result<Value> {
    let schema = schemars::schema_for!(lpa_boards::BoardDisplayFile);
    serde_json::to_value(&schema).context("serializing board display schema")
}

/// File stem for a shape dump: the registry name with Rust path separators
/// flattened (`::` → `.`) so the checked-in file names stay portable.
fn shape_file_stem(name: &str) -> String {
    name.replace("::", ".")
}

/// Attach `$id`/`$schema` headers and render byte-stably.
fn render_schema(schema: Value, file_name: &str) -> Result<String> {
    let Value::Object(mut root) = schema else {
        bail!("schema for {file_name} is not a JSON object");
    };
    root.insert(
        String::from("$id"),
        json!(format!("{SCHEMA_ID_BASE}{file_name}")),
    );
    root.insert(String::from("$schema"), json!(SCHEMA_DIALECT));
    render(Value::Object(root))
}

/// Render a value byte-stably: explicitly sorted object keys, pretty-printed,
/// trailing newline.
fn render(value: Value) -> Result<String> {
    let mut text =
        serde_json::to_string_pretty(&sort_keys(value)).context("serializing generated output")?;
    text.push('\n');
    Ok(text)
}

/// Recursively rebuild every object with keys in sorted order. `serde_json`
/// objects are insertion-ordered by contract (BTree-backed only while the
/// `preserve_order` feature stays off workspace-wide), so ordering is enforced
/// here instead of assumed.
fn sort_keys(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map.into_iter().collect();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            let mut sorted = Map::new();
            for (key, value) in entries {
                sorted.insert(key, sort_keys(value));
            }
            Value::Object(sorted)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sort_keys).collect()),
        other => other,
    }
}

fn write_outputs(out_dir: &Path, outputs: &BTreeMap<String, String>) -> Result<()> {
    fs::create_dir_all(out_dir.join("shapes"))
        .with_context(|| format!("creating {}", out_dir.join("shapes").display()))?;
    let mut updated = 0usize;
    for (rel, contents) in outputs {
        let path = out_dir.join(rel);
        if fs::read_to_string(&path).ok().as_deref() != Some(contents.as_str()) {
            fs::write(&path, contents).with_context(|| format!("writing {}", path.display()))?;
            updated += 1;
        }
    }
    let stale = stale_files(out_dir, outputs)?;
    for path in &stale {
        fs::remove_file(path).with_context(|| format!("removing stale {}", path.display()))?;
        println!("schema gen: removed stale {}", path.display());
    }
    println!(
        "schema gen: {} files in {} ({updated} updated, {} stale removed)",
        outputs.len(),
        out_dir.display(),
        stale.len(),
    );
    Ok(())
}

fn check_outputs(out_dir: &Path, outputs: &BTreeMap<String, String>) -> Result<()> {
    let mut drift = Vec::new();
    for (rel, expected) in outputs {
        match fs::read_to_string(out_dir.join(rel)) {
            Ok(actual) if actual == *expected => {}
            Ok(_) => drift.push(format!("{rel}: differs from generated output")),
            Err(_) => drift.push(format!("{rel}: missing")),
        }
    }
    for (rel, _) in owned_existing_files(out_dir)? {
        if !outputs.contains_key(&rel) {
            drift.push(format!("{rel}: stale (not produced by this generator)"));
        }
    }
    if drift.is_empty() {
        println!(
            "schema check: {} files up to date in {}",
            outputs.len(),
            out_dir.display()
        );
        return Ok(());
    }
    for line in &drift {
        eprintln!("schema drift: {line}");
    }
    bail!(
        "{} schema file(s) out of date in {}; run `lp-cli schema gen` and commit the result",
        drift.len(),
        out_dir.display(),
    );
}

/// Existing files this command owns but did not generate this run.
fn stale_files(out_dir: &Path, outputs: &BTreeMap<String, String>) -> Result<Vec<PathBuf>> {
    Ok(owned_existing_files(out_dir)?
        .into_iter()
        .filter(|(rel, _)| !outputs.contains_key(rel))
        .map(|(_, path)| path)
        .collect())
}

/// Files on disk matching the patterns this command owns:
/// `<out>/*.schema.json` and `<out>/shapes/*.json`. Returned as
/// `(relative path, absolute path)`, sorted by relative path.
fn owned_existing_files(out_dir: &Path) -> Result<Vec<(String, PathBuf)>> {
    let mut files = Vec::new();
    for (dir, prefix, suffix) in [
        (out_dir.to_path_buf(), "", ".schema.json"),
        (out_dir.join("shapes"), "shapes/", ".json"),
    ] {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries {
            let entry = entry.with_context(|| format!("reading {}", dir.display()))?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if !name.ends_with(suffix) {
                continue;
            }
            files.push((format!("{prefix}{name}"), entry.path()));
        }
    }
    files.sort();
    Ok(files)
}

/// Find the repository root by searching upward from the current directory
/// (same heuristic as the hardware manifest store).
fn find_repo_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    for candidate in cwd.ancestors() {
        if candidate.join("Cargo.toml").exists() && candidate.join("lp-core/lpc-shared").exists() {
            return Ok(candidate.to_path_buf());
        }
    }
    bail!("could not find repository root from {}", cwd.display())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two consecutive generations must be byte-identical (same file set,
    /// same contents), or `--check` and the checked-in baseline are useless.
    #[test]
    fn generate_outputs_is_deterministic_across_runs() {
        let first = generate_outputs().unwrap();
        let second = generate_outputs().unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn node_schema_covers_every_node_kind() {
        let outputs = generate_outputs().unwrap();
        let node: Value = serde_json::from_str(&outputs["node.schema.json"]).unwrap();
        let kinds: Vec<&str> = node["oneOf"]
            .as_array()
            .expect("node schema oneOf")
            .iter()
            .map(|branch| {
                branch["properties"]["kind"]["const"]
                    .as_str()
                    .expect("kind const")
            })
            .collect();
        let expected = [
            "Module",
            "Button",
            "Clock",
            "Texture",
            "Shader",
            "ComputeShader",
            "Fluid",
            "Playlist",
            "ControlRadio",
            "Output",
            "Fixture",
        ];
        assert_eq!(kinds.len(), expected.len(), "kinds: {kinds:?}");
        for kind in expected {
            assert!(kinds.contains(&kind), "missing node kind {kind}: {kinds:?}");
        }
    }

    /// D2 (P4): the clock's `controls.*` fields are `SlotRole::Debug`
    /// (session-only), so `node.schema.json` must not advertise them as
    /// authorable — kills W2, where the schema previously invited a def
    /// author to write a value the loader now warns-and-ignores.
    #[test]
    fn node_schema_omits_clock_debug_controls() {
        let outputs = generate_outputs().unwrap();
        let node: Value = serde_json::from_str(&outputs["node.schema.json"]).unwrap();
        let clock_branch = node["oneOf"]
            .as_array()
            .expect("node schema oneOf")
            .iter()
            .find(|branch| branch["properties"]["kind"]["const"] == json!("Clock"))
            .expect("clock branch");

        let controls = &clock_branch["properties"]["controls"];
        let controls_properties = controls["properties"]
            .as_object()
            .expect("controls is a compiled object schema");
        assert!(
            controls_properties.is_empty(),
            "clock controls.* are all Debug-role and must be omitted: {controls}"
        );
    }

    #[test]
    fn project_schema_is_a_closed_container_manifest() {
        let outputs = generate_outputs().unwrap();
        let project: Value = serde_json::from_str(&outputs["project.schema.json"]).unwrap();
        assert_eq!(
            project["properties"]["format"]["const"],
            json!(PROJECT_FORMAT_VERSION)
        );
        assert_eq!(project["required"], json!(["format"]));
        assert_eq!(project["additionalProperties"], json!(false));
        assert!(
            project["properties"]["kind"].is_null(),
            "not a node envelope"
        );
        assert_eq!(
            project["$id"],
            json!(format!("{SCHEMA_ID_BASE}project.schema.json"))
        );
    }

    /// P02: `target` validates as an optional string, and stays an unknown
    /// field like any other stray key (the container schema's whole point
    /// is that nothing outside its declared vocabulary is accepted).
    #[test]
    fn project_schema_validates_advisory_target() {
        let outputs = generate_outputs().unwrap();
        let project: Value = serde_json::from_str(&outputs["project.schema.json"]).unwrap();
        let validator = jsonschema::draft202012::new(&project).expect("compiles");

        let targeted = json!({
            "format": PROJECT_FORMAT_VERSION,
            "name": "Porch sign",
            "target": "espressif/esp32-c6-devkitc-1",
        });
        assert!(
            validator.is_valid(&targeted),
            "a targeted project must validate: {:?}",
            validator.iter_errors(&targeted).collect::<Vec<_>>()
        );

        let untargeted = json!({ "format": PROJECT_FORMAT_VERSION });
        assert!(
            validator.is_valid(&untargeted),
            "target stays optional — an untargeted project must still validate"
        );

        let wrong_typed = json!({
            "format": PROJECT_FORMAT_VERSION,
            "target": 6,
        });
        assert!(
            !validator.is_valid(&wrong_typed),
            "target must be a string, not a number"
        );

        let misspelled = json!({
            "format": PROJECT_FORMAT_VERSION,
            "targt": "espressif/esp32-c6-devkitc-1",
        });
        assert!(
            !validator.is_valid(&misspelled),
            "the container stays closed-vocabulary — a stray key is still refused"
        );
    }

    #[test]
    fn module_schema_pins_kind() {
        let outputs = generate_outputs().unwrap();
        let module: Value = serde_json::from_str(&outputs["module.schema.json"]).unwrap();
        assert_eq!(module["properties"]["kind"]["const"], json!("Module"));
        assert_eq!(module["required"], json!(["kind"]));
        assert!(
            module["properties"]["format"].is_null(),
            "format is a container concern"
        );
        assert_eq!(
            module["$id"],
            json!(format!("{SCHEMA_ID_BASE}module.schema.json"))
        );
    }

    #[test]
    fn shape_index_lists_every_static_shape() {
        let outputs = generate_outputs().unwrap();
        let index: Value = serde_json::from_str(&outputs["shapes/_index.json"]).unwrap();
        let index = index.as_object().expect("index object");
        let ids = lpc_model::slot_shapes::static_slot_shape_ids();
        assert_eq!(index.len(), ids.len());
        for id in ids {
            let name = lpc_model::slot_shapes::static_slot_shape_name(*id).expect("shape name");
            assert_eq!(index[name], json!(id.raw()), "index entry for {name}");
            assert!(
                outputs.contains_key(&format!("shapes/{}.json", shape_file_stem(name))),
                "missing shape dump for {name}"
            );
        }
    }

    #[test]
    fn outputs_are_pretty_printed_with_trailing_newline() {
        for (rel, contents) in generate_outputs().unwrap() {
            assert!(contents.ends_with('\n'), "{rel} missing trailing newline");
            assert!(
                !contents.ends_with("\n\n"),
                "{rel} has extra trailing newline"
            );
            let value: Value = serde_json::from_str(&contents).expect(&rel);
            let mut rendered = serde_json::to_string_pretty(&sort_keys(value)).unwrap();
            rendered.push('\n');
            assert_eq!(contents, rendered, "{rel} is not sorted+pretty-printed");
        }
    }
}
