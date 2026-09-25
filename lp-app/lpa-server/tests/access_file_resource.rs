//! A project that names its own access sidecar as a resource cannot read it.
//!
//! The fs wire path refuses an access file's bytes on every link. This is
//! the other door: a loaded project's runtime. A shader whose `source` is
//! `.lp/access.json` would have the engine read the sidecar and compile it,
//! and whatever the compile error quotes rides out on a play-tier
//! `ProjectRead`. `AccessGuardedFs` refuses the read, so the project gets a
//! load error (or, re-pointed after it loaded, the asset reports the
//! refusal) and no reply — project read, inventory, overlay — carries a byte
//! of the file.
//!
//! The control proves the door is real: the same bytes under a name that is
//! NOT an access file are read, and the canary does come out.

extern crate alloc;

use alloc::boxed::Box;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::path::PathBuf;

use lp_gfx_lpvm::TargetLpvmGraphics;
use lpa_server::{LpGraphics, LpServer};
use lpc_access::{ProjectAccessFile, SecretEntry, Tier};
use lpc_model::{AsLpPath, LpPathBuf};
use lpc_shared::output::MemoryOutputProvider;
use lpc_shared::transport::{Incoming, Link, LinkId};
use lpc_wire::{
    ClientMessage, ClientRequest, NodeReadQuery, ProjectReadQuery, ProjectReadRequest,
    TransportError, WireOverlayReadRequest, WireProjectCommand, WireProjectHandle,
    WireProjectInventoryReadRequest, WireServerMessage,
};
use lpfs::LpFsMemory;

/// A label nothing else in a project or a reply could contain.
const CANARY: &str = "canary-7f3a-sidecar-label";

/// A project whose shader names the sidecar is refused at load, and the
/// refusal names the file without quoting it.
#[test]
fn a_project_loaded_with_the_sidecar_as_a_source_is_refused_without_the_bytes() {
    let mut server = server_with_project(".lp/access.json");
    let error = server
        .load_project(PROJECT.as_path())
        .expect_err("the shader's source cannot be read");
    let error = format!("{error:?}");
    assert!(
        error.contains("access files are not readable by a project"),
        "{error}"
    );
    assert!(!error.contains(CANARY), "{error}");
}

/// The control: the same bytes under a name that is NOT an access file ARE
/// read into the runtime, and the canary comes out — quoted by the shader
/// compiler's parse error in the node's status, on a `ProjectRead` a
/// play-tier link may make. This is the door the guard closes; without the
/// control the test above could pass because nothing ever quotes a source.
#[test]
fn the_same_bytes_under_another_name_do_come_out() {
    let mut server = server_with_project(".lp/not-access.json");
    let handle = server.load_project(PROJECT.as_path()).expect("load");
    // The shader compiles on the first frame, and fails.
    for _ in 0..4 {
        server.advance_frame(16).expect("advance frame");
    }
    let replies = replies(&mut server, handle);
    assert!(
        replies.contains(CANARY),
        "the control no longer surfaces its source; the test above proves nothing:\n{replies}"
    );
}

/// A loaded project whose shader is re-pointed at the sidecar on disk (what
/// a write of `shader.json` does) reads nothing: the inventory names the
/// refusal as the asset's state, and no reply carries a byte.
#[test]
fn a_shader_retargeted_at_the_sidecar_reads_nothing() {
    let mut server = server_with_project("shader.glsl");
    let handle = server.load_project(PROJECT.as_path()).expect("load");
    server
        .base_fs_mut()
        .write_file(
            LpPathBuf::from(PROJECT).join("shader.json").as_path(),
            &shader_json(".lp/access.json"),
        )
        .expect("retarget the shader");
    for _ in 0..4 {
        server.advance_frame(16).expect("advance frame");
    }
    let replies = replies(&mut server, handle);
    assert!(
        !replies.contains(CANARY),
        "a byte of the access file reached a reply:\n{replies}"
    );
    assert!(
        replies.contains("access files are not readable by a project"),
        "the asset reports why it did not load:\n{replies}"
    );
}

const PROJECT: &str = "/projects/leak";

/// Every reply a play-or-edit client could ask a loaded project for —
/// project read, inventory, overlay — as one string.
fn replies(server: &mut LpServer, handle: WireProjectHandle) -> String {
    let mut replies = String::new();
    for request in [
        ClientRequest::ProjectRead {
            handle,
            request: ProjectReadRequest {
                since: None,
                queries: vec![ProjectReadQuery::Nodes(NodeReadQuery::detail_all())],
                probes: Vec::new(),
            },
        },
        project_command(
            handle,
            WireProjectCommand::ReadInventory {
                request: WireProjectInventoryReadRequest,
            },
        ),
        project_command(
            handle,
            WireProjectCommand::ReadOverlay {
                request: WireOverlayReadRequest,
            },
        ),
    ] {
        let mut transport = VecTransport::default();
        let message = Incoming::primary(ClientMessage {
            id: 1,
            msg: request,
        });
        block_on(server.tick_and_send(16, vec![message], &mut transport)).expect("tick");
        for frame in transport.sent {
            replies.push_str(&format!("{:?}\n", frame.msg));
        }
    }
    replies
}

/// A server holding the shader-oracle project at [`PROJECT`] with its
/// shader's `source` set to `source`, the project's sidecar
/// (label [`CANARY`]), and the same bytes again at `.lp/not-access.json`.
fn server_with_project(source: &str) -> LpServer {
    let mut server = memory_server();
    let project = LpPathBuf::from(PROJECT);
    for (name, bytes) in shader_oracle_files() {
        let bytes = if name == "shader.json" {
            shader_json(source)
        } else {
            bytes
        };
        server
            .base_fs_mut()
            .write_file(project.join(&name).as_path(), &bytes)
            .expect("write project file");
    }
    server
        .base_fs_mut()
        .write_file(project.join("shader.glsl").as_path(), SHADER.as_bytes())
        .expect("write shader");
    let sidecar = ProjectAccessFile::new(vec![SecretEntry::from_password(
        CANARY,
        Tier::Edit,
        b"hunter2",
        [9; 16],
        3,
    )]);
    let sidecar = sidecar.to_json().expect("sidecar json");
    for path in [".lp/access.json", ".lp/not-access.json"] {
        server
            .base_fs_mut()
            .write_file(project.join(path).as_path(), sidecar.as_bytes())
            .expect("write sidecar");
    }
    server
}

/// The shader-oracle's `shader.json` with its `source` set to `source`.
fn shader_json(source: &str) -> Vec<u8> {
    let path = repo_root().join("projects/test/shader-oracle/shader.json");
    let text = std::fs::read_to_string(path).expect("read shader.json");
    text.replace("\"shader.glsl\"", &format!("\"{source}\""))
        .into_bytes()
}

const SHADER: &str = r#"
layout(binding = 0) uniform vec2 outputSize;

vec4 render_2d(vec2 pos) {
    vec2 uv = pos / outputSize;
    return vec4(uv.x, 0.0, 0.0, 1.0);
}
"#;

fn project_command(handle: WireProjectHandle, command: WireProjectCommand) -> ClientRequest {
    ClientRequest::ProjectCommand { handle, command }
}

/// The `shader-oracle` example's JSON files: a shader → fixture → output
/// chain whose shader has one `source` to point somewhere.
fn shader_oracle_files() -> Vec<(String, Vec<u8>)> {
    let dir = repo_root().join("projects/test/shader-oracle");
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("read shader-oracle") {
        let path = entry.expect("dir entry").path();
        let name = path
            .file_name()
            .expect("file name")
            .to_string_lossy()
            .into_owned();
        if path.is_file() && name.ends_with(".json") {
            files.push((name, std::fs::read(&path).expect("read project file")));
        }
    }
    files
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn memory_server() -> LpServer {
    LpServer::new(
        Rc::new(RefCell::new(MemoryOutputProvider::new())),
        Box::new(LpFsMemory::new()),
        "/projects/".as_path(),
        None,
        None,
        Arc::new(TargetLpvmGraphics::new(lpa_server::DEVICE_SHADER_FRONTEND))
            as Arc<dyn LpGraphics>,
    )
}

/// In-memory transport that records every sent server message.
#[derive(Default)]
struct VecTransport {
    sent: Vec<WireServerMessage>,
}

impl lpc_shared::transport::ServerTransport for VecTransport {
    async fn send(&mut self, _link: LinkId, msg: WireServerMessage) -> Result<(), TransportError> {
        self.sent.push(msg);
        Ok(())
    }

    async fn receive(&mut self) -> Result<Option<Incoming>, TransportError> {
        Ok(None)
    }

    async fn receive_all(&mut self) -> Result<Vec<Incoming>, TransportError> {
        Ok(Vec::new())
    }

    fn links(&self) -> Vec<Link> {
        vec![Link::PRIMARY]
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
        if let Poll::Ready(output) = Future::poll(Pin::as_mut(&mut future), &mut cx) {
            return output;
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
