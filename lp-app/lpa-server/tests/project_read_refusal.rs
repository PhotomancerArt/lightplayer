//! Refusal-not-reset: a ProjectRead the device cannot afford fails with a
//! structured terminal error on the request id and leaves the connection
//! (and the server) fully alive — it must never reach the infallible-alloc
//! abort path that resets the board
//! (`docs/defects/2026-08-26-project-read-assembly-oom-resets-classic.md`).

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
use lpa_server::{LpGraphics, LpServer, MemoryStatsFn, ReadGate};
use lpc_model::{AsLpPath, LpPathBuf};
use lpc_shared::output::MemoryOutputProvider;
use lpc_wire::{
    ClientMessage, ClientRequest, NodeReadQuery, ProjectReadEvent, ProjectReadQuery,
    ProjectReadRequest, TransportError, WireProjectHandle, WireServerMessage, WireServerMsgBody,
};
use lpfs::LpFsMemory;

#[test]
fn a_small_largest_block_refuses_the_read_and_stays_alive() {
    let (mut server, project_path) = server_with_clock_project("read-refusal", Some(plenty_free));
    let handle = server.load_project(project_path.as_path()).expect("load");
    server.set_read_gate(Some(GATE));

    // Plenty free in total, but no block big enough: refused.
    server.set_read_headroom_probe(Some(|| Some(GATE.min_largest_block_bytes - 1)));
    let sent = read(&mut server, handle, 41);

    // Exactly one terminal frame for the request id, carrying a
    // ProjectReadEvent::Error that says the board is busy and to retry.
    assert_eq!(sent.len(), 1, "one terminal frame: {sent:?}");
    let frame = &sent[0];
    assert_eq!(frame.id, 41);
    assert!(frame.fin, "refusal frame is final");
    let message = refusal_message(&sent).expect("a refusal");
    assert!(
        message.starts_with("read refused: board memory busy")
            && message.contains("retry shortly")
            && !message.contains("narrow the query"),
        "refusal message says busy and transient: {message}"
    );

    // The connection survives: with the probe healthy again, the same server
    // answers the same read normally.
    server.set_read_headroom_probe(Some(|| Some(u32::MAX)));
    let sent = read(&mut server, handle, 42);
    assert_served(&sent);
}

#[test]
fn low_total_free_refuses_even_with_a_big_block() {
    let (mut server, project_path) = server_with_clock_project("read-low-free", Some(short_free));
    let handle = server.load_project(project_path.as_path()).expect("load");
    server.set_read_gate(Some(GATE));
    server.set_read_headroom_probe(Some(|| Some(u32::MAX)));

    let sent = read(&mut server, handle, 43);
    let message = refusal_message(&sent).expect("a refusal");
    assert!(
        message.contains(&format!("free {} B", GATE.min_free_bytes - 1)),
        "refusal names the total free: {message}"
    );
}

/// The prod choker's refusals (2026-09-26/27): ~90 KB free, a 19,480 B
/// largest block. The old single floor (a 32 KiB block) refused it; the
/// two-number gate serves it.
#[test]
fn a_fragmented_heap_with_room_in_total_is_served() {
    let (mut server, project_path) =
        server_with_clock_project("read-fragmented", Some(plenty_free));
    let handle = server.load_project(project_path.as_path()).expect("load");
    server.set_read_gate(Some(GATE));
    server.set_read_headroom_probe(Some(|| Some(19_480)));

    let sent = read(&mut server, handle, 44);
    assert_served(&sent);
}

#[test]
fn a_probe_without_a_gate_never_refuses() {
    let (mut server, project_path) = server_with_clock_project("read-no-gate", Some(short_free));
    let handle = server.load_project(project_path.as_path()).expect("load");
    // The probe alone feeds the load gate and the heartbeat, not reads.
    server.set_read_headroom_probe(Some(|| Some(1)));

    let sent = read(&mut server, handle, 45);
    assert_served(&sent);
}

#[test]
fn unset_probe_never_refuses() {
    let (mut server, project_path) = server_with_clock_project("read-no-probe", None);
    let handle = server.load_project(project_path.as_path()).expect("load");

    let sent = read(&mut server, handle, 7);
    assert!(
        refusal_message(&sent).is_none(),
        "host embedders (no probe) are never refused"
    );
}

/// The C6's numbers (`fw-esp32c6/src/main.rs` `READ_GATE`).
const GATE: ReadGate = ReadGate {
    min_free_bytes: 40 * 1024,
    min_largest_block_bytes: 16 * 1024,
};

fn plenty_free() -> Option<(u32, u32)> {
    Some((90_000, 200_000))
}

fn short_free() -> Option<(u32, u32)> {
    Some((GATE.min_free_bytes - 1, 250_000))
}

/// Sends one full node read and returns every frame the server sent for it.
fn read(server: &mut LpServer, handle: WireProjectHandle, id: u64) -> Vec<WireServerMessage> {
    let mut transport = VecTransport::default();
    let read = Incoming::primary(ClientMessage {
        id,
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
    transport.sent
}

fn project_read_events(sent: &[WireServerMessage]) -> Vec<&ProjectReadEvent> {
    sent.iter()
        .filter_map(|frame| match &frame.msg {
            WireServerMsgBody::ProjectRead { events } => Some(events.iter()),
            _ => None,
        })
        .flatten()
        .collect()
}

/// The refusal's text, when the read was refused (a lone terminal `Error`).
fn refusal_message(sent: &[WireServerMessage]) -> Option<String> {
    match project_read_events(sent).as_slice() {
        [ProjectReadEvent::Error { message }] => Some(message.clone()),
        _ => None,
    }
}

fn assert_served(sent: &[WireServerMessage]) {
    let served = project_read_events(sent);
    assert!(
        served
            .iter()
            .any(|event| matches!(event, ProjectReadEvent::Begin { .. })),
        "the read is served: {served:?}"
    );
    assert!(
        !served
            .iter()
            .any(|event| matches!(event, ProjectReadEvent::Error { .. })),
        "no refusal: {served:?}"
    );
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

fn server_with_clock_project(
    name: &str,
    memory_stats: Option<MemoryStatsFn>,
) -> (LpServer, LpPathBuf) {
    let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::new()));
    let graphics: Arc<dyn LpGraphics> =
        Arc::new(TargetLpvmGraphics::new(lpa_server::DEVICE_SHADER_FRONTEND));
    let mut server = LpServer::new(
        output_provider,
        Box::new(LpFsMemory::new()),
        "projects".as_path(),
        memory_stats,
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
