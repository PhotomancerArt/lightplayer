//! The two orders `tick_and_send` can work in ([`LpServer::set_messages_first`]).
//!
//! A loaded project, and a request that rewrites one of its files arriving
//! with a tick. A frame is where a project takes in filesystem changes, so
//! the question "did this tick's frame see the write?" has a direct answer:
//! is the write still pending for the project after the tick?
//!
//! Off (the default), the frame renders first and the write lands after it:
//! still pending, seen by the next frame. On, the write is answered first
//! and the same tick's frame takes it in. Messages-first also yields exactly
//! once between the replies and the render, and only when a reply was sent.

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
use lpc_wire::server::{FsRequest, FsResponse};
use lpc_wire::{
    ClientMessage, ClientRequest, TransportError, WireProjectHandle, WireServerMessage,
    WireServerMsgBody,
};
use lpfs::LpFsMemory;

#[test]
fn default_order_renders_before_answering() {
    let (pending, polls) = write_in_a_tick(false);
    assert!(
        pending,
        "render-first: the tick's frame ran before the write landed"
    );
    assert_eq!(polls, 1, "render-first never yields");
}

#[test]
fn messages_first_answers_before_rendering() {
    let (pending, polls) = write_in_a_tick(true);
    assert!(
        !pending,
        "messages-first: the same tick's frame took the write in"
    );
    assert_eq!(
        polls, 2,
        "messages-first yields once between the replies and the render"
    );
}

#[test]
fn messages_first_without_requests_does_not_yield() {
    let (mut server, _handle, _clock) = clock_server("messages-first-idle");
    server.set_messages_first(true);
    let mut transport = VecTransport::default();
    let (count, polls) = block_on_counting(server.tick_and_send(16, Vec::new(), &mut transport));
    assert_eq!(count.expect("tick"), 0);
    assert_eq!(polls, 1, "no reply, no yield");
}

/// Load a project, then run one tick carrying a rewrite of its clock file.
/// Returns whether the write is still pending for the project after the tick
/// (its frame ran before the write), and how many polls the tick took
/// (1 = it never yielded).
fn write_in_a_tick(messages_first: bool) -> (bool, usize) {
    let (mut server, handle, clock) = clock_server(match messages_first {
        true => "messages-first-on",
        false => "messages-first-off",
    });
    server.set_messages_first(messages_first);
    assert!(!write_pending(&server, handle), "nothing pending before");

    let mut transport = VecTransport::default();
    let write = Incoming::primary(ClientMessage {
        id: 1,
        msg: ClientRequest::Filesystem(FsRequest::Write {
            path: clock,
            data: clock_json(3.0),
        }),
    });
    let (count, polls) = block_on_counting(server.tick_and_send(16, vec![write], &mut transport));
    assert_eq!(count.expect("tick_and_send"), 1, "the write was answered");
    assert!(
        matches!(
            transport.sent.as_slice(),
            [WireServerMessage {
                msg: WireServerMsgBody::Filesystem(FsResponse::Write { error: None, .. }),
                ..
            }]
        ),
        "the write succeeded: {:?}",
        transport.sent
    );

    (write_pending(&server, handle), polls)
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
        b"{\n  \"format\": 11\n}\n",
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

/// In-memory transport that records every sent server message.
#[derive(Default)]
struct VecTransport {
    sent: Vec<WireServerMessage>,
}

impl ServerTransport for VecTransport {
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

/// Drive a future to completion with a no-op waker, counting the polls.
fn block_on_counting<F: Future>(future: F) -> (F::Output, usize) {
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    let mut polls = 0;
    loop {
        polls += 1;
        if let Poll::Ready(output) = Future::poll(Pin::as_mut(&mut future), &mut cx) {
            return (output, polls);
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
