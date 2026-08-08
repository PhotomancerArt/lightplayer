//! An unbound palette slot still surfaces its picker on the module panel
//! (M4 D5) — the palette analogue of `plasma_phasor_is_public`.
//!
//! Where plasma pins an AUTHORED binding, the palette slot leans on the
//! other half of ADR 2026-08-03-panel-visibility-is-derived (amended): a
//! `default_bind` alone materializes a Default-origin binding, which the
//! panel deliberately does not present, and the declared `panel: "show"`
//! hint is the additive override that promotes exactly that binding. A
//! shader slot is authored data, so it spells the hint as a field value
//! where a native def spells it as `#[slot(panel = "show")]` metadata.

use std::sync::Arc;

use lpc_engine::{EngineServices, ProjectLoader};
use lpc_model::{BindingRef, GradientConfig, PanelHint, ShaderSlotDef, TreePath};
use lpfs::{AsLpPath, LpFs, LpFsMemory};

/// The def half: `ShaderSlotDef::palette()` declares the D5 pair, so every
/// generated palette slot gets the picker without the author spelling it.
#[test]
fn palette_slots_declare_the_d5_default_bind_and_panel_hint() {
    let slot = ShaderSlotDef::palette("Palette", "", GradientConfig::default());

    let endpoint = slot
        .default_bind
        .data
        .as_ref()
        .map(|endpoint| endpoint.value().clone());
    assert_eq!(
        endpoint,
        Some(BindingRef::parse("bus:palette").expect("bus:palette parses")),
        "a palette slot default-binds the shared palette channel"
    );
    assert_eq!(
        slot.panel_hint(),
        Some(PanelHint::Show),
        "the default-bound channel is promoted to a panel control"
    );
}

fn write(fs: &LpFsMemory, path: &str, body: &str) {
    fs.write_file(String::from(path).as_str().as_path(), body.as_bytes())
        .expect("write project file");
}

const PALETTE_GLSL: &str = "layout(binding = 0) uniform vec2 outputSize;\n\
     layout(binding = 1) uniform sampler2D palette;\n\
     vec4 render_2d(vec2 pos) { return texture(palette, vec2(pos.x / outputSize.x, 0.0)); }";

/// One shader with an unbound palette slot. `panel` decides whether the
/// slot def carries the `panel: "show"` hint.
fn palette_fs(panel: bool) -> LpFsMemory {
    let hint = if panel { r#", "panel": "show""# } else { "" };
    let fs = LpFsMemory::new();
    write(&fs, "/project.json", "{ \"format\": 7 }\n");
    write(&fs, "/palette.glsl", PALETTE_GLSL);
    write(
        &fs,
        "/shader.json",
        &format!(
            r#"
{{
  "kind": "Shader",
  "source": {{ "path": "palette.glsl" }},
  "bindings": {{}},
  "consumed": {{
    "palette": {{ "kind": "palette", "value": "sampler2D", "label": "Palette",
                  "description": "", "default_bind": "bus:palette"{hint} }}
  }}
}}
"#
        ),
    );
    write(
        &fs,
        "/module.json",
        "{ \"kind\": \"Module\", \"nodes\": { \"shader\": { \"ref\": \"./shader.json\" } } }\n",
    );
    fs
}

fn probe_graph(panel: bool) -> lpc_wire::WireBindingGraph {
    let services = EngineServices::new(TreePath::parse("/palette_public.show").expect("root path"));
    let mut rt =
        ProjectLoader::load_from_root(&palette_fs(panel), services).expect("load palette project");
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
    graph
}

/// The runtime half: the materialized Default-origin binding carries
/// `panel_show`, so the panel presents the picker with no authored wiring.
#[test]
fn an_unbound_palette_slot_is_promoted_to_the_panel() {
    let graph = probe_graph(true);

    let channel = graph
        .channels
        .iter()
        .find(|channel| channel.name == "palette")
        .expect("the default bind materializes the palette channel");
    let binding = channel
        .consumers
        .iter()
        .map(|index| &graph.bindings[*index as usize])
        .find(|binding| {
            binding
                .slot
                .as_ref()
                .is_some_and(|slot| slot.to_string() == "palette")
        })
        .expect("the palette slot consumes bus:palette");
    assert_eq!(
        binding.origin,
        lpc_wire::WireBindingOrigin::Default,
        "no authored wiring — the binding is the materialized default"
    );
    assert!(
        binding.panel_show,
        "the declared panel hint promotes the default binding: {binding:?}"
    );
}

/// Without the hint the derived rule stands: a default bind alone stays off
/// the panel, so the hint is doing the promotion — not the palette kind.
#[test]
fn a_palette_slot_without_the_hint_stays_private() {
    let graph = probe_graph(false);

    let channel = graph
        .channels
        .iter()
        .find(|channel| channel.name == "palette")
        .expect("the default bind still materializes the channel");
    let binding = channel
        .consumers
        .iter()
        .map(|index| &graph.bindings[*index as usize])
        .find(|binding| {
            binding
                .slot
                .as_ref()
                .is_some_and(|slot| slot.to_string() == "palette")
        })
        .expect("the palette slot consumes bus:palette");
    assert!(
        !binding.panel_show,
        "a default bind alone must not reach the panel: {binding:?}"
    );
}
