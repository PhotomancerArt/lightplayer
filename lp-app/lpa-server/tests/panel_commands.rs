//! Panel command round-trips (panel.md P8/P9, roadmap P9): `PanelWrite` /
//! `PanelClear` are runtime pokes on the playlist-activate pattern —
//! engage a writer at `(scope, channel)`, shadow the channel's authored
//! providers, touch NOTHING authored (no overlay entry, no dirty), and
//! converge across clients through ordinary probe pulls (last writer
//! wins at the engine).

extern crate alloc;

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::ToString;
use alloc::sync::Arc;
use core::cell::RefCell;

use lp_gfx_lpvm::TargetLpvmGraphics;
use lpa_server::{LpGraphics, LpServer, Project};
use lpc_model::{AsLpPath, LpPathBuf, LpValue};
use lpc_shared::output::MemoryOutputProvider;
use lpc_wire::{
    BindingGraphProbeRequest, BindingGraphProbeResult, WireBindingGraph, WireBindingOrigin,
    WirePanelClearRequest, WirePanelCommandResponse, WirePanelWriteRequest, WireProjectHandle,
    WireScopeRef,
};
use lpfs::LpFsMemory;

#[test]
fn panel_write_engages_shadows_and_clears_through_the_probe() {
    let (mut server, project_path) = server_with_clock_project("panel-roundtrip");
    let handle = server.load_project(project_path.as_path()).expect("load");
    server.advance_frame(16).expect("tick");

    let project = project_mut(&mut server, handle);
    let (scope, authored_value) = time_channel(&probe(project));
    assert!(
        authored_value.is_some(),
        "clock publishes bus:time before any panel writer exists"
    );

    // Write: the writer engages and the channel resolves to the held value.
    let response = project.panel_write(&WirePanelWriteRequest {
        scope,
        channel: "time".to_string(),
        value: LpValue::F32(123.5),
        ttl_ms: None,
    });
    assert_eq!(response, WirePanelCommandResponse::Accepted { engaged: 1 });

    let graph = probe(project);
    let (_, held) = time_channel(&graph);
    assert_eq!(
        held,
        Some(LpValue::F32(123.5)),
        "the engaged writer shadows the clock's authored publish"
    );
    assert!(
        panel_provider_engaged(&graph),
        "the probe surfaces the writer as a Panel-origin provider row"
    );

    // Clear: authored wiring returns, the Panel row is gone.
    let response = project.panel_clear(&WirePanelClearRequest::Channel {
        scope,
        channel: "time".to_string(),
    });
    assert_eq!(response, WirePanelCommandResponse::Accepted { engaged: 0 });

    let graph = probe(project);
    let (_, released) = time_channel(&graph);
    assert_ne!(
        released,
        Some(LpValue::F32(123.5)),
        "clearing releases the held value back to the clock"
    );
    assert!(
        !panel_provider_engaged(&graph),
        "no Panel-origin provider row remains after clear"
    );
}

#[test]
fn panel_writes_touch_nothing_authored() {
    let (mut server, project_path) = server_with_clock_project("panel-no-dirty");
    let handle = server.load_project(project_path.as_path()).expect("load");
    server.advance_frame(16).expect("tick");

    let project = project_mut(&mut server, handle);
    let (scope, _) = time_channel(&probe(project));

    project.panel_write(&WirePanelWriteRequest {
        scope,
        channel: "time".to_string(),
        value: LpValue::F32(9.0),
        ttl_ms: None,
    });

    let overlay = project.read_overlay();
    assert!(
        overlay.overlay.is_empty(),
        "a panel write stages no overlay entry (nothing dirty, nothing to save): {:?}",
        overlay.overlay
    );
}

#[test]
fn concurrent_writes_converge_on_the_last_writer() {
    // Two clients fighting one knob (panel.md P9): both speak the same
    // command channel, the engine keeps ONE writer per (scope, channel),
    // and every probe pull reflects whoever moved last.
    let (mut server, project_path) = server_with_clock_project("panel-last-writer");
    let handle = server.load_project(project_path.as_path()).expect("load");
    server.advance_frame(16).expect("tick");

    let project = project_mut(&mut server, handle);
    let (scope, _) = time_channel(&probe(project));

    for value in [1.0_f32, 2.0, 7.75] {
        let response = project.panel_write(&WirePanelWriteRequest {
            scope,
            channel: "time".to_string(),
            value: LpValue::F32(value),
            ttl_ms: None,
        });
        assert_eq!(
            response,
            WirePanelCommandResponse::Accepted { engaged: 1 },
            "re-writes update the one writer, never stack a second"
        );
    }

    let (_, held) = time_channel(&probe(project));
    assert_eq!(held, Some(LpValue::F32(7.75)), "last writer wins");
}

#[test]
fn a_stale_scope_rejects_normally() {
    // A gesture racing a structural edit may name a scope whose owner is
    // gone; that is a normal Rejected response, never a wire error.
    let (mut server, project_path) = server_with_clock_project("panel-stale-scope");
    let handle = server.load_project(project_path.as_path()).expect("load");
    server.advance_frame(16).expect("tick");

    let project = project_mut(&mut server, handle);
    let response = project.panel_write(&WirePanelWriteRequest {
        scope: WireScopeRef::Module {
            owner: lpc_model::NodeId::new(9999),
        },
        channel: "time".to_string(),
        value: LpValue::F32(1.0),
        ttl_ms: None,
    });
    assert!(
        matches!(response, WirePanelCommandResponse::Rejected { .. }),
        "unknown scope owner rejects: {response:?}"
    );
}

// ---------------------------------------------------------------------------

fn probe(project: &mut Project) -> WireBindingGraph {
    let (engine, registry) = project.runtime_read_parts();
    let result = engine.read_project_binding_graph_probe(
        registry,
        BindingGraphProbeRequest {
            include_values: true,
        },
    );
    let BindingGraphProbeResult::Graph(graph) = result else {
        panic!("expected graph result");
    };
    graph
}

/// The `time` channel's scope and current resolved value.
fn time_channel(graph: &WireBindingGraph) -> (WireScopeRef, Option<LpValue>) {
    let channel = graph
        .channels
        .iter()
        .find(|channel| channel.name == "time")
        .expect("clock publishes bus:time");
    let scope = channel.scope.expect("channels list scoped");
    let value = channel.value.as_ref().and_then(|value| value.value.clone());
    (scope, value)
}

/// Whether the `time` channel lists a Panel-origin provider row.
fn panel_provider_engaged(graph: &WireBindingGraph) -> bool {
    graph
        .channels
        .iter()
        .find(|channel| channel.name == "time")
        .is_some_and(|channel| {
            channel.providers.iter().any(|index| {
                graph
                    .bindings
                    .get(*index as usize)
                    .is_some_and(|binding| binding.origin == WireBindingOrigin::Panel)
            })
        })
}

fn project_mut(server: &mut LpServer, handle: WireProjectHandle) -> &mut Project {
    server
        .project_manager_mut()
        .get_project_mut(handle)
        .expect("loaded project")
}

fn server_with_clock_project(name: &str) -> (LpServer, LpPathBuf) {
    let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::new()));
    let graphics: Arc<dyn LpGraphics> =
        Arc::new(TargetLpvmGraphics::new(lpa_server::DEVICE_SHADER_FRONTEND));
    let mut server = LpServer::new(
        output_provider,
        Box::new(LpFsMemory::new()),
        "projects".as_path(),
        None,
        None,
        graphics,
    );
    let project_path = LpPathBuf::from("/projects").join(name);

    server
        .base_fs_mut()
        .write_file(
            project_path.join("project.json").as_path(),
            b"{\n  \"format\": 3\n}\n",
        )
        .expect("write container manifest");
    server
        .base_fs_mut()
        .write_file(
            project_path.join("module.json").as_path(),
            br#"
{
  "kind": "Module",
  "nodes": {
    "clock": {
      "ref": "./clock.json"
    }
  }
}
"#,
        )
        .expect("write project");
    server
        .base_fs_mut()
        .write_file(
            project_path.join("clock.json").as_path(),
            br#"
{
  "kind": "Clock",
  "controls": {
    "rate": 1.0
  }
}
"#,
        )
        .expect("write clock");

    (server, project_path)
}

// ---------------------------------------------------------------------------

/// The full authored path a panel control depends on, which no other test
/// covers end to end: a shader uniform with `bindings: { glow: { source:
/// "bus:glow" } }` must reach the probe as a CONSUMING Bus endpoint that
/// carries a scope. That endpoint is the only thing Studio turns into a
/// `UiPanelTarget`, so if the scope is absent the knob silently stays an
/// ordinary slot editor and every panel behaviour downstream is untested.
#[test]
fn an_authored_bus_binding_on_a_uniform_reaches_the_probe_with_a_scope() {
    let (mut server, project_path) = server_with_glow_shader_project("panel-authored-bind");
    let handle = server.load_project(project_path.as_path()).expect("load");
    server.advance_frame(16).expect("tick");

    let project = project_mut(&mut server, handle);
    let graph = probe(project);

    let consuming = graph
        .bindings
        .iter()
        .find(|binding| {
            binding.direction == lpc_wire::WireBindingDirection::Consumes
                && matches!(
                    &binding.endpoint,
                    lpc_wire::WireBindingEndpoint::Bus { channel, .. } if channel == "glow"
                )
        })
        .unwrap_or_else(|| {
            panic!(
                "no consuming binding to bus:glow; endpoints: {:?}",
                graph
                    .bindings
                    .iter()
                    .map(|b| (&b.direction, &b.endpoint))
                    .collect::<alloc::vec::Vec<_>>()
            )
        });

    let lpc_wire::WireBindingEndpoint::Bus { scope, .. } = &consuming.endpoint else {
        panic!("checked above");
    };
    assert!(
        scope.is_some(),
        "the consuming endpoint carries its scope — Studio keys the panel \
         target on (scope, channel), so a None here means no panel control"
    );

    // ...and the channel lists in that scope, which is what makes a control
    // appear at all (modules.md R6: an unwritten channel still lists).
    assert!(
        graph.channels.iter().any(|c| c.name == "glow"),
        "bus:glow lists even with no writer: {:?}",
        graph
            .channels
            .iter()
            .map(|c| &c.name)
            .collect::<alloc::vec::Vec<_>>()
    );
}

/// One shader whose `glow` uniform consumes `bus:glow` —
/// the shape the G4 walk project uses.
fn server_with_glow_shader_project(name: &str) -> (LpServer, LpPathBuf) {
    let (mut server, project_path) = server_with_clock_project(name);
    server
        .base_fs_mut()
        .write_file(
            project_path.join("module.json").as_path(),
            br#"
{
  "kind": "Module",
  "nodes": {
    "clock": { "ref": "./clock.json" },
    "idle": { "ref": "./idle.json" }
  }
}
"#,
        )
        .expect("write module");
    server
        .base_fs_mut()
        .write_file(
            project_path.join("idle.json").as_path(),
            br#"
{
  "kind": "Shader",
  "source": "idle.glsl",
  "float_mode": "fixed",
  "bindings": { "glow": { "source": "bus:glow" } },
  "consumed": {
    "glow": {
      "kind": "value", "value": "f32", "default": 0.5,
      "min": 0, "max": 1, "label": "Glow"
    }
  }
}
"#,
        )
        .expect("write shader");
    server
        .base_fs_mut()
        .write_file(
            project_path.join("idle.glsl").as_path(),
            b"void main() { out_color = vec4(glow, glow, glow, 1.0); }",
        )
        .expect("write glsl");
    (server, project_path)
}
