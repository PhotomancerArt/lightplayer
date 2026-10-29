//! A repo project's fixtures and shaders, read from its files: what the
//! frozen sampling copy needs, as plain numbers.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use lpa_upgrade::v12::{
    V12ConsumerPolicy, V12Coords, V12Fixture, V12Projection, V12ProjectionShape, V12Sampling,
    V12Source, V12SourceSpace,
};
use serde_json::Value;

/// Every node of one project the report reads.
pub struct ProjectTree {
    pub dir: PathBuf,
    /// The project's path from the workspace root (`catalog/projects/…`).
    pub name: String,
    pub fixtures: Vec<FixtureNode>,
    pub shaders: Vec<ShaderNode>,
    /// Each playlist's definition file and its entries' keys, with the
    /// entry it starts on.
    pub playlists: Vec<PlaylistNode>,
    /// Definition files of Fluid nodes (a source the frozen copy does not
    /// answer: the theory gives it an extent, P4).
    pub fluids: Vec<String>,
}

pub struct FixtureNode {
    /// The definition file, from the project root (`fixture.json`).
    pub file: String,
    pub render_size: [u32; 2],
    pub sampling: V12Sampling,
    pub strip_order_meaningful: bool,
    pub wire_reversed: bool,
    /// The fixture authors a `consume` policy (none in the repo does).
    pub consume: Option<Value>,
    /// `diagnostic_mode`, when it is not `off`: the lamps show a test
    /// pattern and sample nothing.
    pub diagnostic: Option<String>,
    /// Its map2d, read: positions in render order, segment counts, the box.
    pub lamps: Result<FixtureLamps, String>,
}

pub struct FixtureLamps {
    pub positions: Vec<[f32; 2]>,
    pub segment_counts: Vec<u32>,
    pub view_box: [f32; 4],
    /// The map2d file, from the project root.
    pub map2d: String,
}

pub struct ShaderNode {
    pub file: String,
    pub glsl_file: String,
    pub glsl: String,
    pub source: V12Source,
}

pub struct PlaylistNode {
    pub file: String,
    pub idle_entry: Option<u64>,
    pub entries: Vec<String>,
}

impl FixtureNode {
    /// The frozen copy's view of this fixture, when its map2d read.
    pub fn v12(&self) -> Option<V12Fixture<'_>> {
        let lamps = self.lamps.as_ref().ok()?;
        Some(V12Fixture {
            positions: &lamps.positions,
            segment_counts: &lamps.segment_counts,
            view_box: lamps.view_box,
            render_size: self.render_size,
            sampling: self.sampling,
            strip_order_meaningful: self.strip_order_meaningful,
            wire_reversed: self.wire_reversed,
            policy: V12ConsumerPolicy::default(),
        })
    }
}

impl ProjectTree {
    /// Read the project at `dir` (its `module.json` tree).
    pub fn read(workspace: &Path, dir: &Path) -> Self {
        let mut tree = Self {
            dir: dir.to_path_buf(),
            name: dir
                .strip_prefix(workspace)
                .unwrap_or(dir)
                .to_string_lossy()
                .into_owned(),
            fixtures: Vec::new(),
            shaders: Vec::new(),
            playlists: Vec::new(),
            fluids: Vec::new(),
        };
        tree.visit_file(&dir.join("module.json"));
        tree
    }

    /// The shader nodes whose GLSL is `glsl`, compiled as `space`.
    pub fn shaders_with_source(&self, glsl: &str) -> Vec<&ShaderNode> {
        self.shaders.iter().filter(|s| s.glsl == glsl).collect()
    }

    fn relative(&self, path: &Path) -> String {
        let clean: PathBuf = path
            .components()
            .filter(|c| !matches!(c, std::path::Component::CurDir))
            .collect();
        clean
            .strip_prefix(&self.dir)
            .unwrap_or(&clean)
            .to_string_lossy()
            .into_owned()
    }

    fn visit_file(&mut self, file: &Path) {
        let Ok(text) = std::fs::read_to_string(file) else {
            return;
        };
        let Ok(json) = serde_json::from_str::<Value>(&text) else {
            return;
        };
        let dir = file.parent().unwrap_or(Path::new(".")).to_path_buf();
        self.visit_node(&json, file, &dir);
    }

    fn visit_ref(&mut self, value: &Value, dir: &Path) {
        if let Some(reference) = value.get("ref").and_then(Value::as_str) {
            self.visit_file(&dir.join(reference));
        }
    }

    fn visit_node(&mut self, json: &Value, file: &Path, dir: &Path) {
        match json.get("kind").and_then(Value::as_str) {
            Some("Module") => {
                if let Some(nodes) = json.get("nodes").and_then(Value::as_object) {
                    for child in nodes.values() {
                        self.visit_ref(child, dir);
                    }
                }
            }
            Some("Playlist") => {
                let mut entries = Vec::new();
                if let Some(map) = json.get("entries").and_then(Value::as_object) {
                    for (key, entry) in map {
                        entries.push(key.clone());
                        if let Some(node) = entry.get("node") {
                            self.visit_ref(node, dir);
                        }
                    }
                }
                let file = self.relative(file);
                if self.playlists.iter().any(|p| p.file == file) {
                    return;
                }
                self.playlists.push(PlaylistNode {
                    file,
                    idle_entry: json.get("idle_entry").and_then(Value::as_u64),
                    entries,
                });
            }
            Some("Fixture") => {
                let fixture = self.fixture(json, file, dir);
                self.fixtures.push(fixture);
            }
            Some("Shader") => {
                if let Some(shader) = self.shader(json, file, dir) {
                    self.shaders.push(shader);
                }
            }
            Some("Fluid") => self.fluids.push(self.relative(file)),
            _ => {}
        }
    }

    fn fixture(&self, json: &Value, file: &Path, dir: &Path) -> FixtureNode {
        let size = json.get("render_size");
        let dim = |key: &str, default: u32| {
            size.and_then(|s| s.get(key))
                .and_then(Value::as_u64)
                .map_or(default, |v| v as u32)
        };
        let sampling = match json.get("sampling").and_then(Value::as_str) {
            Some("direct") => V12Sampling::Direct,
            // The model's default (`FixtureSamplingConfig::default`).
            _ => V12Sampling::TextureArea,
        };
        let flag =
            |key: &str, default: bool| json.get(key).and_then(Value::as_bool).unwrap_or(default);
        let lamps = json
            .get("mapping")
            .and_then(|m| m.get("source"))
            .and_then(Value::as_str)
            .ok_or_else(|| "it has no map2d".to_string())
            .and_then(|source| self.lamps(&dir.join(source)));
        FixtureNode {
            file: self.relative(file),
            render_size: [dim("width", 16), dim("height", 16)],
            sampling,
            strip_order_meaningful: flag("strip_order_meaningful", true),
            wire_reversed: flag("wire_reversed", false),
            consume: json.get("consume").cloned(),
            diagnostic: json
                .get("diagnostic_mode")
                .and_then(Value::as_str)
                .filter(|mode| *mode != "off")
                .map(str::to_string),
            lamps,
        }
    }

    fn lamps(&self, map2d: &Path) -> Result<FixtureLamps, String> {
        let text =
            std::fs::read_to_string(map2d).map_err(|e| format!("read {}: {e}", map2d.display()))?;
        let mapping = lpc_mapping::map2d6::read(&text).map_err(|e| format!("{e}"))?;
        let tree = lpc_mapping::part::render(&mapping).map_err(|e| format!("{e}"))?;
        Ok(FixtureLamps {
            positions: tree.positions().to_vec(),
            segment_counts: tree.segments().iter().map(|s| s.count()).collect(),
            view_box: mapping.view_box,
            map2d: self.relative(map2d),
        })
    }

    fn shader(&self, json: &Value, file: &Path, dir: &Path) -> Option<ShaderNode> {
        let glsl_path = dir.join(json.get("source")?.as_str()?);
        let glsl = std::fs::read_to_string(&glsl_path).ok()?;
        let coords = match json.get("coords").and_then(Value::as_str) {
            Some("pattern") => V12Coords::Pattern,
            _ => V12Coords::Pixels,
        };
        let space = match json.get("space") {
            Some(space) if space.get("kind").and_then(Value::as_str) == Some("OneD") => {
                V12SourceSpace::OneD {
                    in_2d: projection(space.get("in_2d")),
                }
            }
            _ => V12SourceSpace::TwoD,
        };
        Some(ShaderNode {
            file: self.relative(file),
            glsl_file: self.relative(&glsl_path),
            glsl,
            source: V12Source { coords, space },
        })
    }
}

/// A `space.OneD.in_2d` record as the frozen projection; anything that does
/// not resolve reads as plain extrude-x, as the engine reads it.
fn projection(in_2d: Option<&Value>) -> V12Projection {
    let variant = |key: &str| {
        in_2d
            .and_then(|v| v.get(key))
            .and_then(|v| v.get("kind"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    V12Projection {
        shape: match variant("shape").as_str() {
            "ExtrudeY" => V12ProjectionShape::ExtrudeY,
            "Radial" => V12ProjectionShape::Radial,
            "Angular" => V12ProjectionShape::Angular,
            _ => V12ProjectionShape::ExtrudeX,
        },
        mirror: variant("mirror") == "Mirrored",
        flip: variant("flip") == "Flipped",
    }
}

/// Every project in the repo (`catalog/`, `projects/`), by path.
pub fn repo_projects(workspace: &Path) -> Vec<PathBuf> {
    let mut out = BTreeMap::new();
    for root in ["catalog", "projects"] {
        collect(&workspace.join(root), &mut out);
    }
    out.into_values().collect()
}

fn collect(dir: &Path, out: &mut BTreeMap<String, PathBuf>) {
    if dir.join("project.json").is_file() && dir.join("module.json").is_file() {
        out.insert(dir.to_string_lossy().into_owned(), dir.to_path_buf());
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.path().is_dir() {
            collect(&entry.path(), out);
        }
    }
}

/// The workspace root.
pub fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}
