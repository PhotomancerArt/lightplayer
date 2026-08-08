//! A deploy must render the source it just wrote — not the previous one.
//!
//! `lp-cli upload` (and every other push) drives
//! `lpa_client::project_deploy_requests`: stop all projects, write the files,
//! load the project. Existing project tests write files *directly* and then
//! load, which is exactly the ordering that hides a stale read; this one goes
//! through the deploy request sequence against `LpServer::tick_and_send`, the
//! same entry point firmware serves.
//!
//! The oracle is an independent fresh-server render of each shader, so the
//! assertion says "this deploy rendered THAT source", not merely "the output
//! changed".
//!
//! Coverage for `docs/defects/2026-07-30-deploy-compiles-previous-upload.md`.
//! These were green when written: they pin the deploy path rather than fix it,
//! so that investigation does not have to re-suspect it.

extern crate alloc;

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::path::{Path, PathBuf};

use lp_gfx_lpvm::TargetLpvmGraphics;
use lpa_client::ProjectDeployFile;
use lpa_client::project_deploy::{project_deploy_requests, validate_project_deploy_response};
use lpa_server::{LpGraphics, LpServer};
use lpc_hardware::default_esp32s3_hardware_manifest;
use lpc_model::AsLpPath;
use lpc_shared::output::MemoryOutputProvider;
use lpc_shared::transport::ServerTransport;
use lpc_wire::{ClientMessage, ClientRequest, TransportError, WireMessage, WireServerMessage};
use lpfs::LpFsMemory;
use lpfs::lp_path::LpPathBuf;

const PROJECT_ID: &str = "deploy-reload";

/// Frames rendered after a load, so the shader compiles and produces output.
const FRAMES: u32 = 4;

const SHADER_V1: &str = r#"
layout(binding = 0) uniform vec2 outputSize;

vec4 render_2d(vec2 pos) {
    vec2 uv = pos / outputSize;
    return vec4(uv.x, 0.0, 0.0, 1.0);
}
"#;

const SHADER_V2: &str = r#"
layout(binding = 0) uniform vec2 outputSize;

// A second version, deliberately a different byte length than the first.
vec4 render_2d(vec2 pos) {
    vec2 uv = pos / outputSize;
    return vec4(0.0, 0.0, uv.y, 1.0);
}
"#;

#[test]
fn a_deploy_renders_the_source_it_just_wrote() {
    let expected_v1 = fresh_load_render(SHADER_V1);
    let expected_v2 = fresh_load_render(SHADER_V2);
    assert_ne!(
        expected_v1, expected_v2,
        "the two shader versions must render differently, or this test proves nothing"
    );

    let (mut server, output_provider) = memory_server();

    deploy(&mut server, SHADER_V1);
    advance(&mut server, FRAMES);
    assert_eq!(
        rendered_bytes(&output_provider.borrow()),
        expected_v1,
        "the first deploy must render its own source"
    );

    deploy(&mut server, SHADER_V2);
    advance(&mut server, FRAMES);
    assert_eq!(
        rendered_bytes(&output_provider.borrow()),
        expected_v2,
        "the second deploy rendered a different source than the one it wrote \
         (a stale-by-one-upload reload)"
    );
}

#[test]
fn a_batched_deploy_renders_the_source_it_just_wrote() {
    let expected_v1 = fresh_load_render(SHADER_V1);
    let expected_v2 = fresh_load_render(SHADER_V2);

    let (mut server, output_provider) = memory_server();

    deploy_in_one_batch(&mut server, SHADER_V1);
    advance(&mut server, FRAMES);
    assert_eq!(rendered_bytes(&output_provider.borrow()), expected_v1);

    deploy_in_one_batch(&mut server, SHADER_V2);
    advance(&mut server, FRAMES);
    assert_eq!(
        rendered_bytes(&output_provider.borrow()),
        expected_v2,
        "a deploy served entirely within one frame still loads its own writes"
    );
}

/// Drive the real deploy request sequence through the server's transport
/// entry point, one request per frame — a client awaits each response, so a
/// deploy normally lands one request per drained batch.
fn deploy(server: &mut LpServer, shader: &str) {
    let requests = deploy_requests(shader);
    let mut transport = VecTransport::default();
    for message in deploy_messages(&requests) {
        block_on(server.tick_and_send(16, vec![message], &mut transport)).expect("tick_and_send");
    }
    assert_deploy_accepted(&requests, &transport);
}

/// The same deploy, drained as a single batch: `advance_frame` runs once,
/// *before* any of the writes, so the whole sequence is served from state
/// measured ahead of them. This is the ordering a stale read would need.
fn deploy_in_one_batch(server: &mut LpServer, shader: &str) {
    let requests = deploy_requests(shader);
    let mut transport = VecTransport::default();
    block_on(server.tick_and_send(16, deploy_messages(&requests), &mut transport))
        .expect("tick_and_send");
    assert_deploy_accepted(&requests, &transport);
}

fn deploy_requests(shader: &str) -> Vec<ClientRequest> {
    project_deploy_requests(PROJECT_ID, project_files(shader))
}

fn deploy_messages(requests: &[ClientRequest]) -> Vec<WireMessage> {
    requests
        .iter()
        .enumerate()
        .map(|(index, request)| {
            WireMessage::Client(ClientMessage {
                id: index as u64 + 1,
                msg: request.clone(),
            })
        })
        .collect()
}

/// Every deploy request answered, and answered with the response the client
/// requires — a write that failed server-side would otherwise show up only as
/// a render that happens to look right.
fn assert_deploy_accepted(requests: &[ClientRequest], transport: &VecTransport) {
    assert_eq!(transport.sent.len(), requests.len(), "one response each");
    let mut loaded = false;
    for (request, response) in requests.iter().zip(&transport.sent) {
        loaded |= validate_project_deploy_response(request, &response.msg)
            .expect("deploy request rejected")
            .is_some();
    }
    assert!(loaded, "deploy must end by loading the project");
}

fn advance(server: &mut LpServer, frames: u32) {
    for _ in 0..frames {
        server.advance_frame(16).expect("advance frame");
    }
}

/// The same project, loaded once into a fresh server — the independent
/// expectation each deploy is measured against.
fn fresh_load_render(shader: &str) -> Vec<u8> {
    let (mut server, output_provider) = memory_server();
    for file in project_files(shader) {
        let path = LpPathBuf::from("/projects")
            .join(PROJECT_ID)
            .join(file.relative_path());
        server
            .base_fs_mut()
            .write_file(path.as_path(), file.bytes())
            .expect("write project file");
    }
    server
        .load_project(LpPathBuf::from("/projects").join(PROJECT_ID).as_path())
        .expect("load project");
    advance(&mut server, FRAMES);
    let bytes = rendered_bytes(&output_provider.borrow());
    assert!(
        bytes.iter().any(|byte| *byte != 0),
        "the reference render is entirely black — it cannot distinguish two shaders"
    );
    bytes
}

/// The `shader-oracle` example with `shader.glsl` replaced: a clock-free
/// shader → fixture → output chain whose frames are identical, so the LED
/// bytes identify the compiled source and nothing else.
fn project_files(shader: &str) -> Vec<ProjectDeployFile> {
    let dir = repo_root().join("examples").join("shader-oracle");
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("read shader-oracle") {
        let path = entry.expect("dir entry").path();
        if !path.is_file() {
            continue;
        }
        let name = path
            .file_name()
            .expect("file name")
            .to_string_lossy()
            .into_owned();
        if !name.ends_with(".json") {
            continue;
        }
        files.push(ProjectDeployFile::new(
            name,
            std::fs::read(&path).expect("read project file"),
        ));
    }
    files.push(ProjectDeployFile::new("shader.glsl", shader.as_bytes()));
    files
}

fn memory_server() -> (LpServer, Rc<RefCell<MemoryOutputProvider>>) {
    let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::with_hardware_manifest(
        default_esp32s3_hardware_manifest(),
    )));
    let graphics: Arc<dyn LpGraphics> =
        Arc::new(TargetLpvmGraphics::new(lpa_server::DEVICE_SHADER_FRONTEND));
    let server = LpServer::new(
        output_provider.clone(),
        Box::new(LpFsMemory::new()),
        "/projects/".as_path(),
        None,
        None,
        graphics,
    );
    (server, output_provider)
}

/// The u16 engine samples reduced to the bytes a ws281x driver sees. The
/// oracle project's output options make `DisplayPipeline` collapse to this
/// stateless rounding step (see `examples/shader-oracle/README.md`).
fn rendered_bytes(provider: &MemoryOutputProvider) -> Vec<u8> {
    let mut bytes = Vec::new();
    for handle in provider.get_all_handles() {
        let Some(samples) = provider.get_data(handle) else {
            continue;
        };
        bytes.extend(
            samples
                .iter()
                .map(|sample| ((u32::from(*sample) + 0x80) >> 8).min(255) as u8),
        );
    }
    bytes
}

/// `CARGO_MANIFEST_DIR` is `lp-app/lpa-server`; the repo root is two levels up.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .to_path_buf()
}

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
