//! Refusal-not-reset for loads: a LoadProject the device cannot afford fails
//! with a structured error on the request id and leaves the server fully
//! alive — it must never reach the infallible-alloc abort path that resets
//! the board mid-load
//! (`docs/defects/2026-08-29-load-project-resets-instead-of-refusing.md`).
//!
//! Companion to `project_read_refusal.rs`, which pins the same posture for
//! ProjectRead (ADR `2026-08-28-project-reads-bounded-streamed-refusable`).

extern crate alloc;

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use lp_gfx_lpvm::TargetLpvmGraphics;
use lpa_server::{LpGraphics, LpServer, PROJECT_LOAD_MIN_HEADROOM_BYTES};
use lpc_model::{AsLpPath, LpPathBuf};
use lpc_shared::output::MemoryOutputProvider;
use lpc_wire::{
    ClientMessage, ClientRequest, TransportError, WireMessage, WireServerMessage, WireServerMsgBody,
};
use lpfs::LpFsMemory;

#[test]
fn starved_heap_refuses_the_load_and_stays_alive() {
    let (mut server, project_path) = server_with_clock_project("load-refusal");

    // A probe reporting headroom below the gate: the load is refused with a
    // structured Error frame naming the free bytes and the remedy.
    server.set_read_headroom_probe(Some(|| Some(PROJECT_LOAD_MIN_HEADROOM_BYTES - 1)));

    let mut transport = VecTransport::default();
    let load = WireMessage::Client(ClientMessage {
        id: 51,
        msg: ClientRequest::LoadProject {
            path: String::from(project_path.as_str()),
        },
    });
    block_on(server.tick_and_send(16, vec![load], &mut transport)).expect("tick");

    assert_eq!(
        transport.sent.len(),
        1,
        "one response frame: {:?}",
        transport.sent
    );
    let frame = &transport.sent[0];
    assert_eq!(frame.id, 51);
    let WireServerMsgBody::Error { error } = &frame.msg else {
        panic!("expected an Error body, got {:?}", frame.msg);
    };
    assert!(
        error.contains("load refused")
            && error.contains("largest free block")
            && error.contains("smaller project"),
        "refusal names the free bytes and the remedy: {error}"
    );
    assert!(
        server.project_manager().list_loaded_projects().is_empty(),
        "a refused load leaves nothing loaded"
    );

    // The server survives: with the probe healthy again, the same request
    // loads normally.
    server.set_read_headroom_probe(Some(|| Some(u32::MAX)));
    let mut transport = VecTransport::default();
    let load = WireMessage::Client(ClientMessage {
        id: 52,
        msg: ClientRequest::LoadProject {
            path: String::from(project_path.as_str()),
        },
    });
    block_on(server.tick_and_send(16, vec![load], &mut transport)).expect("tick");
    assert!(
        matches!(
            transport.sent.as_slice(),
            [WireServerMessage {
                msg: WireServerMsgBody::LoadProject { .. },
                ..
            }]
        ),
        "healthy probe serves the load normally: {:?}",
        transport.sent
    );
}

#[test]
fn starved_heap_refuses_the_host_call_path_too() {
    // Boot-time startup loads use `LpServer::load_project` directly; the
    // same gate must refuse there so an unaffordable startup project boots
    // to an idle server instead of a reset loop.
    let (mut server, project_path) = server_with_clock_project("load-refusal-host");
    server.set_read_headroom_probe(Some(|| Some(PROJECT_LOAD_MIN_HEADROOM_BYTES - 1)));
    let error = server
        .load_project(project_path.as_path())
        .expect_err("starved heap refuses the host-call load");
    let message = alloc::format!("{error}");
    assert!(
        message.contains("load refused"),
        "host-call refusal carries the same shape: {message}"
    );

    server.set_read_headroom_probe(Some(|| Some(u32::MAX)));
    server
        .load_project(project_path.as_path())
        .expect("healthy probe loads normally");
}

#[test]
fn unset_probe_never_refuses_a_load() {
    let (mut server, project_path) = server_with_clock_project("load-no-probe");
    server
        .load_project(project_path.as_path())
        .expect("host embedders (no probe) are never refused");
}

/// In-memory transport that records every sent server message.
#[derive(Default)]
struct VecTransport {
    sent: Vec<WireServerMessage>,
}

impl lpc_shared::transport::ServerTransport for VecTransport {
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
            b"{\n  \"format\": 10\n}\n",
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
