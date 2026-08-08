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
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use lp_gfx_lpvm::TargetLpvmGraphics;
use lpa_server::{LpGraphics, LpServer, Project};
use lpc_model::{
    AsLpPath, Colorspace, FromLpValue, Gradient, GradientConfig, GradientStop, InterpMethod,
    LpPathBuf, LpValue, ToLpValue,
};
use lpc_shared::output::MemoryOutputProvider;
use lpc_shared::transport::ServerTransport;
use lpc_wire::{
    BindingGraphProbeRequest, BindingGraphProbeResult, ClientMessage, ClientRequest,
    ProjectReadEvent, ProjectReadQuery, ProjectReadQueryEvent, ProjectReadRequest,
    RuntimeReadQuery, TransportError, WireBindingGraph, WireBindingOrigin, WireMessage,
    WirePanelAutoSaveRequest, WirePanelClearRequest, WirePanelCommandResponse,
    WirePanelWriteRequest, WireProjectCommand, WireProjectCommandResponse, WireProjectHandle,
    WireScopeRef, WireServerMessage, WireServerMsgBody,
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

/// One channel's owning scope.
fn channel_scope(graph: &WireBindingGraph, name: &str) -> WireScopeRef {
    graph
        .channels
        .iter()
        .find(|channel| channel.name == name)
        .unwrap_or_else(|| panic!("no {name} channel in the graph"))
        .scope
        .expect("channels list scoped")
}

/// One channel's current resolved value.
fn channel_value(graph: &WireBindingGraph, name: &str) -> Option<LpValue> {
    graph
        .channels
        .iter()
        .find(|channel| channel.name == name)?
        .value
        .as_ref()?
        .value
        .clone()
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
            b"{\n  \"format\": 7\n}\n",
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
  "transport": {
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

/// The palette chooser's write path (M4 P3), end to end on the wire shape
/// Studio dispatches: a panel write whose payload is a whole
/// `GradientConfig` — a STRUCT, where every other panel command carries a
/// scalar. The engine's `resolve_gradient_config` takes a driven config
/// whole (never as a partial overlay), so what has to round-trip is the
/// entire record: kind tag, the gradient set, and both timings.
#[test]
fn a_gradient_config_panel_write_round_trips_on_a_palette_channel() {
    let (mut server, project_path) = server_with_palette_shader_project("panel-palette-write");
    let handle = server.load_project(project_path.as_path()).expect("load");
    server.advance_frame(16).expect("tick");

    let project = project_mut(&mut server, handle);
    let scope = channel_scope(&probe(project), "palette");

    let picked = GradientConfig::Cycle {
        set: vec![solid([1.0, 0.0, 0.0]), solid([0.0, 0.4, 1.0])],
        step_seconds: 20.0,
        fade_seconds: 0.5,
    };
    let response = project.panel_write(&WirePanelWriteRequest {
        scope,
        channel: "palette".to_string(),
        value: picked.to_lp_value(),
        ttl_ms: None,
    });
    assert_eq!(response, WirePanelCommandResponse::Accepted { engaged: 1 });

    let graph = probe(project);
    let held = channel_value(&graph, "palette").expect("the channel resolves to the held palette");
    assert_eq!(
        GradientConfig::from_lp_value(&held).expect("the payload is still a GradientConfig"),
        picked,
        "the picked palette survives the wire whole — set, count, and timings"
    );

    // And clearing releases it, exactly like a scalar control.
    let response = project.panel_clear(&WirePanelClearRequest::Channel {
        scope,
        channel: "palette".to_string(),
    });
    assert_eq!(response, WirePanelCommandResponse::Accepted { engaged: 0 });
    assert_ne!(
        channel_value(&probe(project), "palette"),
        Some(picked.to_lp_value()),
        "clearing drops the panel's palette"
    );
}

/// The same gradient panel write, read back through the REAL wire transport
/// — the budgeted project-read stream sink — instead of calling the engine
/// probe directly. This is browser Studio's actual read path after a
/// palette pick, and it is where the padded §5 storage form used to fail
/// the whole read: the probe echoes the held channel value raw inside one
/// event, and a padded `GradientConfig` (~17.7 KiB) alone exceeded
/// `PROJECT_READ_FRAME_MAX_BYTES` ("project-read event exceeded frame
/// budget of 16384 bytes"). The stops-literal storage form (ADR
/// 2026-08-05-gradient-stops-string-storage) is what keeps this passing.
#[test]
fn a_gradient_panel_write_survives_a_wire_project_read() {
    let (mut server, project_path) = server_with_palette_shader_project("panel-palette-wire-read");
    let handle = server.load_project(project_path.as_path()).expect("load");
    server.advance_frame(16).expect("tick");

    let project = project_mut(&mut server, handle);
    let scope = channel_scope(&probe(project), "palette");
    let picked = GradientConfig::Cycle {
        set: vec![solid([1.0, 0.0, 0.0]), solid([0.0, 0.4, 1.0])],
        step_seconds: 20.0,
        fade_seconds: 0.5,
    };
    let response = project.panel_write(&WirePanelWriteRequest {
        scope,
        channel: "palette".to_string(),
        value: picked.to_lp_value(),
        ttl_ms: None,
    });
    assert_eq!(response, WirePanelCommandResponse::Accepted { engaged: 1 });

    let messages = vec![WireMessage::Client(ClientMessage {
        id: 13,
        msg: ClientRequest::ProjectRead {
            handle,
            request: ProjectReadRequest {
                since: None,
                queries: Vec::new(),
                probes: vec![lpc_wire::ProjectProbeRequest::BindingGraph(
                    BindingGraphProbeRequest {
                        include_values: true,
                    },
                )],
            },
        },
    })];
    let mut transport = VecTransport::default();
    block_on(server.tick_and_send(16, messages, &mut transport)).expect("tick");
    let events: Vec<ProjectReadEvent> = transport
        .sent
        .into_iter()
        .filter_map(|message| match message.msg {
            WireServerMsgBody::ProjectRead { events } => Some(events),
            _ => None,
        })
        .flatten()
        .collect();
    let errors: Vec<&str> = events
        .iter()
        .filter_map(|event| match event {
            ProjectReadEvent::Error { message } => Some(message.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        errors.is_empty(),
        "the wire read after a palette pick must not fail: {errors:?}"
    );
}

/// One solid-color gradient, so an assertion is about the palette path
/// rather than about a transfer function.
fn solid(c: [f32; 3]) -> Gradient {
    Gradient {
        space: Colorspace::LinearSrgb,
        method: InterpMethod::Linear,
        stops: vec![GradientStop { at: 0.0, c }, GradientStop { at: 1.0, c }],
    }
}

/// One shader whose `palette` slot consumes `bus:palette` — the wiring that
/// makes a palette swatch a PANEL control rather than a card-local one.
fn server_with_palette_shader_project(name: &str) -> (LpServer, LpPathBuf) {
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
    "tint": { "ref": "./tint.json" }
  }
}
"#,
        )
        .expect("write module");
    server
        .base_fs_mut()
        .write_file(
            project_path.join("tint.json").as_path(),
            br#"
{
  "kind": "Shader",
  "source": "tint.glsl",
  "float_mode": "fixed",
  "bindings": { "palette": { "source": "bus:palette" } },
  "consumed": {
    "palette": {
      "kind": "palette", "value": "sampler2D",
      "label": "Palette", "description": ""
    }
  }
}
"#,
        )
        .expect("write shader");
    server
        .base_fs_mut()
        .write_file(
            project_path.join("tint.glsl").as_path(),
            b"layout(binding = 0) uniform vec2 outputSize;\n\
              layout(binding = 1) uniform sampler2D palette;\n\
              vec4 render(vec2 pos) { return texture(palette, vec2(pos.x / outputSize.x, 0.0)); }",
        )
        .expect("write glsl");
    (server, project_path)
}

#[test]
fn the_auto_save_toggle_round_trips_over_the_wire() {
    // The P11 switch has two halves and they travel on DIFFERENT messages:
    // the WRITE is `WireProjectCommand::PanelAutoSave`, and the READ rides
    // `ServerRuntimeStatus::panel_auto_save` on the ordinary project read
    // Studio already makes every refresh. This drives both through the
    // real transport, because a toggle whose new value never comes back
    // is a switch that lies.
    let (mut server, project_path) = server_with_clock_project("panel-auto-save-wire");
    let handle = server.load_project(project_path.as_path()).expect("load");
    server.advance_frame(16).expect("tick");

    assert_eq!(
        read_panel_auto_save(&mut server, handle),
        Some(true),
        "auto-save is on by default (panel.md P11) and the read says so"
    );

    let response = command(
        &mut server,
        handle,
        WireProjectCommand::PanelAutoSave {
            request: WirePanelAutoSaveRequest { enabled: false },
        },
    );
    assert!(
        matches!(
            response,
            WireProjectCommandResponse::PanelAutoSave {
                response: WirePanelCommandResponse::Accepted { .. }
            }
        ),
        "the toggle answers in the shared panel-command shape, got {response:?}"
    );

    assert_eq!(
        read_panel_auto_save(&mut server, handle),
        Some(false),
        "and the next read carries the new value"
    );
    assert!(
        !server
            .project_manager()
            .get_project(handle)
            .expect("loaded")
            .panel_auto_save(),
        "the wire arm moved the server-side flag, not just the report"
    );

    // Back on again: the arm is not one-way.
    command(
        &mut server,
        handle,
        WireProjectCommand::PanelAutoSave {
            request: WirePanelAutoSaveRequest { enabled: true },
        },
    );
    assert_eq!(read_panel_auto_save(&mut server, handle), Some(true));
}

/// Dispatch one project command through the real transport and return its
/// response body.
fn command(
    server: &mut LpServer,
    handle: WireProjectHandle,
    command: WireProjectCommand,
) -> WireProjectCommandResponse {
    let messages = vec![WireMessage::Client(ClientMessage {
        id: 11,
        msg: ClientRequest::ProjectCommand { handle, command },
    })];
    let mut transport = VecTransport::default();
    block_on(server.tick_and_send(16, messages, &mut transport)).expect("tick");
    match transport.sent.into_iter().next().expect("one response").msg {
        WireServerMsgBody::ProjectCommand { response } => response,
        other => panic!("expected a project command response, got {other:?}"),
    }
}

/// The auto-save flag as reported by a runtime project read — the carrier
/// Studio actually reads it from.
fn read_panel_auto_save(server: &mut LpServer, handle: WireProjectHandle) -> Option<bool> {
    let messages = vec![WireMessage::Client(ClientMessage {
        id: 12,
        msg: ClientRequest::ProjectRead {
            handle,
            request: ProjectReadRequest {
                since: None,
                queries: vec![ProjectReadQuery::Runtime(RuntimeReadQuery)],
                probes: Vec::new(),
            },
        },
    })];
    let mut transport = VecTransport::default();
    block_on(server.tick_and_send(16, messages, &mut transport)).expect("tick");
    transport
        .sent
        .into_iter()
        .filter_map(|message| match message.msg {
            WireServerMsgBody::ProjectRead { events } => Some(events),
            _ => None,
        })
        .flatten()
        .find_map(|event| match event {
            ProjectReadEvent::Query {
                event: ProjectReadQueryEvent::Runtime(runtime),
                ..
            } => Some(runtime.server?.panel_auto_save),
            _ => None,
        })
        .flatten()
}

/// In-memory transport that records every sent server message.
#[derive(Default)]
struct VecTransport {
    sent: Vec<WireServerMessage>,
}

impl ServerTransport for VecTransport {
    async fn send(&mut self, msg: WireServerMessage) -> Result<(), TransportError> {
        self.sent.push(msg);
        Ok(())
    }

    async fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError> {
        Ok(None)
    }

    async fn receive_all(&mut self) -> Result<Vec<ClientMessage>, TransportError> {
        Ok(Vec::new())
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        match Future::poll(Pin::as_mut(&mut future), &mut cx) {
            Poll::Ready(output) => return output,
            Poll::Pending => {}
        }
    }
}

fn noop_waker() -> Waker {
    unsafe fn clone(_: *const ()) -> RawWaker {
        RawWaker::new(core::ptr::null(), &VTABLE)
    }
    unsafe fn wake(_: *const ()) {}
    unsafe fn wake_by_ref(_: *const ()) {}
    unsafe fn drop(_: *const ()) {}
    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);

    unsafe { Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE)) }
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
