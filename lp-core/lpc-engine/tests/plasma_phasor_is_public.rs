//! The checked-in plasma example is the TimeProduct demo: its phasor slot's
//! period must reach the module panel, not just the shader card.
//!
//! Publicity is an AUTHORED binding (docs/adr/2026-08-03-panel-visibility-is-
//! derived.md) — a `default_bind` alone materializes a Default-origin binding,
//! which the panel deliberately does not present. The M2 migration sweep
//! replaced plasma's `speed` value uniform with a `phase` phasor and left only
//! the `default_bind`, which quietly demoted the demo's one knob to the card
//! face. This pins the authored binding back in place.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lpc_engine::{EngineServices, ProjectLoader};
use lpc_model::TreePath;
use lpfs::LpFsStd;

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lpc-engine lives two levels under the workspace root")
        .parent()
        .expect("workspace dir")
        .to_path_buf()
}

/// The def half: a statement about the checked-in artifact, which is what a
/// future sweep would be tempted to "tidy".
#[test]
fn plasma_phase_keeps_an_authored_binding() {
    let def = std::fs::read_to_string(workspace_dir().join("examples/plasma/shader.json"))
        .expect("read examples/plasma/shader.json");
    let def: serde_json::Value = serde_json::from_str(&def).expect("parse shader.json");

    assert_eq!(
        def["consumed"]["phase"]["kind"], "phasor",
        "plasma's animation rides a phasor"
    );
    assert_eq!(
        def["bindings"]["phase"]["source"], "bus:speed",
        "the period knob only joins the module panel through an AUTHORED binding"
    );
}

/// The runtime half: the bound channel really shows up on the root scope with
/// the phasor slot as its consumer, so the panel has a channel to present.
#[test]
fn plasma_publishes_its_phasor_config_channel() {
    let fs = LpFsStd::new(workspace_dir().join("examples/plasma"));
    let services = EngineServices::new(TreePath::parse("/plasma.show").expect("root path"));
    let mut rt = ProjectLoader::load_from_root(&fs, services).expect("load examples/plasma");
    rt.engine_mut()
        .set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
    for _ in 0..5 {
        rt.tick(33).expect("tick");
    }

    let (engine, registry) = rt.read_parts();
    let probe = engine.read_project_binding_graph_probe(
        registry,
        lpc_wire::BindingGraphProbeRequest {
            include_values: false,
        },
    );
    let lpc_wire::BindingGraphProbeResult::Graph(graph) = probe else {
        panic!("binding graph probe failed");
    };

    let channel = graph
        .channels
        .iter()
        .find(|channel| channel.name == "speed")
        .expect("the phasor's config channel is listed on the root scope");
    let consumers: Vec<_> = channel
        .consumers
        .iter()
        .map(|index| &graph.bindings[*index as usize])
        .collect();
    assert!(
        consumers.iter().any(|binding| {
            binding.origin == lpc_wire::WireBindingOrigin::Authored
                && binding
                    .slot
                    .as_ref()
                    .is_some_and(|slot| slot.to_string() == "phase")
        }),
        "plasma's phase slot must consume bus:speed by authored binding: {consumers:?}"
    );
}
