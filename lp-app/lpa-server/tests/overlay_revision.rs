//! Overlay revision surfaces on the wire: mutation/read/commit responses carry
//! the overlay's `changed_at`, and every project read's runtime status reports
//! it (mutate → read → observe bump).

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

use lpa_server::{Graphics, LpGraphics, LpServer, Project};
use lpc_model::{
    ArtifactLocation, AsLpPath, LpPathBuf, LpValue, MutationCmd, MutationCmdBatch, MutationCmdId,
    MutationOp, Revision, SlotEdit, SlotPath,
};
use lpc_shared::output::MemoryOutputProvider;
use lpc_shared::transport::ServerTransport;
use lpc_wire::{
    ClientMessage, ClientRequest, ProjectReadEvent, ProjectReadQuery, ProjectReadQueryEvent,
    ProjectReadRequest, RuntimeReadQuery, TransportError, WireMessage, WireOverlayCommitRequest,
    WireOverlayMutationRequest, WireProjectHandle, WireServerMessage, WireServerMsgBody,
};
use lpfs::LpFsMemory;

#[test]
fn mutation_and_read_responses_carry_overlay_revision() {
    let (mut server, project_path) = server_with_clock_project("overlay-rev-mutate");
    let handle = server.load_project(project_path.as_path()).expect("load");

    // Advance frames so the mutation lands at a revision strictly newer than
    // the fresh overlay's default `changed_at`.
    server.advance_frame(16).expect("tick");
    server.advance_frame(16).expect("tick");

    let project = project_mut(&mut server, handle);
    let before = project.registry().overlay().changed_at();
    assert_eq!(before, Revision::default(), "fresh overlay is at zero");

    let response = project
        .mutate_overlay(rate_mutation(1, 3.0))
        .expect("mutate overlay");

    let after = project.registry().overlay().changed_at();
    assert!(after > before, "overlay revision advanced");
    assert_eq!(
        response.overlay_revision, after,
        "mutation response carries the post-mutation overlay revision"
    );

    let read = project.read_overlay();
    assert_eq!(
        read.revision, after,
        "overlay read response carries the overlay revision"
    );
    assert!(!read.overlay.is_empty(), "overlay holds the pending edit");
}

#[test]
fn commit_response_carries_overlay_revision() {
    let (mut server, project_path) = server_with_clock_project("overlay-rev-commit");
    let handle = server.load_project(project_path.as_path()).expect("load");
    server.advance_frame(16).expect("tick");

    let project = project_mut(&mut server, handle);
    project
        .mutate_overlay(rate_mutation(1, 3.0))
        .expect("mutate overlay");
    let mutated_at = project.registry().overlay().changed_at();

    let response = project
        .commit_overlay(WireOverlayCommitRequest)
        .expect("commit overlay");

    let after = project.registry().overlay().changed_at();
    assert_eq!(
        response.overlay_revision, after,
        "commit response carries the post-commit overlay revision"
    );
    assert!(
        after >= mutated_at,
        "post-commit overlay revision does not regress"
    );
}

#[test]
fn runtime_status_reports_overlay_revision_bump() {
    let (mut server, project_path) = server_with_clock_project("overlay-rev-runtime");
    let handle = server.load_project(project_path.as_path()).expect("load");
    server.advance_frame(16).expect("tick");

    // Before any mutation: runtime status reports the zero overlay revision.
    assert_eq!(
        runtime_overlay_changed_at(&mut server, handle, 1),
        Revision::default(),
        "runtime status reports zero for a fresh overlay"
    );

    let mutation_revision = project_mut(&mut server, handle)
        .mutate_overlay(rate_mutation(1, 2.5))
        .expect("mutate overlay")
        .overlay_revision;
    assert!(mutation_revision > Revision::default());

    // After the mutation: the streamed read's runtime status reports the bump.
    assert_eq!(
        runtime_overlay_changed_at(&mut server, handle, 2),
        mutation_revision,
        "runtime status reports the post-mutation overlay revision"
    );
}

/// Run a runtime-only project read through the streaming transport path and
/// return the `overlay_changed_at` its runtime status reports.
fn runtime_overlay_changed_at(
    server: &mut LpServer,
    handle: WireProjectHandle,
    msg_id: u64,
) -> Revision {
    let request = ProjectReadRequest {
        since: None,
        queries: vec![ProjectReadQuery::Runtime(RuntimeReadQuery)],
        probes: Vec::new(),
    };
    let message = WireMessage::Client(ClientMessage {
        id: msg_id,
        msg: ClientRequest::ProjectRead { handle, request },
    });
    let mut transport = VecTransport::default();
    block_on(server.tick_and_send(16, vec![message], &mut transport)).expect("tick_and_send");

    transport
        .sent
        .iter()
        .filter_map(|msg| match &msg.msg {
            WireServerMsgBody::ProjectRead { events } => Some(events.iter()),
            _ => None,
        })
        .flatten()
        .find_map(|event| match event {
            ProjectReadEvent::Query {
                event: ProjectReadQueryEvent::Runtime(result),
                ..
            } => Some(result.project.overlay_changed_at),
            _ => None,
        })
        .expect("runtime status present in project read stream")
}

fn rate_mutation(id: u64, rate: f32) -> WireOverlayMutationRequest {
    WireOverlayMutationRequest::new(MutationCmdBatch::new(vec![MutationCmd {
        id: MutationCmdId::new(id),
        mutation: MutationOp::PutSlotEdit {
            artifact: ArtifactLocation::file("/clock.json"),
            edit: SlotEdit::assign_value(
                SlotPath::parse("controls.rate").expect("rate path"),
                LpValue::F32(rate),
            ),
        },
    }]))
}

fn project_mut(server: &mut LpServer, handle: WireProjectHandle) -> &mut Project {
    server
        .project_manager_mut()
        .get_project_mut(handle)
        .expect("loaded project")
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

fn server_with_clock_project(name: &str) -> (LpServer, LpPathBuf) {
    let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::new()));
    let graphics: Arc<dyn LpGraphics> = Arc::new(Graphics::new());
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
            br#"
{
  "kind": "Project",
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
  "controls": {
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
