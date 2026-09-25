//! An unloaded entry's phasors leave with it (multi-pattern plan P6b, AC1,
//! AC4).
//!
//! A real project (clock → playlist → fixture → output) tours between an
//! idle shader and the catalog's `pulse` pattern, whose shader consumes a
//! `phasor` uniform — so while pulse plays, the engine's timebase store
//! holds an integrator keyed by pulse's shader. The store's own idle horizon
//! (`PHASOR_IDLE_TICKS`, 120 ticks) used to keep it long after the entry
//! was gone: a touring playlist carried its last few patterns' phasors that
//! way (P6 measured it as the whole of AC4's pass-to-pass difference). What
//! this pins: right after the residency step that unloads pulse, the store
//! holds nothing keyed by, read by, or scoped to any node that was in it,
//! on the first pass and on the second.
//!
//! No whole-heap assertion here: on the host, this harness's live heap moves
//! by several KB from one tour cycle to the next with shader-compile and
//! graphics activity (measured while writing this), which would drown a
//! phasor's few dozen bytes. The retained-heap proof is P6's fw-emu tour
//! (`docs/reports/2026-09-25-dormant-playlist-entries-proof.md`), and the
//! engine's bookkeeping of a switch is pinned by `entry_residency_memory`.
//!
//! ```bash
//! cargo test -p lpc-engine --test entry_residency_phasors -- --nocapture
//! ```

use lpc_engine::dataflow::timebase::PhasorKey;
use lpc_engine::engine::{EntryResidencyEvent, LoadedProjectRuntime};
use lpc_engine::node::ScopeRef;
use lpc_engine::{Engine, EngineServices, ProjectLoader};
use lpc_model::{NodeId, NodeUseLocation, SlotPath, TreePath};
use lpfs::{AsLpPath, LpFs, LpFsMemory};

const IDLE: u32 = 1;
const PULSE: u32 = 2;

/// Frames in one tour step (0.5 s at 16 ms a frame) plus slack; a cycle is
/// two steps.
const FRAMES_PER_CYCLE: usize = 2 * 32 + 8;

#[test]
fn an_unloaded_entry_leaves_no_phasor_behind() {
    let fs = project_fs();
    let services = EngineServices::new(TreePath::parse("/phasors.show").expect("path"));
    let mut rt = ProjectLoader::load_from_root(&fs, services).expect("load project");
    rt.engine_mut().set_graphics(Some(std::sync::Arc::new(
        lp_gfx_lpvm::TargetLpvmGraphics::new(lp_shader::ShaderFrontend::LpsGlsl),
    )));

    let first = tour_until_pulse_unloads(&mut rt, &fs);
    let second = tour_until_pulse_unloads(&mut rt, &fs);

    let while_playing = second.phasors_while_playing;
    for (pass, stats) in [("first", first), ("second", second)] {
        assert!(
            stats.phasors_while_playing > 0,
            "{pass} pass: pulse's shader never materialized a phasor, so this \
             test would prove nothing"
        );
        assert!(
            stats.leftovers.is_empty(),
            "{pass} pass: the store still holds the unloaded entry's phasors \
             right after the residency step: {:?}",
            stats.leftovers
        );
    }
    println!("pulse's phasors while it plays: {while_playing}; none left after its unload");
}

struct PassStats {
    phasors_while_playing: usize,
    leftovers: Vec<PhasorKey>,
}

/// Tick (residency step, then tick, as every edge does) until pulse is
/// unloaded; read the store just before that step and straight after it,
/// before the tick's own sweep can run. Then one more tick.
fn tour_until_pulse_unloads(rt: &mut LoadedProjectRuntime, fs: &LpFsMemory) -> PassStats {
    let mut before: Option<(Vec<NodeId>, usize)> = None;
    for _ in 0..FRAMES_PER_CYCLE {
        let playing = entry_child(rt.engine(), PULSE).map(|child| {
            let subtree = subtree_of(rt.engine(), child);
            let count = phasors_of(rt.engine(), &subtree).len();
            (subtree, count)
        });
        let applied = rt.apply_residency(fs).expect("residency step");
        let unloaded = applied.events.iter().any(
            |event| matches!(event, EntryResidencyEvent::Unloaded { entry, .. } if *entry == PULSE),
        );
        if unloaded {
            let (subtree, phasors_while_playing) = playing
                .or(before)
                .expect("pulse was playing before its unload");
            let leftovers = phasors_of(rt.engine(), &subtree);
            rt.tick(16).expect("tick");
            assert_eq!(loaded(rt.engine()), [IDLE], "back on idle");
            return PassStats {
                phasors_while_playing,
                leftovers,
            };
        }
        if playing.is_some() {
            before = playing;
        }
        rt.tick(16).expect("tick");
    }
    panic!("pulse did not unload within one tour cycle");
}

fn playlist_id(engine: &Engine) -> NodeId {
    engine
        .project_runtime_index()
        .node_id(&NodeUseLocation::root().child(SlotPath::parse("nodes[list]").expect("slot")))
        .expect("playlist projected")
}

fn entry_child(engine: &Engine, entry: u32) -> Option<NodeId> {
    let owner = playlist_id(engine);
    let scope = ScopeRef::Sink { owner, entry };
    engine
        .tree()
        .entries()
        .find(|node| node.parent == Some(owner) && node.scope == Some(scope))
        .map(|node| node.id)
}

fn loaded(engine: &Engine) -> Vec<u32> {
    [IDLE, PULSE]
        .into_iter()
        .filter(|entry| entry_child(engine, *entry).is_some())
        .collect()
}

/// `root` and every node below it.
fn subtree_of(engine: &Engine, root: NodeId) -> Vec<NodeId> {
    let is_under = |mut id: NodeId| {
        loop {
            if id == root {
                return true;
            }
            match engine.tree().get(id).and_then(|entry| entry.parent) {
                Some(parent) => id = parent,
                None => return false,
            }
        }
    };
    engine
        .tree()
        .entries()
        .map(|entry| entry.id)
        .filter(|id| is_under(*id))
        .collect()
}

/// Every phasor in the store that belongs to pulse's entry: keyed by, or
/// read by, a node of `subtree`, or scoped to a scope one of them owns or to
/// the entry's own sink.
fn phasors_of(engine: &Engine, subtree: &[NodeId]) -> Vec<PhasorKey> {
    let sink = ScopeRef::Sink {
        owner: playlist_id(engine),
        entry: PULSE,
    };
    let mut keys = Vec::new();
    for clock in engine.tree().entries().map(|entry| entry.id) {
        let Some(timebase) = engine.timebases().entry(clock) else {
            continue;
        };
        for (key, state) in timebase.phasors() {
            let keyed = match key {
                PhasorKey::Private { node, .. } => subtree.contains(node),
                PhasorKey::Shared { scope, .. } => {
                    subtree.contains(&scope.owner()) || *scope == sink
                }
            };
            let read = state
                .readings()
                .iter()
                .any(|reading| subtree.contains(&reading.node));
            if keyed || read {
                keys.push(key.clone());
            }
        }
    }
    keys
}

/// Clock, a two-entry touring playlist (idle shader, the catalog's `pulse`
/// pattern module), and `projects/test/basic`'s fixture and output.
fn project_fs() -> LpFsMemory {
    let fs = LpFsMemory::new();
    write(&fs, "/project.json", b"{\n  \"format\": 11\n}\n");
    write(
        &fs,
        "/module.json",
        br#"{
  "kind": "Module",
  "nodes": {
    "clock": { "ref": "./clock.json" },
    "list": { "ref": "./playlist.json" },
    "fixture": { "ref": "./fixture.json" },
    "output": { "ref": "./output.json" }
  }
}"#,
    );
    write(&fs, "/clock.json", br#"{ "kind": "Clock" }"#);
    write(
        &fs,
        "/playlist.json",
        br#"{
  "kind": "Playlist",
  "bindings": {
    "time": { "source": "bus:time" },
    "output": { "target": "bus:visual.out" }
  },
  "idle_entry": 1,
  "default_fade": 0.1,
  "tour": { "kind": "cycle", "step_seconds": 0.5, "fade_seconds": 0.1 },
  "entries": {
    "1": { "name": "idle", "node": { "ref": "./idle.json" } },
    "2": { "name": "pulse", "node": { "ref": "./pulse/module.json" } }
  }
}"#,
    );
    write(
        &fs,
        "/idle.json",
        br#"{ "kind": "Shader", "source": "idle.glsl" }"#,
    );
    write(
        &fs,
        "/idle.glsl",
        b"vec4 render_2d(vec2 pos) { return vec4(0.2, 0.2, 0.2, 1.0); }\n",
    );
    write(
        &fs,
        "/pulse/module.json",
        include_bytes!("../../../catalog/patterns/pulse/effect/module.json"),
    );
    write(
        &fs,
        "/pulse/shader.json",
        include_bytes!("../../../catalog/patterns/pulse/effect/shader.json"),
    );
    write(
        &fs,
        "/pulse/shader.glsl",
        include_bytes!("../../../catalog/patterns/pulse/effect/shader.glsl"),
    );
    write(
        &fs,
        "/fixture.json",
        include_bytes!("../../../projects/test/basic/fixture.json"),
    );
    write(
        &fs,
        "/fixture.map2d.json",
        include_bytes!("../../../projects/test/basic/fixture.map2d.json"),
    );
    write(
        &fs,
        "/output.json",
        include_bytes!("../../../projects/test/basic/output.json"),
    );
    fs
}

fn write(fs: &LpFsMemory, path: &str, bytes: &[u8]) {
    fs.write_file(path.as_path(), bytes).expect("write fixture");
}
