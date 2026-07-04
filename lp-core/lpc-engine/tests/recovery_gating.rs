//! Recovery-frame gating through the engine's panic boundary helpers.
//!
//! Lives in an integration test (own process) because it installs the
//! process-wide lp-recovery global; unit tests elsewhere must keep seeing
//! an uninstalled (inert) global.
//!
//! Panic → catch → `record_recovered_crash` is exercised with REAL panics
//! in the emulator test suite (fw-tests); host tests avoid `unwinding`-
//! based unwinds and instead drive the ledger through the lp-recovery API,
//! asserting the engine-side wrapper behavior: inert when uninstalled,
//! frames pushed/popped, gated paths denied as legible `NodeError`s.

use lp_recovery::{CrashCause, FrameKind, InMemoryBackend, Recovery, RecoveryLevel, ResetCause};
use lpc_engine::node::NodeError;
use lpc_engine::node::catch_node_panic::catch_node_panic_framed;

/// Single test fn: steps share the installed global and must run in order.
#[test]
fn framed_wrapper_gates_and_tracks() {
    // --- Uninstalled global: wrapper is a pass-through -------------------
    let ran = catch_node_panic_framed(FrameKind::NodeRender, "nodes/any", || {
        Ok::<_, NodeError>(42)
    })
    .unwrap();
    assert_eq!(ran, 42);
    assert!(lp_recovery::snapshot().is_none());

    // --- Install a live recovery instance --------------------------------
    let (recovery, assessment) = Recovery::init(InMemoryBackend::new(), ResetCause::PowerOn);
    assert_eq!(assessment.level, RecoveryLevel::Green);
    lp_recovery::set_global(Box::leak(Box::new(recovery)));
    lp_recovery::mark_boot_complete();

    // --- Normal errors are not crashes -----------------------------------
    let err = catch_node_panic_framed(FrameKind::NodeRender, "nodes/erroring", || {
        Err::<(), _>(NodeError::msg("plain node error"))
    })
    .unwrap_err();
    assert_eq!(err.to_string(), "plain node error");
    let snap = lp_recovery::snapshot().unwrap();
    assert_eq!(snap.level, RecoveryLevel::Green, "errors are not blame");
    assert_eq!(snap.stack_depth, 0, "frame popped on error return");

    // --- Two crashes on one path gate it (in-run, no reboot) -------------
    // Simulate what the panic path does: stage inside the frame, then the
    // catch boundary records the recovered crash.
    for _ in 0..2 {
        let _ = catch_node_panic_framed(FrameKind::NodeRender, "nodes/crashy", || {
            lp_recovery::stage_crash(CrashCause::Panic, &"simulated panic", None, &[], None);
            lp_recovery::record_recovered_crash();
            Err::<(), _>(NodeError::msg("panic: simulated panic"))
        });
    }
    assert_eq!(lp_recovery::snapshot().unwrap().level, RecoveryLevel::Red);

    let denied = catch_node_panic_framed(
        FrameKind::NodeRender,
        "nodes/crashy",
        || -> Result<(), NodeError> { panic!("must not execute: path is gated") },
    )
    .unwrap_err();
    let message = denied.to_string();
    assert!(
        message.contains("recovery") && message.contains("nodes/crashy"),
        "gated error is legible, got: {message}"
    );

    // --- Siblings unaffected; nesting works -------------------------------
    let nested = catch_node_panic_framed(FrameKind::NodeRender, "nodes/healthy", || {
        catch_node_panic_framed(FrameKind::ShaderCompile, "glsl", || {
            let snap = lp_recovery::snapshot().unwrap();
            assert_eq!(snap.stack_depth, 2);
            Ok::<_, NodeError>("compiled")
        })
    })
    .unwrap();
    assert_eq!(nested, "compiled");
    assert_eq!(lp_recovery::snapshot().unwrap().stack_depth, 0);
}
