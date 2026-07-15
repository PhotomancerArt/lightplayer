//! Every client request id gets exactly one response frame, even when the
//! handler fails. Regression test: a failed project load used to propagate out
//! of `tick_and_send` before a response was sent, so the server logged the
//! error and the client awaited forever.

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
use lpc_model::AsLpPath;
use lpc_shared::output::MemoryOutputProvider;
use lpc_shared::transport::ServerTransport;
use lpc_wire::{
    ClientMessage, ClientRequest, TransportError, WireMessage, WireServerMessage, WireServerMsgBody,
};
use lpfs::LpFsMemory;

#[test]
fn failed_load_project_gets_error_response_and_later_requests_still_answer() {
    let mut server = memory_server();
    // Manifest missing `format: 1` fails to load server-side (the original
    // repro from the device-link e2e tests).
    server
        .base_fs_mut()
        .write_file(
            "/projects/bad/project.json".as_path(),
            br#"{ "kind": "Project", "nodes": {} }"#,
        )
        .expect("write bad manifest");

    let messages = vec![
        WireMessage::Client(ClientMessage {
            id: 7,
            msg: ClientRequest::LoadProject {
                path: "/projects/bad".into(),
            },
        }),
        WireMessage::Client(ClientMessage {
            id: 8,
            msg: ClientRequest::ListAvailableProjects,
        }),
    ];

    let mut transport = VecTransport::default();
    let response_count = block_on(server.tick_and_send(16, messages, &mut transport))
        .expect("handler failure must not abort the tick");

    assert_eq!(response_count, 2, "both requests answered");
    assert_eq!(transport.sent.len(), 2);

    assert_eq!(transport.sent[0].id, 7);
    assert!(
        matches!(transport.sent[0].msg, WireServerMsgBody::Error { .. }),
        "failed load answers with an Error body, got {:?}",
        transport.sent[0].msg
    );

    assert_eq!(
        transport.sent[1].id, 8,
        "later request in the same batch still answered"
    );
    assert!(
        matches!(
            transport.sent[1].msg,
            WireServerMsgBody::ListAvailableProjects { .. }
        ),
        "expected ListAvailableProjects response, got {:?}",
        transport.sent[1].msg
    );
}

#[test]
fn load_of_nonexistent_project_gets_error_response() {
    let mut server = memory_server();

    let messages = vec![WireMessage::Client(ClientMessage {
        id: 3,
        msg: ClientRequest::LoadProject {
            path: "/projects/does-not-exist".into(),
        },
    })];

    let mut transport = VecTransport::default();
    block_on(server.tick_and_send(16, messages, &mut transport))
        .expect("handler failure must not abort the tick");

    assert_eq!(transport.sent.len(), 1);
    assert_eq!(transport.sent[0].id, 3);
    assert!(matches!(
        transport.sent[0].msg,
        WireServerMsgBody::Error { .. }
    ));
}

fn memory_server() -> LpServer {
    let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::new()));
    let graphics: Arc<dyn LpGraphics> =
        Arc::new(TargetLpvmGraphics::new(lpa_server::DEVICE_SHADER_FRONTEND));
    LpServer::new(
        output_provider,
        Box::new(LpFsMemory::new()),
        "/projects/".as_path(),
        None,
        None,
        graphics,
    )
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
