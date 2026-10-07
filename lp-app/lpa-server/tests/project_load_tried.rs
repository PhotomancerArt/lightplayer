//! Loads are tried, not gated: a LoadProject is attempted whatever the heap
//! looks like, and a load that cannot finish either fails with a structured
//! error (and the previous project runs again: never dark) or, when it runs
//! the board out of memory, resets it with the load recorded so the next
//! boot runs the previous project again and says why (`lp-recovery`'s
//! `InterruptedLoad`; ADR `2026-08-28-project-reads-bounded-streamed-refusable`,
//! D7 as amended 2026-10-07). The blunt 64 KiB headroom gate this file used
//! to pin is gone (`docs/defects/2026-08-29-load-project-resets-instead-of-refusing.md`).

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
use lpc_shared::transport::{Incoming, Link, LinkId};

use lp_gfx_lpvm::TargetLpvmGraphics;
use lpa_server::{LpGraphics, LpServer};
use lpc_model::{AsLpPath, LpPathBuf};
use lpc_shared::output::MemoryOutputProvider;
use lpc_wire::{
    ClientMessage, ClientRequest, TransportError, WireServerMessage, WireServerMsgBody,
};
use lpfs::LpFsMemory;

#[test]
fn a_load_is_tried_whatever_the_headroom_probe_says() {
    let (mut server, project_path) = server_with_clock_project("load-tried");
    // The probe reports almost nothing free: the load is tried anyway (the
    // read gate, which still consults it, is not the load's business).
    server.set_read_headroom_probe(Some(|| Some(1024)));

    let mut transport = VecTransport::default();
    let load = Incoming::primary(ClientMessage {
        id: 51,
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
        "the load is served: {:?}",
        transport.sent
    );

    // The host-call path (the boot's startup load) is not gated either.
    let (mut server, project_path) = server_with_clock_project("load-tried-host");
    server.set_read_headroom_probe(Some(|| Some(1024)));
    server
        .load_startup_project(project_path.as_path())
        .expect("the startup load is tried");
}

/// Never dark (G1 desk walk): a switch that fails without a reset runs the
/// project that was running before, whether the load itself unloaded it or
/// a StopAllProjects just before it did (what an upload sends), and the
/// error says so.
#[test]
fn a_failed_switch_leaves_the_previous_project_running() {
    let (mut server, project_path) = server_with_clock_project("load-failed-restore");
    server
        .load_project(project_path.as_path())
        .expect("the first project loads");

    for stop_first in [false, true] {
        let mut requests = Vec::new();
        if stop_first {
            requests.push(Incoming::primary(ClientMessage {
                id: 60,
                msg: ClientRequest::StopAllProjects,
            }));
        }
        requests.push(Incoming::primary(ClientMessage {
            id: 61,
            msg: ClientRequest::LoadProject {
                path: String::from("/projects/not-a-project"),
            },
        }));
        let mut transport = VecTransport::default();
        block_on(server.tick_and_send(16, requests, &mut transport)).expect("tick");
        let failure = transport
            .sent
            .iter()
            .find(|frame| frame.id == 61)
            .expect("the load is answered");
        let WireServerMsgBody::Error { error } = &failure.msg else {
            panic!("expected an Error body, got {:?}", failure.msg);
        };
        assert!(
            error.contains("is running again"),
            "stop first: {stop_first}: {error}"
        );
        assert_eq!(
            server.project_manager().list_loaded_projects().len(),
            1,
            "stop first: {stop_first}: the previous project runs again"
        );
    }
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
            b"{\n  \"format\": 11\n}\n",
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
