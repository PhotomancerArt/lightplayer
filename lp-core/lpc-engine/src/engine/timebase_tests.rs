//! Engine-level obligations for the timebase store (TimeProduct M2 / P2).
//!
//! The store's own arithmetic is pinned in `dataflow::timebase`; what is at
//! stake here is the wiring: that a ticking clock actually publishes, that
//! the clock's controls reach phasors through the delta it publishes, that
//! the store is Engine state (so authoring edits cannot destroy it), and
//! that two clocks in a nested tree stay independent.
//!
//! Projects are built in memory rather than on `test_support`'s synthetic
//! tree: the demand chain that ends up ticking a clock at all
//! (output → fixture → shader → `bus:time`) is part of what is under test.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;

use lpc_model::{NodeId, PhasorConfig, Revision, SlotPath, TreePath};
use lpc_registry::{ParseCtx, ProjectRegistry};
use lpfs::{AsLpPath, FsEvent, FsEventKind, LpFs, LpFsMemory, LpPathBuf};

use crate::dataflow::timebase::PhasorKey;
use crate::engine::{Engine, EngineServices, ProjectLoader};

const TICK_MS: u32 = 100;
const TICK_SECONDS: f32 = 0.1;

/// One loaded project under test.
struct Project {
    engine: Engine,
    registry: ProjectRegistry,
    fs: LpFsMemory,
    revision: i64,
}

impl Project {
    fn tick(&mut self) {
        self.engine.tick(&self.registry, TICK_MS).expect("tick");
    }

    /// Tick, ignoring a node-level failure — used where the *point* of the
    /// test is what the tick's end-of-frame maintenance did.
    fn tick_allowing_failure(&mut self) {
        let _ = self.engine.tick(&self.registry, TICK_MS);
    }

    /// Re-read one artifact and apply the resulting changes, the way an
    /// authoring edit does.
    fn apply_edit(&mut self, path: &str) {
        let shapes = self.engine.slot_shapes().clone();
        self.revision += 1;
        let changes = self.registry.refresh_artifacts(
            &self.fs,
            &[FsEvent {
                path: LpPathBuf::from(path),
                kind: FsEventKind::Modify,
            }],
            Revision::new(self.revision),
            &ParseCtx { shapes: &shapes },
        );
        self.engine
            .apply_project_changes(&self.fs, &mut self.registry, &changes)
            .expect("apply project changes");
    }

    /// Stand in for the consumer P3 will add: read a phasor once per tick,
    /// the way a shader uniform fill will.
    fn phasor_after_tick(&mut self, clock: NodeId, period: f32) -> (f32, u32) {
        self.tick();
        self.phasor(clock, period)
    }

    fn phasor(&mut self, clock: NodeId, period: f32) -> (f32, u32) {
        let slot = SlotPath::parse("phase").expect("slot path");
        self.engine
            .timebases_mut()
            .phasor_tick(
                clock,
                &key("phase"),
                &PhasorConfig::with_period(period),
                (NodeId::new(1), &slot),
            )
            .expect("clock published a timebase")
    }

    /// Every alive node whose tree path ends in `suffix`, id-sorted.
    fn nodes_ending(&self, suffix: &str) -> Vec<NodeId> {
        let mut ids: Vec<NodeId> = self
            .engine
            .tree()
            .entries()
            .filter(|entry| entry.path.to_string().ends_with(suffix))
            .map(|entry| entry.id)
            .collect();
        ids.sort();
        ids
    }

    fn the_clock(&self) -> NodeId {
        let ids = self.nodes_ending("clock.clock");
        assert_eq!(ids.len(), 1, "expected exactly one clock, got {ids:?}");
        ids[0]
    }
}

fn key(name: &str) -> PhasorKey {
    PhasorKey::Private {
        node: NodeId::new(1),
        slot: SlotPath::parse(name).expect("slot path"),
    }
}

// --- Project fixtures ------------------------------------------------------

fn write(fs: &LpFsMemory, path: &str, body: &str) {
    let path = String::from(path);
    fs.write_file(path.as_str().as_path(), body.as_bytes())
        .expect("write project file");
}

/// clock → shader (`bus:time`) → fixture → output, under one `prefix`.
fn write_pipeline(fs: &LpFsMemory, prefix: &str, clock_transport: &str, endpoint: &str) {
    write(
        fs,
        &format!("/{prefix}clock.json"),
        &format!("{{ \"kind\": \"Clock\", \"transport\": {clock_transport} }}"),
    );
    write(
        fs,
        &format!("/{prefix}shader.json"),
        r#"
{
  "kind": "Shader",
  "source": "shader.glsl",
  "bindings": { "output": { "target": "bus:visual.out" } },
  "float_mode": "fixed",
  "consumed": {
    "time": {
      "kind": "value",
      "value": "f32",
      "default": 0,
      "default_bind": "bus:time"
    }
  }
}
"#,
    );
    write(
        fs,
        &format!("/{prefix}fixture.json"),
        r#"
{
  "kind": "Fixture",
  "render_size": { "width": 4, "height": 4 },
  "bindings": {
    "input": { "source": "bus:visual.out" },
    "output": { "target": "bus:control.out" }
  },
  "sampling": "direct",
  "mapping": { "kind": "Map2d", "source": "map.json" },
  "color_order": "rgb",
  "brightness": 255,
  "gamma_correction": false
}
"#,
    );
    write(
        fs,
        &format!("/{prefix}output.json"),
        &format!(
            r#"
{{
  "kind": "Output",
  "channels": {{ "0": {{ "endpoint": "{endpoint}" }} }},
  "bindings": {{ "input": {{ "source": "bus:control.out" }} }}
}}
"#
        ),
    );
}

fn write_shared_assets(fs: &LpFsMemory) {
    write(fs, "/project.json", "{ \"format\": 7 }\n");
    write(
        fs,
        "/shader.glsl",
        "uniform float time;\nvec4 render_2d(vec2 p) { return vec4(time, p.y, 0.0, 1.0); }\n",
    );
    write(
        fs,
        "/map.json",
        r#"
{
  "format": 1,
  "sample_diameter": 2.0,
  "canvas": [0.0, 0.0, 100.0, 100.0],
  "objects": [
    { "name": "grid", "shape": { "grid": { "origin": [50, 50], "cols": 2, "rows": 2, "pitch": 10 } } }
  ]
}
"#,
    );
}

fn single_clock_fs(clock_transport: &str) -> LpFsMemory {
    let fs = LpFsMemory::new();
    write_shared_assets(&fs);
    write_pipeline(&fs, "", clock_transport, "ws281x:local:D10");
    write(
        &fs,
        "/module.json",
        r#"
{
  "kind": "Module",
  "nodes": {
    "clock": { "ref": "./clock.json" },
    "shader": { "ref": "./shader.json" },
    "fixture": { "ref": "./fixture.json" },
    "output": { "ref": "./output.json" }
  }
}
"#,
    );
    fs
}

/// Depth 2: a root module with its own pipeline plus a nested module whose
/// clock shadows the outer `bus:time` writer inside it.
fn nested_clocks_fs() -> LpFsMemory {
    let fs = LpFsMemory::new();
    write_shared_assets(&fs);
    write_pipeline(&fs, "", "{ \"rate\": 1.0 }", "ws281x:local:D10");
    write_pipeline(&fs, "inner_", "{ \"rate\": 3.0 }", "ws281x:local:D11");
    write(
        &fs,
        "/inner.json",
        r#"
{
  "kind": "Module",
  "nodes": {
    "clock": { "ref": "./inner_clock.json" },
    "shader": { "ref": "./inner_shader.json" },
    "fixture": { "ref": "./inner_fixture.json" },
    "output": { "ref": "./inner_output.json" }
  }
}
"#,
    );
    write(
        &fs,
        "/module.json",
        r#"
{
  "kind": "Module",
  "nodes": {
    "clock": { "ref": "./clock.json" },
    "shader": { "ref": "./shader.json" },
    "fixture": { "ref": "./fixture.json" },
    "output": { "ref": "./output.json" },
    "inner": { "ref": "./inner.json" }
  }
}
"#,
    );
    fs
}

fn load(fs: LpFsMemory) -> Project {
    let services = EngineServices::new(TreePath::parse("/timebase.show").expect("root path"));
    let loaded = ProjectLoader::load_from_root(&fs, services).expect("load timebase project");
    let (mut engine, registry) = loaded.into_parts();
    engine.set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
        lp_shader::ShaderFrontend::LpsGlsl,
    ))));
    Project {
        engine,
        registry,
        fs,
        revision: 1,
    }
}

// --- Tests -----------------------------------------------------------------

#[test]
fn a_ticking_clock_publishes_its_timebase() {
    let mut project = load(single_clock_fs("{}"));
    let clock = project.the_clock();

    assert_eq!(
        project.engine.timebases().seconds(clock),
        None,
        "nothing is published before the first tick"
    );

    for _ in 0..3 {
        project.tick();
    }

    let seconds = project
        .engine
        .timebases()
        .seconds(clock)
        .expect("published");
    let delta = project.engine.timebases().delta(clock).expect("published");
    // The first tick has no previous engine timestamp to subtract, so only
    // two of the three ticks accumulate.
    assert!(
        (seconds - 2.0 * TICK_SECONDS).abs() < 1e-5,
        "seconds: {seconds}"
    );
    assert!((delta - TICK_SECONDS).abs() < 1e-5, "delta: {delta}");
}

#[test]
fn engine_wall_time_and_the_published_timebase_are_different_clocks() {
    // The clock node stays a transformer ON engine wall time: what it
    // publishes is its own accumulation, not `TickContext::time_seconds`.
    // At rate 1 they track each other except for the first tick, which has
    // no previous engine timestamp to subtract — that one-frame offset is
    // the visible proof they are separate quantities.
    //
    // (Rate and pause cannot be authored in a project file: clock controls
    // are `Debug`-role slot data that the project codec deliberately
    // round-trips to defaults. Their effect on the published delta is
    // pinned at the node in `nodes::clock::clock_node`.)
    let mut project = load(single_clock_fs("{}"));
    let clock = project.the_clock();

    for _ in 0..4 {
        project.tick();
    }

    let seconds = project
        .engine
        .timebases()
        .seconds(clock)
        .expect("published");
    let engine_seconds = project.engine.frame_time().total_ms as f32 / 1000.0;

    assert!(
        (seconds - 3.0 * TICK_SECONDS).abs() < 1e-5,
        "clock seconds: {seconds}"
    );
    assert!(
        (engine_seconds - 4.0 * TICK_SECONDS).abs() < 1e-5,
        "engine seconds: {engine_seconds}"
    );
}

#[test]
fn the_timebase_survives_an_authoring_edit() {
    let mut project = load(single_clock_fs("{ \"rate\": 1.0 }"));
    let clock = project.the_clock();

    for _ in 0..4 {
        project.phasor_after_tick(clock, 10.0);
    }
    let before = project
        .engine
        .timebases()
        .phasor_read(clock, &key("phase"))
        .expect("phasor");
    assert!(before.0 > 0.0, "phase before the edit: {before:?}");

    // An ordinary authoring edit rebuilds bindings from defs. A phasor
    // registered as a binding would vanish here; Engine state does not.
    write(
        &project.fs,
        "/clock.json",
        r#"{ "kind": "Clock", "transport": { "rate": 1.0, "scrub_offset_seconds": 0.0 } }"#,
    );
    project.apply_edit("/clock.json");

    assert_eq!(
        project.engine.timebases().phasor_read(clock, &key("phase")),
        Some(before),
        "the store is Engine state, not bindings"
    );

    let after = project.phasor_after_tick(clock, 10.0);
    assert!(
        after.0 > before.0,
        "and it keeps advancing from where it was: {before:?} -> {after:?}"
    );
}

#[test]
fn a_removed_clock_loses_its_timebase() {
    let mut project = load(single_clock_fs("{ \"rate\": 1.0 }"));
    let clock = project.the_clock();
    project.phasor_after_tick(clock, 10.0);
    assert!(project.engine.timebases().entry(clock).is_some());

    write(
        &project.fs,
        "/module.json",
        r#"
{
  "kind": "Module",
  "nodes": {
    "shader": { "ref": "./shader.json" },
    "fixture": { "ref": "./fixture.json" },
    "output": { "ref": "./output.json" }
  }
}
"#,
    );
    project.apply_edit("/module.json");
    // The shader now has no `bus:time` writer and falls back to its default.
    // Whether that tick succeeds is not the subject; the sweep is.
    project.tick_allowing_failure();

    assert!(
        project.engine.timebases().entry(clock).is_none(),
        "a clock that left the tree must not keep a timebase"
    );
    assert!(project.engine.timebases().is_empty());
}

#[test]
fn two_clocks_in_a_depth_two_tree_keep_independent_timebases() {
    let mut project = load(nested_clocks_fs());
    let clocks = project.nodes_ending("clock.clock");
    assert_eq!(clocks.len(), 2, "root clock and inner-module clock");
    let (outer, inner) = (clocks[0], clocks[1]);

    for _ in 0..4 {
        project.tick();
    }

    // Both clocks are demanded — the inner module's writer shadows the
    // outer one inside its scope, so the inner shader's `bus:time` reaches
    // the inner clock and the outer shader's reaches the outer one.
    assert_eq!(
        project.engine.timebases().len(),
        2,
        "one timebase entry per clock, not one shared entry"
    );
    for clock in &clocks {
        assert!(project.engine.timebases().seconds(*clock).is_some());
    }

    // Same key, two clocks: two integrators. Run one ahead of the other and
    // they must not converge — a store keyed by anything coarser than the
    // clock node would collapse these into one phase.
    for _ in 0..3 {
        project.phasor_after_tick(outer, 10.0);
    }
    let outer_phase = project.phasor_after_tick(outer, 10.0).0;
    let inner_phase = project.phasor(inner, 10.0).0;

    assert!(outer_phase > 0.0, "outer phase: {outer_phase}");
    assert!(
        inner_phase > 0.0 && inner_phase < outer_phase,
        "the inner clock's phasor was only just materialized, so it owes one \
         tick where the outer one owes four: {inner_phase} vs {outer_phase}"
    );
    assert_eq!(
        project
            .engine
            .timebases()
            .entry(outer)
            .expect("outer entry")
            .phasor_count(),
        1
    );
    assert_eq!(
        project
            .engine
            .timebases()
            .entry(inner)
            .expect("inner entry")
            .phasor_count(),
        1
    );

    // And from here they advance in lockstep but never merge.
    let outer_next = project.phasor_after_tick(outer, 10.0).0;
    let inner_next = project.phasor(inner, 10.0).0;
    assert!(
        (outer_next - outer_phase - (inner_next - inner_phase)).abs() < 1e-6,
        "equal deltas must advance both by the same amount"
    );
    assert!(
        outer_next > inner_next,
        "…while keeping their independent offsets: {outer_next} vs {inner_next}"
    );
}
