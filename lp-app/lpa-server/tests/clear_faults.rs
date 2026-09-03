//! `ClientRequest::ClearFaults` dispatch: both halves, and an ack that does
//! not over-claim.
//!
//! The verb has to do two things at once — forget the crash-recovery ledger
//! and re-arm the engine's faulted nodes — because neither alone is a retry.
//! A re-armed node whose path is still gated faults again on the next tick,
//! and a cleared ledger under latched node statuses changes nothing anybody
//! can see.
//!
//! The ack reports whether there was a LEDGER to forget, and only that: a
//! host server (and the browser sim) installs no recovery region at all, so
//! `false` there is the honest answer rather than a failure.
//!
//! `examples/fault-demo` is the engine-side subject for the same reason the
//! heartbeat test uses it: a shader that compiles and then traps on fuel
//! every frame, deterministic on every backend and incapable of crashing a
//! board.

extern crate alloc;

use alloc::rc::Rc;
use alloc::sync::Arc;
use core::cell::RefCell;
use std::path::{Path, PathBuf};

use lp_gfx_lpvm::TargetLpvmGraphics;
use lp_recovery::{
    CrashCause, FrameKind, InMemoryBackend, Recovery, RecoveryHandle, RecoveryLevel, ResetCause,
};
use lpa_server::{LpGraphics, LpServer, handlers::handle_client_message};
use lpc_model::{AsLpPath, AsLpPathBuf};
use lpc_shared::output::MemoryOutputProvider;
use lpc_wire::messages::{ClientMessage, ClientRequest};
use lpfs::{LpFsMemory, LpFsStd};

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lpa-server lives two levels under the workspace root")
        .parent()
        .expect("workspace dir")
        .to_path_buf()
}

fn graphics() -> Arc<dyn LpGraphics> {
    Arc::new(TargetLpvmGraphics::new(lpa_server::DEVICE_SHADER_FRONTEND))
}

/// A server whose project store IS the checked-in `examples/` directory.
fn server_over_examples() -> (
    LpServer,
    Rc<RefCell<dyn lpc_shared::output::OutputProvider>>,
) {
    let output_provider: Rc<RefCell<dyn lpc_shared::output::OutputProvider>> =
        Rc::new(RefCell::new(MemoryOutputProvider::new()));
    let base_fs = Box::new(LpFsStd::new(workspace_dir().join("examples")));
    let server = LpServer::new(
        output_provider.clone(),
        base_fs,
        "/".as_path(),
        None,
        None,
        graphics(),
    );
    (server, output_provider)
}

fn empty_server() -> (
    LpServer,
    Rc<RefCell<dyn lpc_shared::output::OutputProvider>>,
) {
    let output_provider: Rc<RefCell<dyn lpc_shared::output::OutputProvider>> =
        Rc::new(RefCell::new(MemoryOutputProvider::new()));
    let server = LpServer::new(
        output_provider.clone(),
        Box::new(LpFsMemory::new()),
        "projects/".as_path(),
        None,
        None,
        graphics(),
    );
    (server, output_provider)
}

fn load(
    server: &mut LpServer,
    output: Rc<RefCell<dyn lpc_shared::output::OutputProvider>>,
    name: &str,
) {
    let server_ptr: *mut LpServer = server;
    // SAFETY: project_manager and base_fs are disjoint fields of `server`;
    // nothing else touches `server` for the duration (the established
    // pattern in this crate's handler tests).
    unsafe {
        let pm = (*server_ptr).project_manager_mut();
        let fs = (*server_ptr).base_fs_mut();
        pm.load_project(
            &"/".as_path_buf().join(name),
            fs,
            output,
            None,
            None,
            None,
            None,
            graphics(),
        )
        .expect("the example loads");
    }
}

/// Dispatch one `ClearFaults` request through the real handler.
fn clear_faults(
    server: &mut LpServer,
    output_provider: &Rc<RefCell<dyn lpc_shared::output::OutputProvider>>,
) -> lpc_wire::WireServerMessage {
    let request = ClientMessage {
        id: 4,
        msg: ClientRequest::ClearFaults,
    };
    let server_ptr: *mut LpServer = server;
    // SAFETY: as in `load`.
    unsafe {
        let pm = (*server_ptr).project_manager_mut();
        let fs = (*server_ptr).base_fs_mut();
        handle_client_message(
            pm,
            fs,
            output_provider,
            None,
            None,
            false,
            None,
            None,
            None,
            graphics(),
            (*server_ptr).hello(),
            lpa_server::handlers::EngineLinkState::default(),
            request,
        )
    }
    .expect("clear faults is always answerable")
}

fn ledger_cleared(response: &lpc_wire::WireServerMessage) -> bool {
    match response.msg {
        lpc_wire::server::ServerMsgBody::ClearFaults { ledger_cleared } => ledger_cleared,
        ref other => panic!("expected the ClearFaults ack, got {other:?}"),
    }
}

/// The engine half, on a project that really is faulted. The verb reaches
/// EVERY loaded engine, not one a handle names: a fault is a device-level
/// condition and the verb the user pressed is on the device card.
#[test]
fn clearing_faults_re_arms_every_loaded_engine() {
    let (mut server, output) = server_over_examples();
    load(&mut server, output.clone(), "fault-demo");

    // Warm past the compile-window deferral: the first render only REQUESTS
    // a compile window, so the trap cannot happen until the compile has.
    for _ in 0..5 {
        let _ = server.advance_frame(16);
    }
    assert!(
        server.project_manager().list_loaded_projects_with_faults()[0]
            .fault
            .is_some(),
        "the subject must be faulted before it can be cleared"
    );

    let response = clear_faults(&mut server, &output);
    assert_eq!(response.id, 4, "the ack is correlated");

    assert!(
        server.project_manager().list_loaded_projects_with_faults()[0]
            .fault
            .is_none(),
        "the verdict is dropped so the next tick re-derives it rather than \
         inheriting a stale one"
    );

    // And the honest outcome: fault-demo traps every frame, so it faults
    // again immediately. Clearing forgives; it does not fix.
    let _ = server.advance_frame(16);
    assert!(
        server.project_manager().list_loaded_projects_with_faults()[0]
            .fault
            .is_some(),
        "a failure that is still there must come straight back"
    );
}

/// Nothing resets and nothing is retried inside the handler — the contrast
/// with `Reboot`, which the embedder honors after the ack is written. Here
/// the ack IS the whole request path.
#[test]
fn clearing_faults_is_answerable_with_nothing_loaded() {
    let (mut server, output) = empty_server();

    let response = clear_faults(&mut server, &output);

    assert!(matches!(
        response.msg,
        lpc_wire::server::ServerMsgBody::ClearFaults { .. }
    ));
    assert!(server.project_manager().list_loaded_projects().is_empty());
}

/// The ledger half, and the reason the ack carries a boolean at all.
///
/// One test function because the recovery global is process-wide and cargo
/// runs tests in parallel: the uninstalled case has to be observed before
/// anything installs one (the same shape as `lpc-engine`'s
/// `recovery_gating.rs`).
#[test]
fn the_ack_says_whether_there_was_a_ledger_to_forget() {
    let (mut server, output) = empty_server();

    // Host and browser servers install no recovery region. Saying "cleared"
    // there would be a claim about nothing.
    assert!(
        !ledger_cleared(&clear_faults(&mut server, &output)),
        "no recovery global means no ledger to forget"
    );

    // A device-shaped global with a path gated red — the bench case: two
    // crashes on one path, and nothing but a power cycle to lift it.
    let (mut recovery, _) = Recovery::init(InMemoryBackend::new(), ResetCause::PowerOn);
    recovery.mark_boot_complete();
    for _ in 0..2 {
        let frame = recovery
            .enter_frame(FrameKind::NodeRender, "nodes/meteor")
            .expect("not gated yet");
        recovery.stage_crash(CrashCause::Panic, &"simulated oom", None, &[], None);
        recovery.leave_frame(frame);
        recovery.record_recovered_crash();
    }
    assert_eq!(recovery.snapshot().level, RecoveryLevel::Red);
    lp_recovery::set_global(Box::leak(Box::new(recovery)));

    assert!(
        ledger_cleared(&clear_faults(&mut server, &output)),
        "an installed ledger is forgotten, and the ack says so"
    );
    let snapshot = lp_recovery::snapshot().expect("the global is installed");
    assert_eq!(snapshot.level, RecoveryLevel::Green);
    assert!(
        snapshot.path_entries.iter().all(|entry| entry.is_empty()),
        "every accusation is gone, not just the red ones"
    );
}
