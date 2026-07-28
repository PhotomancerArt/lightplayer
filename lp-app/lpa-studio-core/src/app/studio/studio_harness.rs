//! In-process studio world for e2e tests and the headless agent runner.
//!
//! One home for the fixture the studio e2e suites always rebuilt inline:
//! a real [`LpServer`] behind a [`ClientIo`] adapter, a connected
//! [`StudioController`], and the DTO walkers the assertions share. The
//! `#[cfg(test)]` suites in this crate consume it directly; the
//! `lpa-agent-harness` runner (P2) reaches it through the `harness`
//! feature — same code, zero duplication, so the runner exercises exactly
//! the world the e2e tests prove.
//!
//! Compiled under `cfg(any(test, feature = "harness"))`: in test builds
//! the heavy host-only deps (`lpa-server`, `lp-gfx-lpvm`, `lpc-shared`)
//! come from dev-dependencies; under the `harness` feature the same crates
//! are optional dependencies. Never part of a production (wasm or host)
//! build.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::{Pin, pin};
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use lp_gfx_lpvm::TargetLpvmGraphics;
use lpa_client::ClientIo;
use lpa_server::{LpGraphics, LpServer};
use lpc_model::AsLpPath;
use lpc_shared::output::MemoryOutputProvider;
use lpc_shared::transport::ServerTransport;
use lpc_wire::{ClientMessage, TransportError, WireMessage, WireServerMessage};
use lpfs::LpFsMemory;

pub use lpa_server::ShaderFrontend;

use crate::{
    ControllerId, ProjectController, ProjectOp, StudioCommand, StudioController,
    StudioServerClient, UiAction, UiNodeSection, UiNodeTabBody, UiStudioView, UiViewContent,
};

/// The agent project's starting shader source (a position gradient).
pub const ASSET_SHADER_V1: &str =
    "uniform float time;\n\nvec4 render(vec2 pos) {\n    return vec4(pos.x, pos.y, 0.5, 1.0);\n}\n";

/// Where the in-memory project files live on the server's fs.
pub const PROJECT_DIR: &str = "/projects/edit-e2e";

/// A real in-process server with the agent-ready project loaded (clock +
/// shader + fixture + output, so the demand chain renders — and therefore
/// compiles — the shader), compiling via the server's pinned frontend
/// ([`lpa_server::DEVICE_SHADER_FRONTEND`]).
pub fn asset_e2e_server() -> LpServer {
    agent_e2e_server(lpa_server::DEVICE_SHADER_FRONTEND)
}

/// [`asset_e2e_server`] with an explicit GLSL frontend — the runner's
/// browser-parity seam. Frontends are a host product decision (the
/// `ShaderFrontend::default()` removal), so the harness takes it as a
/// parameter instead of inheriting lpa-server's device pin.
pub fn agent_e2e_server(frontend: ShaderFrontend) -> LpServer {
    agent_e2e_server_with_backend(frontend).0
}

/// [`agent_e2e_server`] plus the constructed engine's backend name (the
/// server keeps its graphics private, so the name is captured here).
fn agent_e2e_server_with_backend(frontend: ShaderFrontend) -> (LpServer, &'static str) {
    let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::new()));
    let graphics: Arc<dyn LpGraphics> = Arc::new(TargetLpvmGraphics::new(frontend));
    let backend_name = graphics.backend_name();
    let mut server = LpServer::new(
        output_provider,
        Box::new(LpFsMemory::new()),
        "projects".as_path(),
        None,
        None,
        graphics,
    );

    // The shader publishes to the visual bus and a fixture consumes it —
    // without a consumer the shader never renders, so it would never
    // (re)compile and compile errors would never surface.
    let shader_json = r#"{
  "kind": "Shader",
  "source": "shader.glsl",
  "bindings": {
    "output": { "target": "bus:visual.out" }
  },
  "consumed": {
    "time": {
      "kind": "value",
      "value": "f32",
      "default": 0,
      "label": "Time",
      "description": "Project clock time in seconds"
    }
  }
}"#;
    let fixture_json = r#"{
  "kind": "Fixture",
  "render_size": { "width": 4, "height": 4 },
  "bindings": {
    "input": { "source": "bus:visual.out" },
    "output": { "target": "bus:control.out" }
  }
}"#;
    // The output node drives the demand chain (output pulls control →
    // fixture pulls visual → shader renders/compiles); the memory output
    // provider accepts any authored endpoint.
    let output_json = r#"{
  "kind": "Output",
  "endpoint": "ws281x:rmt:D10",
  "bindings": {
    "input": { "source": "bus:control.out" }
  }
}"#;
    let project_json = r#"{
  "kind": "Project",
  "format": 1,
  "nodes": {
    "clock": { "ref": "./clock.json" },
    "shader": { "ref": "./shader.json" },
    "pixels": { "ref": "./fixture.json" },
    "output": { "ref": "./output.json" }
  }
}"#;
    let clock_json = r#"{
  "kind": "Clock",
  "controls": {
    "running": true,
    "rate": 1.0
  }
}"#;
    let files: &[(&str, &str)] = &[
        ("project.json", project_json),
        ("clock.json", clock_json),
        ("shader.json", shader_json),
        ("fixture.json", fixture_json),
        ("output.json", output_json),
        ("shader.glsl", ASSET_SHADER_V1),
    ];
    for (name, body) in files {
        server
            .base_fs_mut()
            .write_file(format!("{PROJECT_DIR}/{name}").as_path(), body.as_bytes())
            .expect("write project file");
    }
    server
        .load_project(PROJECT_DIR.as_path())
        .expect("load asset-e2e project");
    server.advance_frame(16).expect("tick");
    (server, backend_name)
}

/// The connected studio world for one headless agent session: the real
/// server behind [`InProcessServerIo`], a [`StudioController`] attached
/// through the same stub-sim + injected-client fixture the e2e tests use,
/// and the resolved VM-runtime name for the run header.
pub struct AgentWorld {
    pub controller: StudioController,
    pub server: Rc<RefCell<LpServer>>,
    /// The LPVM engine actually executing shaders for this world (e.g.
    /// `lpvm-wasm::rt_wasmtime` on host) — print it next to the frontend
    /// so a run states which compiler path it exercised.
    pub backend_name: &'static str,
}

impl AgentWorld {
    /// Build the world compiling via `frontend`. Mirrors the agent e2e's
    /// `AgentHarness::new` minus the scripted provider/spawner (callers
    /// install their own via the public `set_agent_*` seams).
    pub fn new(frontend: ShaderFrontend) -> Self {
        let (server, backend_name) = agent_e2e_server_with_backend(frontend);
        let server = Rc::new(RefCell::new(server));
        let io = InProcessServerIo {
            server: Rc::clone(&server),
            inbox: Rc::new(RefCell::new(VecDeque::new())),
            sent: Rc::new(RefCell::new(Vec::new())),
        };
        let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
        let controller = StudioController::connected_with_client_for_harness(client);
        Self {
            controller,
            server,
            backend_name,
        }
    }
}

/// The connect action every walk starts with (and other project ops).
pub fn project_action(op: ProjectOp) -> StudioCommand {
    StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        op,
    ))
}

/// Find the shader node's inline asset editor anywhere in the editor DTO
/// tree, or `None` while the project view has not resolved it yet.
pub fn try_find_asset_editor(view: &UiStudioView) -> Option<crate::UiAssetEditor> {
    let editor = view.panes.iter().find_map(|pane| match &pane.body {
        UiViewContent::ProjectEditor(editor) => Some(editor),
        _ => None,
    })?;

    fn in_slots(slots: &[crate::UiConfigSlot]) -> Option<crate::UiAssetEditor> {
        slots.iter().find_map(|slot| match &slot.body {
            crate::UiConfigSlotBody::Asset(asset) => asset.inline_editor.clone(),
            crate::UiConfigSlotBody::Record(record) => in_slots(&record.fields),
            _ => None,
        })
    }
    fn in_sections(sections: &[UiNodeSection]) -> Option<crate::UiAssetEditor> {
        sections.iter().find_map(|section| match section {
            UiNodeSection::AssetSlots(slots) | UiNodeSection::ConfigSlots(slots) => in_slots(slots),
            _ => None,
        })
    }
    fn in_children(children: &[crate::UiNodeChild]) -> Option<crate::UiAssetEditor> {
        children
            .iter()
            .find_map(|child| in_sections(&child.sections).or_else(|| in_children(&child.children)))
    }

    editor.nodes.iter().find_map(|node| {
        node.tabs
            .iter()
            .find_map(|tab| match &tab.body {
                UiNodeTabBody::Sections(sections) => in_sections(sections),
                _ => None,
            })
            .or_else(|| in_children(&node.children))
    })
}

/// [`try_find_asset_editor`], panicking form for test assertions.
pub fn find_asset_editor(view: &UiStudioView) -> crate::UiAssetEditor {
    try_find_asset_editor(view).expect("shader node exposes an inline asset editor")
}

/// `ClientIo` that pumps each client message through the in-process server's
/// `tick_and_send` and queues the produced frames for `receive`.
pub struct InProcessServerIo {
    pub server: Rc<RefCell<LpServer>>,
    pub inbox: Rc<RefCell<VecDeque<WireServerMessage>>>,
    pub sent: Rc<RefCell<Vec<ClientMessage>>>,
}

impl ClientIo for InProcessServerIo {
    fn send<'life0, 'async_trait>(
        &'life0 mut self,
        msg: ClientMessage,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        self.sent.borrow_mut().push(msg.clone());
        let server = Rc::clone(&self.server);
        let inbox = Rc::clone(&self.inbox);
        Box::pin(async move {
            let mut transport = CollectTransport::default();
            server
                .borrow_mut()
                .tick_and_send(16, vec![WireMessage::Client(msg)], &mut transport)
                .await
                .map_err(|error| TransportError::Other(format!("server error: {error}")))?;
            inbox.borrow_mut().extend(transport.sent);
            Ok(())
        })
    }

    fn receive<'life0, 'async_trait>(
        &'life0 mut self,
    ) -> Pin<Box<dyn Future<Output = Result<WireServerMessage, TransportError>> + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        let response = self
            .inbox
            .borrow_mut()
            .pop_front()
            .ok_or_else(|| TransportError::Other("in-process server inbox empty".to_string()));
        Box::pin(async move { response })
    }

    fn close<'life0, 'async_trait>(
        &'life0 mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async { Ok(()) })
    }
}

/// In-memory server transport that records every sent frame.
#[derive(Default)]
struct CollectTransport {
    sent: Vec<WireServerMessage>,
}

impl ServerTransport for CollectTransport {
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

/// Drive a future to completion with a self-waking waker (bounded, so a hung
/// future fails the test instead of the suite).
pub fn drive<F: Future>(future: F) -> F::Output {
    struct NoopWake;
    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }

    let waker = Waker::from(Arc::new(NoopWake));
    let mut context = Context::from_waker(&waker);
    let mut future = pin!(future);
    for _ in 0..100_000 {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => {}
        }
    }
    panic!("e2e future did not complete");
}
