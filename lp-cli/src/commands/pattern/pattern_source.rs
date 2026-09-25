//! A pattern project directory, read into what a preview needs: the
//! exported module files verbatim, the rig pieces the swatch rig reuses
//! (clock, fixture render settings), and the review metadata (name,
//! description, family, idea source, provenance, knobs).
//!
//! Everything is read as raw JSON on purpose: the preview must not care
//! which optional fields a pattern's defs carry (a new opt-in coordinate
//! rule, say) — it copies them through and lets the engine decide.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value, json};

/// One pattern project, read from disk.
#[derive(Debug, Clone)]
pub struct PatternSource {
    /// Directory name, e.g. `plasma`.
    pub slug: String,
    /// `project.json`, verbatim.
    pub manifest: Value,
    /// The folders the manifest exports (`["effect"]` for every catalog
    /// pattern today).
    pub exports: Vec<String>,
    /// Every file under the exported folders: `(path relative to the
    /// pattern root, bytes)`, `/`-separated.
    pub export_files: Vec<(String, Vec<u8>)>,
    /// The root module's first `Clock` node def, if it has one.
    pub clock: Option<Value>,
    /// The root module's first `Fixture` node def, if it has one.
    pub fixture: Option<Value>,
    /// The root `module.json`'s `provenance` block (or `null`).
    pub provenance: Value,
}

impl PatternSource {
    /// Read `dir`, which must hold a `project.json` of `kind: pattern`.
    pub fn read(dir: &Path) -> Result<Self> {
        let slug = dir
            .canonicalize()
            .unwrap_or_else(|_| dir.to_path_buf())
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string)
            .with_context(|| format!("{}: no directory name", dir.display()))?;
        let manifest = read_json(&dir.join("project.json"))?;
        if manifest.get("kind").and_then(Value::as_str) != Some("pattern") {
            bail!(
                "{}: project.json is not `kind: pattern` — preview renders pattern projects",
                dir.display()
            );
        }
        let exports: Vec<String> = manifest
            .get("exports")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        if exports.is_empty() {
            bail!("{}: project.json exports nothing", dir.display());
        }

        let mut export_files = Vec::new();
        for export in &exports {
            let root = dir.join(export);
            if !root.join("module.json").is_file() {
                bail!(
                    "{}: exports `{export}` but has no {export}/module.json",
                    dir.display()
                );
            }
            collect_files(dir, &root, &mut export_files)?;
        }
        export_files.sort_by(|a, b| a.0.cmp(&b.0));

        let root_module = read_json(&dir.join("module.json"))?;
        let provenance = root_module
            .get("provenance")
            .cloned()
            .unwrap_or(Value::Null);
        let mut clock = None;
        let mut fixture = None;
        if let Some(nodes) = root_module.get("nodes").and_then(Value::as_object) {
            for node in nodes.values() {
                let Some(reference) = node.get("ref").and_then(Value::as_str) else {
                    continue;
                };
                let path = dir.join(reference.trim_start_matches("./"));
                if !path.is_file() {
                    continue;
                }
                let def = read_json(&path)?;
                match def.get("kind").and_then(Value::as_str) {
                    Some("Clock") if clock.is_none() => clock = Some(def),
                    Some("Fixture") if fixture.is_none() => fixture = Some(def),
                    _ => {}
                }
            }
        }

        Ok(Self {
            slug,
            manifest,
            exports,
            export_files,
            clock,
            fixture,
            provenance,
        })
    }

    /// The exported files that parse as JSON node defs, with their paths.
    pub fn export_json_defs(&self) -> Vec<(String, Value)> {
        self.export_files
            .iter()
            .filter(|(path, _)| path.ends_with(".json"))
            .filter_map(|(path, bytes)| {
                serde_json::from_slice::<Value>(bytes)
                    .ok()
                    .map(|value| (path.clone(), value))
            })
            .collect()
    }

    /// The float mode the exported shaders compile in: `Q32` when none pins
    /// `float_mode` (the CPU backends' native mode), else the pins, listed.
    pub fn float_mode(&self) -> String {
        let pins: Vec<String> = self
            .export_json_defs()
            .into_iter()
            .filter_map(|(path, def)| {
                def.get("float_mode")
                    .map(|mode| format!("{path}: {}", mode.as_str().unwrap_or("?")))
            })
            .collect();
        if pins.is_empty() {
            "Q32 (no shader pins float_mode)".to_string()
        } else {
            pins.join(", ")
        }
    }

    /// The review metadata the page shows under each row.
    pub fn review_metadata(&self) -> Value {
        let name = self
            .manifest
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(&self.slug)
            .to_string();
        let description = self
            .manifest
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        // Export module provenance (the one import carries) over the root's.
        let mut provenance = Map::new();
        if let Value::Object(root) = &self.provenance {
            provenance.extend(root.clone());
        }
        for (path, def) in self.export_json_defs() {
            if path.ends_with("/module.json")
                && let Some(Value::Object(block)) = def.get("provenance")
            {
                provenance.extend(block.clone());
            }
        }

        let glsl: Vec<String> = self
            .export_files
            .iter()
            .filter(|(path, _)| path.ends_with(".glsl"))
            .map(|(_, bytes)| String::from_utf8_lossy(bytes).into_owned())
            .collect();
        let lookup = |key: &str| -> Option<String> {
            self.manifest
                .get(key)
                .and_then(Value::as_str)
                .or_else(|| provenance.get(key).and_then(Value::as_str))
                .map(str::to_string)
                .or_else(|| glsl.iter().find_map(|source| comment_field(source, key)))
        };
        let family = lookup("family");
        let idea = lookup("idea").or_else(|| {
            // A port names its upstream in the provenance author.
            provenance
                .get("author")
                .and_then(Value::as_str)
                .filter(|author| *author != "Photomancer")
                .map(str::to_string)
        });

        json!({
            "slug": self.slug,
            "name": name,
            "description": description,
            "family": family,
            "idea": idea,
            "provenance": Value::Object(provenance),
            "knobs": self.knobs(),
        })
    }

    /// Every consumed slot of every exported shader def, in file order.
    ///
    /// A slot is a *knob* in Studio when it has an authored binding to a
    /// bus channel (ADR 2026-08-03-panel-visibility-is-derived); `binding`
    /// carries that source so the page can tell.
    pub fn knobs(&self) -> Vec<Value> {
        let mut knobs = Vec::new();
        for (path, def) in self.export_json_defs() {
            let Some(consumed) = def.get("consumed").and_then(Value::as_object) else {
                continue;
            };
            let node = path.trim_end_matches(".json").to_string();
            for (slot, spec) in consumed {
                let kind = spec.get("kind").and_then(Value::as_str).unwrap_or("");
                let binding = def
                    .get("bindings")
                    .and_then(|bindings| bindings.get(slot))
                    .and_then(|binding| binding.get("source"))
                    .and_then(Value::as_str);
                let mut knob = json!({
                    "node": node,
                    "slot": slot,
                    "kind": kind,
                    "label": spec.get("label").and_then(Value::as_str).unwrap_or(""),
                    "description": spec.get("description").and_then(Value::as_str).unwrap_or(""),
                    "default": spec.get("default").cloned().unwrap_or(Value::Null),
                    "min": spec.get("min").cloned().unwrap_or(Value::Null),
                    "max": spec.get("max").cloned().unwrap_or(Value::Null),
                    "binding": binding,
                });
                if let Some(phasor) = spec.get("phasor") {
                    knob["period_seconds"] =
                        phasor.get("period_seconds").cloned().unwrap_or(Value::Null);
                    knob["waveform"] = phasor.get("waveform").cloned().unwrap_or(Value::Null);
                }
                if let Some(stops) = spec
                    .get("gradient")
                    .and_then(|gradient| gradient.get("set"))
                    .and_then(Value::as_array)
                    .and_then(|set| set.first())
                    .and_then(|first| first.get("stops"))
                {
                    knob["stops"] = stops.clone();
                }
                knobs.push(knob);
            }
        }
        knobs
    }
}

/// `// <key>: value` in a GLSL comment line, case-insensitive key.
fn comment_field(source: &str, key: &str) -> Option<String> {
    source.lines().find_map(|line| {
        let body = line.trim_start().strip_prefix("//")?.trim_start();
        let (field, value) = body.split_once(':')?;
        (field.trim().eq_ignore_ascii_case(key) && !value.trim().is_empty())
            .then(|| value.trim().to_string())
    })
}

fn read_json(path: &Path) -> Result<Value> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))
}

fn collect_files(base: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) -> Result<()> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .with_context(|| format!("list {}", dir.display()))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<_, _>>()?;
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_files(base, &path, out)?;
        } else {
            let relative = path
                .strip_prefix(base)
                .expect("walked under base")
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
            out.push((relative, bytes));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comment_field_reads_case_insensitive_keys() {
        let source = "// Header\n//   Idea: WLED twinkle, expanded to 2D\nfloat x;";
        assert_eq!(
            comment_field(source, "idea").as_deref(),
            Some("WLED twinkle, expanded to 2D")
        );
        assert_eq!(comment_field(source, "family"), None);
    }
}
