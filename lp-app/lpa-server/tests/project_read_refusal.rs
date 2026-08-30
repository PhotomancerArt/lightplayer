//! Refusal-not-reset: a ProjectRead the device cannot afford fails with a
//! structured terminal error on the request id and leaves the connection
//! (and the server) fully alive — it must never reach the infallible-alloc
//! abort path that resets the board
//! (`docs/defects/2026-08-26-project-read-assembly-oom-resets-classic.md`).

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

use lp_gfx_lpvm::TargetLpvmGraphics;
use lpa_server::{LpGraphics, LpServer, PROJECT_READ_MIN_HEADROOM_BYTES};
use lpc_model::{AsLpPath, LpPathBuf};
use lpc_shared::output::MemoryOutputProvider;
use lpc_wire::{
    ClientMessage, ClientRequest, NodeReadQuery, ProjectReadEvent, ProjectReadQuery,
    ProjectReadRequest, TransportError, WireMessage, WireServerMessage, WireServerMsgBody,
};
use lpfs::LpFsMemory;

#[test]
fn starved_heap_refuses_the_read_and_stays_alive() {
    let (mut server, project_path) = server_with_clock_project("read-refusal");
    let handle = server.load_project(project_path.as_path()).expect("load");

    // A probe reporting headroom below the gate: every read is refused.
    server.set_read_headroom_probe(Some(|| Some(PROJECT_READ_MIN_HEADROOM_BYTES - 1)));

    let mut transport = VecTransport::default();
    let read = WireMessage::Client(ClientMessage {
        id: 41,
        msg: ClientRequest::ProjectRead {
            handle,
            request: ProjectReadRequest {
                since: None,
                queries: vec![ProjectReadQuery::Nodes(NodeReadQuery::detail_all())],
                probes: Vec::new(),
            },
        },
    });
    block_on(server.tick_and_send(16, vec![read], &mut transport)).expect("tick");

    // Exactly one terminal frame for the request id, carrying a
    // ProjectReadEvent::Error whose message names the remedy.
    assert_eq!(
        transport.sent.len(),
        1,
        "one terminal frame: {:?}",
        transport.sent
    );
    let frame = &transport.sent[0];
    assert_eq!(frame.id, 41);
    assert!(frame.fin, "refusal frame is final");
    let WireServerMsgBody::ProjectRead { events } = &frame.msg else {
        panic!("expected a ProjectRead body, got {:?}", frame.msg);
    };
    let [ProjectReadEvent::Error { message }] = events.as_slice() else {
        panic!("expected a single terminal Error event, got {events:?}");
    };
    assert!(
        message.contains("read refused") && message.contains("narrow the query"),
        "refusal message names the remedy: {message}"
    );

    // The connection survives: with the probe healthy again, the same server
    // answers the same read normally.
    server.set_read_headroom_probe(Some(|| Some(u32::MAX)));
    let mut transport = VecTransport::default();
    let read = WireMessage::Client(ClientMessage {
        id: 42,
        msg: ClientRequest::ProjectRead {
            handle,
            request: ProjectReadRequest {
                since: None,
                queries: vec![ProjectReadQuery::Nodes(NodeReadQuery::detail_all())],
                probes: Vec::new(),
            },
        },
    });
    block_on(server.tick_and_send(16, vec![read], &mut transport)).expect("tick");
    let served: Vec<&ProjectReadEvent> = transport
        .sent
        .iter()
        .filter_map(|frame| match &frame.msg {
            WireServerMsgBody::ProjectRead { events } => Some(events.iter()),
            _ => None,
        })
        .flatten()
        .collect();
    assert!(
        served
            .iter()
            .any(|event| matches!(event, ProjectReadEvent::Begin { .. })),
        "healthy probe serves the read normally: {served:?}"
    );
    assert!(
        !served
            .iter()
            .any(|event| matches!(event, ProjectReadEvent::Error { .. })),
        "no refusal once headroom is healthy"
    );
}

#[test]
fn unset_probe_never_refuses() {
    let (mut server, project_path) = server_with_clock_project("read-no-probe");
    let handle = server.load_project(project_path.as_path()).expect("load");

    let mut transport = VecTransport::default();
    let read = WireMessage::Client(ClientMessage {
        id: 7,
        msg: ClientRequest::ProjectRead {
            handle,
            request: ProjectReadRequest {
                since: None,
                queries: vec![ProjectReadQuery::Nodes(NodeReadQuery::detail_all())],
                probes: Vec::new(),
            },
        },
    });
    block_on(server.tick_and_send(16, vec![read], &mut transport)).expect("tick");
    let has_error = transport.sent.iter().any(|frame| {
        matches!(
            &frame.msg,
            WireServerMsgBody::ProjectRead { events }
                if events.iter().any(|e| matches!(e, ProjectReadEvent::Error { .. }))
        )
    });
    assert!(!has_error, "host embedders (no probe) are never refused");
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
