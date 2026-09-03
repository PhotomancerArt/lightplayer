//! The heartbeat must carry a loaded project's fault verdict.
//!
//! This is the honesty half of "a fault is never black": the strip breathes
//! red, and the device card has to say why. The card's whole pipeline hangs
//! off this one additive field — if the server never fills it, every layer
//! above is decoration (2026-09-01 bench: a C6 read "Running" for two days
//! while its only shader was quarantined).
//!
//! `examples/fault-demo` is the subject on purpose: a shader that compiles
//! and then traps on fuel every frame, deterministic on every backend and
//! incapable of crashing a board.

extern crate alloc;

use alloc::rc::Rc;
use alloc::sync::Arc;
use core::cell::RefCell;
use std::path::{Path, PathBuf};

use lp_gfx_lpvm::TargetLpvmGraphics;
use lpa_server::{LpGraphics, LpServer};
use lpc_model::{AsLpPath, AsLpPathBuf};
use lpc_shared::output::MemoryOutputProvider;
use lpfs::LpFsStd;

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lpa-server lives two levels under the workspace root")
        .parent()
        .expect("workspace dir")
        .to_path_buf()
}

/// A server whose project store IS the checked-in `examples/` directory, so
/// `/fault-demo` loads the real example rather than a rebuilt lookalike.
fn server_over_examples() -> (
    LpServer,
    Rc<RefCell<dyn lpc_shared::output::OutputProvider>>,
) {
    let output_provider: Rc<RefCell<dyn lpc_shared::output::OutputProvider>> =
        Rc::new(RefCell::new(MemoryOutputProvider::new()));
    let graphics: Arc<dyn LpGraphics> =
        Arc::new(TargetLpvmGraphics::new(lpa_server::DEVICE_SHADER_FRONTEND));
    let base_fs = Box::new(LpFsStd::new(workspace_dir().join("examples")));
    let server = LpServer::new(
        output_provider.clone(),
        base_fs,
        "/".as_path(),
        None,
        None,
        graphics,
    );
    (server, output_provider)
}

fn load(
    server: &mut LpServer,
    output: Rc<RefCell<dyn lpc_shared::output::OutputProvider>>,
    name: &str,
) {
    let graphics: Arc<dyn LpGraphics> =
        Arc::new(TargetLpvmGraphics::new(lpa_server::DEVICE_SHADER_FRONTEND));
    let server_ptr: *mut LpServer = server;
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
            graphics,
        )
        .expect("the example loads");
    }
}

#[test]
fn a_faulting_project_reports_its_fault_to_the_heartbeat() {
    let (mut server, output) = server_over_examples();
    load(&mut server, output, "fault-demo");

    // Warm past the compile-window deferral: the first render only REQUESTS
    // a compile window, so the trap cannot happen until the compile has.
    // The frames are expected to fail — that is the subject.
    for _ in 0..5 {
        let _ = server.advance_frame(16);
    }

    let reported = server.project_manager().list_loaded_projects_with_faults();
    let [project] = reported.as_slice() else {
        panic!("one loaded project, got {}", reported.len());
    };
    let fault = project
        .fault
        .as_ref()
        .expect("a trapping project reports a fault on the heartbeat");
    assert!(
        fault
            .nodes
            .iter()
            .any(|node| node.message.contains("fuel exhausted")),
        "the reported fault must name the runtime's own reason, got {:?}",
        fault.nodes,
    );

    // The per-FRAME listing stays cheap on purpose: `advance_frame` calls it
    // every tick, and cloning status strings 40+ times a second on a device
    // whose heap is what the fault is usually about is the wrong trade.
    let cheap = server.project_manager().list_loaded_projects();
    assert!(
        cheap[0].fault.is_none(),
        "the per-frame listing must not build fault records"
    );
}

#[test]
fn a_healthy_project_reports_no_fault() {
    // The other half of the guard: the card must never wear a degraded face
    // over a board that is simply running.
    let (mut server, output) = server_over_examples();
    load(&mut server, output, "pulse");

    for _ in 0..5 {
        server.advance_frame(16).expect("pulse ticks cleanly");
    }

    let reported = server.project_manager().list_loaded_projects_with_faults();
    assert!(
        reported[0].fault.is_none(),
        "a healthy project must report no fault, got {:?}",
        reported[0].fault,
    );
}
