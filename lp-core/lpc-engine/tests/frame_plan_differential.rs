//! The byte differential: a cached engine renders what an uncached one does,
//! through every kind of edit, on every project the host can load.
//!
//! The frame-plan work (planning `lp2025/2026-10-24-0208-engine-frame-plan`)
//! makes nodes and the engine keep, across frames, things they re-derive every
//! frame today. Each kept thing is a cache that can go stale, and a stale
//! cache fails silently: the frame still renders, from yesterday's answer.
//! This test is the oracle that catches it without anyone first imagining how.
//!
//! Every corpus project runs one scripted session — steady frames, then one
//! edit of each kind the engine knows (a slot value, a binding, a node removed
//! and restored, a mapping, a patch, a shader body, a playlist pick, panel
//! writes and their clears and expiry, memory pressure) — in several
//! [`Mode`]s side by side, and every output's published bytes must agree
//! frame for frame with [`Mode::Cached`]. After every file edit the cached
//! engine must also agree with a **fresh load** of the edited files, ticked
//! to the same frame: an edit applied in place is not allowed to leave
//! anything behind that a load would not build.
//!
//! A mode is one enum value; adding one is adding a variant and its
//! [`Mode::configure`] arm.
//!
//! ```bash
//! cargo test -p lpc-engine --test frame_plan_differential -- --nocapture
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lpc_engine::engine::LoadedProjectRuntime;
use lpc_engine::frame_plan::PlanMode;
use lpc_engine::node::PressureLevel;
use lpc_engine::{EngineServices, ProjectLoader};
use lpc_model::{ChannelName, LpValue, NodeUseLocation, SlotPath, TreePath, current_revision};
use lpc_registry::ParseCtx;
use lpfs::{FsEvent, FsEventKind, LpFs, LpFsMemory, LpPath, LpPathBuf};
use serde_json::Value;

/// Milliseconds per tick: fixed, so animated projects are reproducible and a
/// fresh load ticked the same number of frames reaches the same time.
const DELTA_MS: u32 = 16;
/// Frames before the first edit (the first frame compiles; later ones are
/// steady).
const STEADY_FRAMES: usize = 4;
/// Frames after each edit: the edit's own frame, then steady ones.
const SETTLE_FRAMES: usize = 3;

/// Projects the host cannot run, and why. Everything else under the corpus
/// roots must load, or the test fails.
const SKIP: &[(&str, &str)] = &[
    (
        "fyeah-sign",
        "its control radio needs a radio service this host does not provide: every frame is the fault pattern",
    ),
    (
        "fyeah-button",
        "its control radio needs a radio service this host does not provide: every frame is the fault pattern",
    ),
    (
        "button-playlist",
        "its button needs a button service this host does not provide: every frame is the fault pattern",
    ),
    (
        "fault-demo",
        "a deliberate fault: every frame is the fault pattern (tests/fault_demo_paints_the_pattern.rs covers it)",
    ),
];

/// The comparison modes. [`Mode::Cached`] is the reference every other mode
/// is compared against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// The product's configuration: the resolver keeps resolution across
    /// frames until a structural edit, and frame plans are replayed.
    Cached,
    /// The resolver forgets every route, value and intern at the top of every
    /// frame (`Resolver::set_force_invalidate_per_frame`).
    ForcedInvalidate,
    /// Every frame plan is rebuilt every frame, as if every key moved
    /// (`PlanMode::RebuildEveryFrame`).
    RebuildEveryFrame,
    /// Plans replayed and also rebuilt and compared every frame, panicking
    /// on a difference (`PlanMode::strict_check`): the checker's own verdict.
    Checked,
}

const MODES: &[Mode] = &[
    Mode::Cached,
    Mode::ForcedInvalidate,
    Mode::RebuildEveryFrame,
    Mode::Checked,
];

impl Mode {
    fn configure(self, rt: &mut LoadedProjectRuntime) {
        let engine = rt.engine_mut();
        match self {
            Mode::Cached => engine.set_plan_mode(PlanMode::Replay),
            Mode::ForcedInvalidate => {
                engine.set_plan_mode(PlanMode::Replay);
                engine.resolver_mut().set_force_invalidate_per_frame(true);
            }
            Mode::RebuildEveryFrame => engine.set_plan_mode(PlanMode::RebuildEveryFrame),
            Mode::Checked => engine.set_plan_mode(PlanMode::strict_check()),
        }
    }
}

#[test]
fn every_mode_renders_the_same_bytes_through_every_edit() {
    let projects = corpus();
    assert!(
        projects.len() > 40,
        "corpus walk found {} projects",
        projects.len()
    );
    let mut failures = Vec::new();
    let mut report = Vec::new();
    for (name, dir) in &projects {
        if let Some((_, why)) = SKIP.iter().find(|(skip, _)| skip == name) {
            report.push(format!("{name}: SKIPPED ({why})"));
            continue;
        }
        let outcome = run_project(name, dir);
        report.push(format!("{name}: {}", outcome.steps.join(", ")));
        failures.extend(outcome.failures);
    }
    println!("{}", report.join("\n"));
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

struct Outcome {
    /// The steps that applied to this project, in order.
    steps: Vec<String>,
    failures: Vec<String>,
}

/// One engine of the session, in one mode.
struct Lane {
    mode: Mode,
    rt: LoadedProjectRuntime,
}

/// Every mode's engine over one shared, editable copy of the project.
struct Session {
    name: String,
    fs: LpFsMemory,
    lanes: Vec<Lane>,
    frames: usize,
    /// Whether a fresh load ticked to the same frame is expected to agree
    /// (`None`), or why not.
    fresh_skip: Option<&'static str>,
    steps: Vec<String>,
    failures: Vec<String>,
    /// Every distinct frame the reference lane published, so a session that
    /// renders nothing cannot pass by agreeing with itself.
    distinct: Vec<Vec<(String, Vec<u8>)>>,
}

fn run_project(name: &str, dir: &Path) -> Outcome {
    let fs = copy_to_memory(dir);
    let kinds = node_kinds(&fs);
    let fresh_skip = fresh_load_skip(&kinds);
    let lanes = MODES
        .iter()
        .map(|&mode| {
            let mut rt = load(name, &fs);
            mode.configure(&mut rt);
            Lane { mode, rt }
        })
        .collect();
    let mut s = Session {
        name: name.to_string(),
        fs,
        lanes,
        frames: 0,
        fresh_skip,
        steps: Vec::new(),
        failures: Vec::new(),
        distinct: Vec::new(),
    };
    s.step("steady", STEADY_FRAMES);
    edit_fixture_value(&mut s);
    edit_output_name(&mut s);
    edit_rewire(&mut s);
    edit_remove_and_restore_node(&mut s);
    edit_mapping(&mut s);
    edit_patch(&mut s);
    edit_shader_body(&mut s);
    pick_playlist_entry(&mut s);
    panel_writes(&mut s);
    memory_pressure(&mut s);
    if s.distinct.len() < 2 {
        s.failures.push(format!(
            "{name}: the reference published {} distinct frame(s) through the whole script; \
             the differential proves nothing on a project that never changes",
            s.distinct.len()
        ));
    }
    Outcome {
        steps: s.steps,
        failures: s.failures,
    }
}

impl Session {
    /// Tick every lane `frames` times, comparing each lane's published bytes
    /// with the reference lane after every frame.
    fn step(&mut self, label: &str, frames: usize) {
        self.steps.push(label.to_string());
        for frame_in_step in 0..frames {
            self.frames += 1;
            // A node error is a tick result like any other (a radio node with
            // no radio service, an output whose producer was removed): the
            // modes must agree on it, not avoid it.
            let results: Vec<String> = self
                .lanes
                .iter_mut()
                .map(
                    |lane| match lane.rt.tick_with_residency(&self.fs, DELTA_MS) {
                        Ok(()) => String::from("ok"),
                        Err(e) => format!("{e:?}"),
                    },
                )
                .collect();
            // The last frame of every step is a steady one: it must build no
            // plan and re-derive no structure (AC2).
            if frame_in_step + 1 == frames && frames > 1 {
                let engine = self.lanes[0].rt.engine();
                let plans = *engine.frame_plan_counters();
                let resolution = *engine.resolver().frame_counters();
                if plans.structural_work() != 0 || resolution.structural_work() != 0 {
                    self.failures.push(format!(
                        "{} [{label}] frame {}: a steady frame did structural work: {plans:?}, {resolution:?}",
                        self.name, self.frames
                    ));
                }
            }
            let reference = published_outputs(&self.lanes[0].rt);
            if !self.distinct.contains(&reference) {
                self.distinct.push(reference.clone());
            }
            for (lane, result) in self.lanes.iter().zip(&results).skip(1) {
                if result != &results[0] {
                    self.failures.push(format!(
                        "{} [{label}] frame {}: {:?} ticked {result}, {:?} ticked {}",
                        self.name, self.frames, lane.mode, self.lanes[0].mode, results[0]
                    ));
                }
                if let Some(diff) = first_difference(&reference, &published_outputs(&lane.rt)) {
                    self.failures.push(format!(
                        "{} [{label}] frame {}: {:?} differs from {:?}: {diff}",
                        self.name, self.frames, lane.mode, self.lanes[0].mode
                    ));
                }
            }
        }
    }

    /// Write `bytes` to `path`, apply it to every lane, tick, and compare the
    /// cached lane with a fresh load of the edited files.
    fn file_edit(&mut self, label: &str, path: &str, bytes: &[u8]) {
        self.fs
            .write_file(LpPath::new(path), bytes)
            .unwrap_or_else(|e| panic!("{}: write {path}: {e:?}", self.name));
        for lane in &mut self.lanes {
            let (engine, registry) = lane.rt.split_mut();
            let shapes = engine.slot_shapes().clone();
            let changes = registry.refresh_artifacts(
                &self.fs,
                &[FsEvent {
                    path: LpPathBuf::from(path),
                    kind: FsEventKind::Modify,
                }],
                current_revision(),
                &ParseCtx { shapes: &shapes },
            );
            if let Err(e) = engine.apply_project_changes(&self.fs, registry, &changes) {
                self.failures.push(format!(
                    "{} [{label}] ({:?}): apply failed: {e:?}",
                    self.name, lane.mode
                ));
            }
        }
        self.step(label, SETTLE_FRAMES);
        self.compare_with_fresh_load(label);
    }

    fn compare_with_fresh_load(&mut self, label: &str) {
        if self.fresh_skip.is_some() {
            return;
        }
        let mut fresh = load(&self.name, &self.fs);
        for _ in 0..self.frames {
            // Node errors are compared through the bytes they leave.
            let _ = fresh.tick_with_residency(&self.fs, DELTA_MS);
        }
        // An output a fresh load never published (its input resolves to
        // nothing: rewired away, or its producer removed) is not compared:
        // the edited engine still holds the buffer it published before the
        // edit, and a load has none. Everything a load does publish must
        // agree.
        let fresh = published_outputs(&fresh);
        let unpublished: Vec<&String> = fresh
            .iter()
            .filter(|(_, bytes)| bytes.is_empty())
            .map(|(name, _)| name)
            .collect();
        let keep = |outputs: &[(String, Vec<u8>)]| -> Vec<(String, Vec<u8>)> {
            outputs
                .iter()
                .filter(|(name, _)| !unpublished.contains(&name))
                .cloned()
                .collect()
        };
        let edited = keep(&published_outputs(&self.lanes[0].rt));
        if let Some(diff) = first_difference(&keep(&fresh), &edited) {
            self.failures.push(format!(
                "{} [{label}] frame {}: the edited engine differs from a fresh load of the edit: {diff}",
                self.name, self.frames
            ));
        }
    }

    fn read_json(&self, path: &str) -> Value {
        let bytes = self
            .fs
            .read_file(LpPath::new(path))
            .unwrap_or_else(|e| panic!("{}: read {path}: {e:?}", self.name));
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|e| panic!("{}: parse {path}: {e}", self.name))
    }

    fn write_json(&mut self, label: &str, path: &str, value: &Value) {
        let mut text = String::new();
        write_json_kind_first(value, &mut text);
        self.file_edit(label, path, text.as_bytes());
    }
}

// --- The edits -------------------------------------------------------------

/// A slot value edit: the first fixture's brightness.
fn edit_fixture_value(s: &mut Session) {
    let Some(path) = first_file_of_kind(&s.fs, "Fixture") else {
        return;
    };
    let mut def = s.read_json(&path);
    let next = match def.get("brightness").and_then(Value::as_f64) {
        Some(b) if (b - 0.5).abs() < 1e-6 => 0.25,
        _ => 0.5,
    };
    def["brightness"] = Value::from(next);
    s.write_json("fixture brightness", &path, &def);
}

/// A slot value edit on the output: its display name (the identity the
/// engine registers and patches resolve against).
fn edit_output_name(s: &mut Session) {
    let Some(path) = first_file_of_kind(&s.fs, "Output") else {
        return;
    };
    let mut def = s.read_json(&path);
    def["name"] = Value::from("frame plan renamed");
    s.write_json("output name", &path, &def);
}

/// A binding change: the first output's input points nowhere, then back.
fn edit_rewire(s: &mut Session) {
    let Some(path) = first_file_of_kind(&s.fs, "Output") else {
        return;
    };
    let original = s.read_json(&path);
    if original.pointer("/bindings/input/source").is_none() {
        return;
    }
    let mut def = original.clone();
    def["bindings"]["input"]["source"] = Value::from("bus:frame.plan.nowhere");
    s.write_json("rewire away", &path, &def);
    s.write_json("rewire back", &path, &original);
}

/// A node removed from the root module, then added back.
fn edit_remove_and_restore_node(s: &mut Session) {
    let path = "/module.json";
    if !s.fs.file_exists(LpPath::new(path)).unwrap_or(false) {
        return;
    }
    let original = s.read_json(path);
    let Some(nodes) = original.get("nodes").and_then(Value::as_object) else {
        return;
    };
    // The last fixture by name if there is one (its output loses a lane),
    // else the last node of all.
    let victim = nodes
        .iter()
        .filter(|(_, use_)| node_use_kind(&s.fs, use_).as_deref() == Some("Fixture"))
        .map(|(name, _)| name.clone())
        .last()
        .or_else(|| nodes.keys().last().cloned());
    let Some(victim) = victim else {
        return;
    };
    if nodes.len() < 2 {
        return;
    }
    let mut edited = original.clone();
    edited["nodes"]
        .as_object_mut()
        .expect("nodes object")
        .remove(&victim);
    s.write_json("node remove", path, &edited);
    s.write_json("node add", path, &original);
}

/// A mapping change: the first map's sample diameter.
fn edit_mapping(s: &mut Session) {
    let Some(path) = files(&s.fs)
        .into_iter()
        .filter(|p| p.ends_with(".json"))
        .find(|p| {
            s.read_json(p)
                .get("sample_diameter")
                .and_then(Value::as_f64)
                .is_some()
        })
    else {
        return;
    };
    let mut map = s.read_json(&path);
    let d = map["sample_diameter"].as_f64().expect("number");
    map["sample_diameter"] = Value::from(d * 1.5);
    s.write_json("mapping", &path, &map);
}

/// A patch edit: the first patch file's first entry moves one channel.
fn edit_patch(s: &mut Session) {
    let Some(path) = files(&s.fs)
        .into_iter()
        .find(|p| p.ends_with(".patch.json"))
    else {
        return;
    };
    let mut patch = s.read_json(&path);
    let Some(entry) = patch
        .get_mut("entries")
        .and_then(Value::as_array_mut)
        .and_then(|entries| entries.first_mut())
    else {
        return;
    };
    if let Some(row) = entry.as_array_mut() {
        // Format 2: [path, output index, channel, …].
        let channel = row
            .get(2)
            .and_then(Value::as_u64)
            .expect("format 2 channel");
        row[2] = Value::from(channel + 1);
    } else if let Some(channel) = entry.pointer_mut("/at/channel") {
        // Format 1: { range, at: { channel } }.
        let next = channel.as_u64().expect("format 1 channel") + 1;
        *channel = Value::from(next);
    } else {
        return;
    }
    s.write_json("patch", &path, &patch);
}

/// A shader body edit (a recompile). A comment changes no pixel, so a fresh
/// load agrees; what it proves is that a recompile leaves nothing stale.
fn edit_shader_body(s: &mut Session) {
    let Some(path) = files(&s.fs).into_iter().find(|p| p.ends_with(".glsl")) else {
        return;
    };
    let mut source = s.fs.read_file(LpPath::new(&path)).expect("read glsl");
    source.extend_from_slice(b"\n// frame plan differential: recompiled\n");
    s.file_edit("shader body", &path, &source);
}

/// A playlist pick: the root module's first playlist activates its second
/// entry. Runtime choice, not structure.
fn pick_playlist_entry(s: &mut Session) {
    let Some((name, entry)) = root_nodes(&s.fs).into_iter().find_map(|(name, use_)| {
        let def = node_use_def(&s.fs, &use_)?;
        if def.get("kind").and_then(Value::as_str) != Some("Playlist") {
            return None;
        }
        let entry = def
            .get("entries")?
            .as_object()?
            .keys()
            .filter_map(|k| k.parse::<u32>().ok())
            .nth(1)?;
        Some((name, entry))
    }) else {
        return;
    };
    let location = NodeUseLocation::root()
        .child(SlotPath::parse(&format!("nodes[{name}]")).expect("node slot path"));
    for lane in &mut s.lanes {
        let engine = lane.rt.engine_mut();
        let id = engine
            .project_runtime_index()
            .node_id(&location)
            .unwrap_or_else(|| panic!("{}: playlist {name} projected", s.name));
        // A pick is a panel write on `playlist.entry` in the playlist's
        // read scope (playlists-on-the-bus D1).
        let Some(scope) = engine.tree().bus_read_scope(id) else {
            s.failures.push(format!(
                "{} [playlist pick] ({:?}): no read scope",
                s.name, lane.mode
            ));
            continue;
        };
        engine.panel_write(
            scope,
            lpc_model::ChannelName(String::from(lpc_model::PLAYLIST_ENTRY_CHANNEL)),
            lpc_model::LpValue::U32(entry),
            None,
        );
    }
    s.step("playlist pick", SETTLE_FRAMES);
}

/// Panel writes on the root scope's `time` channel: latched, cleared, then a
/// momentary write left to expire.
fn panel_writes(s: &mut Session) {
    let channel = ChannelName(String::from("time"));
    for lane in &mut s.lanes {
        let engine = lane.rt.engine_mut();
        let scope = engine
            .tree()
            .node_scope(engine.tree().root())
            .expect("root scope");
        engine.panel_write(scope, channel.clone(), LpValue::F32(1.5), None);
    }
    s.step("panel write", SETTLE_FRAMES);
    for lane in &mut s.lanes {
        let engine = lane.rt.engine_mut();
        let scope = engine
            .tree()
            .node_scope(engine.tree().root())
            .expect("root scope");
        engine.panel_clear(scope, &channel);
    }
    s.step("panel clear", SETTLE_FRAMES);
    for lane in &mut s.lanes {
        let engine = lane.rt.engine_mut();
        let scope = engine
            .tree()
            .node_scope(engine.tree().root())
            .expect("root scope");
        engine.panel_write(
            scope,
            channel.clone(),
            LpValue::F32(0.75),
            Some(DELTA_MS * 2),
        );
    }
    s.step("panel ttl expiry", SETTLE_FRAMES + 2);
}

/// Memory pressure between frames: every node drops what it can rebuild.
fn memory_pressure(s: &mut Session) {
    for lane in &mut s.lanes {
        if let Err(e) = lane
            .rt
            .engine_mut()
            .broadcast_memory_pressure(PressureLevel::High)
        {
            s.failures.push(format!(
                "{} [memory pressure] ({:?}): {e:?}",
                s.name, lane.mode
            ));
        }
    }
    s.step("memory pressure", SETTLE_FRAMES);
}

// --- Corpus and project files ---------------------------------------------

/// `(name, dir)` of every project under the corpus roots: the catalog's
/// patterns and projects, and the test rigs (one level of nesting, for
/// `projects/test/perf/*`).
fn corpus() -> Vec<(String, PathBuf)> {
    let workspace = workspace_dir();
    let mut out = Vec::new();
    for root in ["catalog/patterns", "catalog/projects", "projects/test"] {
        let base = workspace.join(root);
        for dir in sorted_dirs(&base) {
            if dir.join("project.json").is_file() {
                out.push((relative_name(&base, &dir), dir));
            } else {
                for sub in sorted_dirs(&dir) {
                    if sub.join("project.json").is_file() {
                        out.push((relative_name(&base, &sub), sub));
                    }
                }
            }
        }
    }
    out
}

fn sorted_dirs(dir: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .map(|entry| entry.expect("dir entry").path())
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();
    dirs
}

fn relative_name(base: &Path, dir: &Path) -> String {
    dir.strip_prefix(base)
        .expect("under base")
        .to_string_lossy()
        .into_owned()
}

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("lpc-engine lives two levels under the workspace root")
        .to_path_buf()
}

/// The project's files in an editable in-memory filesystem rooted at `/`.
fn copy_to_memory(dir: &Path) -> LpFsMemory {
    let fs = LpFsMemory::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        for entry in std::fs::read_dir(&next).expect("read project dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let relative = path.strip_prefix(dir).expect("under project dir");
            let lp_path = format!("/{}", relative.to_string_lossy());
            fs.write_file(
                LpPath::new(&lp_path),
                &std::fs::read(&path).expect("read file"),
            )
            .expect("write project file");
        }
    }
    fs
}

fn load(name: &str, fs: &LpFsMemory) -> LoadedProjectRuntime {
    let root = format!("/{}.show", name.replace(['/', '-'], "_"));
    let services = EngineServices::new(TreePath::parse(&root).expect("root path"));
    let mut rt = ProjectLoader::load_from_root(fs, services)
        .unwrap_or_else(|e| panic!("load {name}: {e:?}"));
    rt.engine_mut()
        .set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
    rt
}

/// Every file path in `fs`, sorted.
fn files(fs: &LpFsMemory) -> Vec<String> {
    let mut out: Vec<String> = fs
        .list_dir(LpPath::new("/"), true)
        .expect("list project files")
        .into_iter()
        .map(|p| p.as_str().to_string())
        .filter(|p| fs.read_file(LpPath::new(p)).is_ok())
        .collect();
    out.sort();
    out
}

fn first_file_of_kind(fs: &LpFsMemory, kind: &str) -> Option<String> {
    files(fs)
        .into_iter()
        .filter(|p| p.ends_with(".json"))
        .find(|p| {
            fs.read_file(LpPath::new(p))
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                .and_then(|v| v.get("kind").and_then(Value::as_str).map(|k| k == kind))
                .unwrap_or(false)
        })
}

/// Every `"kind"` an authored node file in the project declares.
fn node_kinds(fs: &LpFsMemory) -> Vec<String> {
    files(fs)
        .into_iter()
        .filter(|p| p.ends_with(".json"))
        .filter_map(|p| {
            let bytes = fs.read_file(LpPath::new(&p)).ok()?;
            let v: Value = serde_json::from_slice(&bytes).ok()?;
            v.get("kind").and_then(Value::as_str).map(str::to_string)
        })
        .collect()
}

/// Why a fresh load ticked to the same frame cannot be expected to match an
/// edited engine: state that integrates over the frames it has seen.
fn fresh_load_skip(kinds: &[String]) -> Option<&'static str> {
    let has = |kind: &str| kinds.iter().any(|k| k == kind);
    if has("Fluid") {
        Some("a fluid solver integrates its own history")
    } else if has("ComputeShader") {
        Some("a compute shader keeps state across frames")
    } else if has("Playlist") {
        Some("a playlist's entry timing depends on when it was loaded")
    } else {
        None
    }
}

fn root_nodes(fs: &LpFsMemory) -> Vec<(String, Value)> {
    fs.read_file(LpPath::new("/module.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|v| v.get("nodes").and_then(Value::as_object).cloned())
        .map(|nodes| nodes.into_iter().collect())
        .unwrap_or_default()
}

/// The def a root-module node use refers to (`{ "ref": "./x.json" }`), or
/// the inline def.
fn node_use_def(fs: &LpFsMemory, use_: &Value) -> Option<Value> {
    match use_.get("ref").and_then(Value::as_str) {
        Some(reference) => {
            let path = format!("/{}", reference.trim_start_matches("./"));
            let bytes = fs.read_file(LpPath::new(&path)).ok()?;
            serde_json::from_slice(&bytes).ok()
        }
        None => Some(use_.clone()),
    }
}

fn node_use_kind(fs: &LpFsMemory, use_: &Value) -> Option<String> {
    node_use_def(fs, use_)?
        .get("kind")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// JSON text with every object's `"kind"` first, as authored files have it
/// (the def readers key their shape off the kind they meet first).
fn write_json_kind_first(value: &Value, out: &mut String) {
    match value {
        Value::Object(map) => {
            out.push('{');
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by_key(|k| (k.as_str() != "kind", k.as_str()));
            for (i, key) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(key).expect("key"));
                out.push(':');
                write_json_kind_first(&map[key], out);
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_json_kind_first(item, out);
            }
            out.push(']');
        }
        other => out.push_str(&serde_json::to_string(other).expect("scalar")),
    }
}

// --- Published bytes -------------------------------------------------------

/// `(node path, published bytes)` for every output node, in tree order.
fn published_outputs(rt: &LoadedProjectRuntime) -> Vec<(String, Vec<u8>)> {
    let engine = rt.engine();
    let mut out = Vec::new();
    for entry in engine.tree().entries() {
        let Some(buffer_id) = engine.runtime_output_sink_buffer_id(entry.id) else {
            continue;
        };
        let Some(buffer) = engine.runtime_buffers().get(buffer_id) else {
            continue;
        };
        out.push((entry.path.to_string(), buffer.value().bytes().to_vec()));
    }
    out.sort();
    out
}

fn first_difference(a: &[(String, Vec<u8>)], b: &[(String, Vec<u8>)]) -> Option<String> {
    let names = |v: &[(String, Vec<u8>)]| v.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>();
    if names(a) != names(b) {
        return Some(format!(
            "output sets differ: {:?} vs {:?}",
            names(a),
            names(b)
        ));
    }
    for ((name, x), (_, y)) in a.iter().zip(b) {
        if x.len() != y.len() {
            return Some(format!("{name}: {} B vs {} B", x.len(), y.len()));
        }
        if let Some(i) = x.iter().zip(y).position(|(p, q)| p != q) {
            return Some(format!(
                "{name}: first difference at byte {i} ({} vs {})",
                x[i], y[i]
            ));
        }
    }
    None
}
