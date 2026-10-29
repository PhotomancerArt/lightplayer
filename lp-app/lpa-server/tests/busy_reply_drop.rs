//! A reply the link drops ends only that reply, not the tick.
//!
//! `UsbLinkTransport` gives up on a reply the host is too slow to take: it
//! queues a drop notice and fails the send with `host link busy`. The tick
//! that sent it must still answer the rest of its requests and render its
//! frame (fs changes reaching projects, the load commit and a queued power-off
//! all ride that frame). A transport that reports `ConnectionLost` has no
//! peer to answer: the tick stops answering, and still renders.

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
use lpa_server::{LpGraphics, LpServer};
use lpc_model::{AsLpPath, LpPathBuf};
use lpc_shared::output::MemoryOutputProvider;
use lpc_shared::transport::{Incoming, Link, LinkId, ServerTransport};
use lpc_wire::server::FsRequest;
use lpc_wire::{
    ClientMessage, ClientRequest, TransportError, WireProjectHandle, WireServerMessage,
};
use lpfs::LpFsMemory;

#[test]
fn a_busy_drop_of_one_reply_leaves_the_rest_of_the_tick() {
    let (mut server, handle, clock) = clock_server("busy-drop");
    server.set_messages_first(true);
    assert!(!write_pending(&server, handle), "nothing pending before");
    let mut transport = FailFirstSendTransport::new(TransportError::Other(
        "host link busy: the reply before it is not out (12000 B held)".into(),
    ));

    let result = block_on(server.tick_and_send(16, write_and_list(clock), &mut transport));

    assert_eq!(result.expect("a dropped reply does not fail the tick"), 2);
    assert_eq!(
        transport.attempted,
        vec![1, 2],
        "both requests were answered"
    );
    assert_eq!(transport.sent, vec![2], "id 1's reply was the one dropped");
    assert!(
        !write_pending(&server, handle),
        "the tick's frame ran and took the write in"
    );
}

#[test]
fn a_lost_connection_stops_the_answers_but_not_the_frame() {
    let (mut server, handle, clock) = clock_server("connection-lost");
    server.set_messages_first(true);
    let mut transport = FailFirstSendTransport::new(TransportError::ConnectionLost);

    let result = block_on(server.tick_and_send(16, write_and_list(clock), &mut transport));

    result.expect("a lost connection does not fail the tick");
    assert_eq!(
        transport.attempted,
        vec![1],
        "nothing is sent once the connection is gone"
    );
    assert!(
        !write_pending(&server, handle),
        "the tick's frame still ran and took the write in"
    );
}

/// A rewrite of the clock file, then a list: two requests, two replies.
fn write_and_list(clock: LpPathBuf) -> Vec<Incoming> {
    vec![
        Incoming::primary(ClientMessage {
            id: 1,
            msg: ClientRequest::Filesystem(FsRequest::Write {
                path: clock,
                data: clock_json(3.0),
            }),
        }),
        Incoming::primary(ClientMessage {
            id: 2,
            msg: ClientRequest::ListAvailableProjects,
        }),
    ]
}

/// Whether the filesystem holds a change the project's frames have not taken
/// in yet.
fn write_pending(server: &LpServer, handle: WireProjectHandle) -> bool {
    let project = server
        .project_manager()
        .get_project(handle)
        .expect("loaded project");
    !server
        .base_fs()
        .get_changes_since(project.last_fs_version())
        .is_empty()
}

/// A server with a one-clock project loaded and settled. Returns the server,
/// the project's handle and the clock file's path.
fn clock_server(name: &str) -> (LpServer, WireProjectHandle, LpPathBuf) {
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
    let clock = project_path.join("clock.json");
    let fs = server.base_fs_mut();
    fs.write_file(
        project_path.join("project.json").as_path(),
        b"{\n  \"format\": 12\n}\n",
    )
    .expect("write container manifest");
    fs.write_file(
        project_path.join("module.json").as_path(),
        br#"{ "kind": "Module", "nodes": { "clock": { "ref": "./clock.json" } } }"#,
    )
    .expect("write module");
    fs.write_file(clock.as_path(), &clock_json(1.0))
        .expect("write clock");

    let handle = server.load_project(project_path.as_path()).expect("load");
    server.advance_frame(16).expect("settle");
    (server, handle, clock)
}

fn clock_json(rate: f32) -> Vec<u8> {
    alloc::format!(r#"{{ "kind": "Clock", "transport": {{ "rate": {rate:?} }} }}"#).into_bytes()
}

/// Fails its first send with `error` (for a busy drop, the way
/// `UsbLinkTransport` fails a reply the host is too slow to take) and takes
/// every later one.
struct FailFirstSendTransport {
    error: TransportError,
    /// Ids of every reply the server tried to send.
    attempted: Vec<u64>,
    /// Ids of the replies that went out.
    sent: Vec<u64>,
}

impl FailFirstSendTransport {
    fn new(error: TransportError) -> Self {
        Self {
            error,
            attempted: Vec::new(),
            sent: Vec::new(),
        }
    }
}

impl ServerTransport for FailFirstSendTransport {
    async fn send(&mut self, _link: LinkId, msg: WireServerMessage) -> Result<(), TransportError> {
        self.attempted.push(msg.id);
        if self.attempted.len() == 1 {
            return Err(self.error.clone());
        }
        self.sent.push(msg.id);
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

/// Drive a future to completion with a no-op waker.
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
