//! The checked-in meteor example must actually animate on the CPU (lpvm)
//! tier: the sim compute shader integrates positions from the scope's time
//! product into a persistent sentinel map, so they have to move across
//! engine ticks.
//!
//! Meteor is the sanctioned-integrator case of the TimeProduct migration: it
//! computes `dt = time - prev_time`, so its `time` uniform stays **unbounded
//! seconds** where every other animated example moved to a wrapped phasor. A
//! phasor there would rewind `dt` once per cycle and stall the meteors, so
//! the slot kind is pinned below alongside the movement assertion.
//!
//! Regression pin for the editor-sim "meteor frozen while the clock
//! advances" defect: every wasm shader instance's vmctx sat at guest address
//! 0 of the engine's shared linear memory, so the render px shader clobbered
//! the compute shader's persistent globals each frame. The GPU tier hid the
//! defect (its render shader lives in wgpu), which is why the gallery
//! preview animated while the editor sim froze. See
//! docs/defects/2026-08-03-wasm-shader-instances-share-vmctx.md.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lpc_engine::engine::LoadedProjectRuntime;
use lpc_engine::node::NodeEntryState;
use lpc_engine::{EngineServices, ProjectLoader};
use lpc_model::{LpValue, SlotDataAccess, SlotPath, TreePath, lookup_slot_data};
use lpfs::LpFsStd;

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lpc-engine lives two levels under the workspace root")
        .parent()
        .expect("workspace dir")
        .to_path_buf()
}

fn load_meteor() -> LoadedProjectRuntime {
    let fs = LpFsStd::new(workspace_dir().join("examples/meteor"));
    let services = EngineServices::new(TreePath::parse("/meteor.show").expect("root path"));
    let mut rt = ProjectLoader::load_from_root(&fs, services).expect("load examples/meteor");
    rt.engine_mut()
        .set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
    rt
}

/// Read `meteors[<first key>].pos.x` from the sim node's runtime state.
fn meteor0_pos_x(rt: &LoadedProjectRuntime) -> Option<f32> {
    let engine = rt.engine();
    let entry = engine
        .tree()
        .entries()
        .find(|entry| entry.path.to_string().ends_with("sim.compute_shader"))
        .expect("sim node in tree");
    let NodeEntryState::Alive(node) = entry.state.value() else {
        panic!("sim node alive");
    };
    let state = node.runtime_state_slots()?;
    let data = lookup_slot_data(
        state,
        engine.slot_shapes(),
        &SlotPath::parse("meteors").expect("meteors path"),
    )
    .ok()?;
    let SlotDataAccess::Map(map) = data else {
        return None;
    };
    let first_key = map.keys().into_iter().next()?;
    let SlotDataAccess::Value(value) = map.get(&first_key)? else {
        return None;
    };
    let LpValue::Struct { fields, .. } = value.value() else {
        return None;
    };
    fields.iter().find_map(|(name, value)| {
        (name == "pos").then(|| match value {
            LpValue::Vec2(v) => v[0],
            _ => panic!("pos should be vec2, got {value:?}"),
        })
    })
}

/// The def half of the pin: meteor integrates a delta, so its timebase
/// uniform must stay `seconds`. Reading the authored artifact (rather than
/// the loaded node) is deliberate — this is a statement about the checked-in
/// example, which is what a future sweep would be tempted to "finish".
#[test]
fn meteor_sim_keeps_an_unbounded_seconds_uniform() {
    let def = std::fs::read_to_string(workspace_dir().join("examples/meteor/sim.json"))
        .expect("read examples/meteor/sim.json");
    let def: serde_json::Value = serde_json::from_str(&def).expect("parse sim.json");
    let time = &def["consumed"]["time"];

    assert_eq!(
        time["kind"], "seconds",
        "meteor/sim integrates dt; a phasor would rewind it every cycle"
    );
    assert!(
        def["bindings"]["time"].is_null(),
        "a seconds slot reads the scope's time product, it takes no binding"
    );
}

#[test]
fn meteor_positions_advance_across_ticks() {
    let mut rt = load_meteor();

    // Warm up past the compile-window deferral and the seed tick.
    for _ in 0..5 {
        rt.tick(33).expect("tick");
    }
    let start = meteor0_pos_x(&rt).expect("meteor pos after warm-up");

    for _ in 0..30 {
        rt.tick(33).expect("tick");
    }
    let end = meteor0_pos_x(&rt).expect("meteor pos after run");

    // ~1s at velocity 0.22 × speed 1 → about 0.22 of travel (mod 1.0).
    // Anything clearly nonzero proves the integration is live.
    let moved = (end - start).abs();
    assert!(
        moved > 0.05,
        "meteor 0 should travel across ticks: start={start} end={end} moved={moved}"
    );
}
