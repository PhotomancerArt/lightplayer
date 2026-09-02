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

    denied_shader_compile_is_a_fault();
}

/// A DENIED shader compile is a Fault, not an Error.
///
/// This is the bench case in miniature: the compile crashed (an OOM at JIT
/// time), the ledger gated the path, and the node then has no program and
/// renders black forever. The status has to say FAULT so the outputs paint
/// the pattern instead of the wall going quietly dark.
///
/// Runs inside the single test above — it shares the installed global and
/// depends on the red ledger the steps before it built.
#[cfg(feature = "node-shader")]
fn denied_shader_compile_is_a_fault() {
    use std::sync::Arc;

    use lpc_engine::node::{NodeRuntime, RenderContext};
    use lpc_engine::nodes::ShaderNode;
    use lpc_engine::products::visual::{
        ConsumerPolicy, RenderTextureRequest, VisualProduct, VisualSpace,
    };
    use lpc_model::{
        ArtifactLocation, AssetContentType, AssetLocation, NodeId, NodeRuntimeStatus, Revision,
        ShaderDef,
    };
    use lpc_registry::AssetText;

    // Drive the shader node's own compile path red at the depth the node
    // enters it (top level here, since the render is called directly).
    for _ in 0..2 {
        let _ = catch_node_panic_framed(FrameKind::ShaderCompile, "glsl", || {
            lp_recovery::stage_crash(CrashCause::Panic, &"simulated compile OOM", None, &[], None);
            lp_recovery::record_recovered_crash();
            Err::<(), _>(NodeError::msg("panic: simulated compile OOM"))
        });
    }

    let graphics = Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
        lp_shader::ShaderFrontend::LpsGlsl,
    ));
    let node_id = NodeId::new(1);
    let mut node = ShaderNode::new(
        node_id,
        ShaderDef::default(),
        AssetText {
            location: AssetLocation::artifact(ArtifactLocation::file("/shader.glsl")),
            content_type: AssetContentType::ShaderSource,
            revision: Revision::new(1),
            text: String::from("void render_2d(vec2 p) { lp_color = vec4(1.0); }"),
            diagnostic_name: String::from("/shader.glsl"),
        },
    );
    // The engine opens compile windows during tick; stand in for it.
    node.open_compile_window(Revision::new(1));
    let mut ctx = RenderContext::new(node_id, Revision::new(1), Some(graphics.clone()), None, 0.0);
    let request = RenderTextureRequest {
        width: 4,
        height: 4,
        format: lps_shared::TextureStorageFormat::Rgba16Unorm,
        time_seconds: 0.0,
        space: VisualSpace::TwoD,
        policy: ConsumerPolicy::default(),
    };
    let mut texture =
        lp_gfx::LpGraphics::create_render_target(graphics.as_ref(), 4, 4).expect("render target");
    let product = VisualProduct::new(node_id, 0);

    lpc_engine::node::RenderNode::render_texture_into(
        &mut node,
        product,
        &request,
        &mut texture,
        &mut ctx,
    )
    .expect("a denied compile still renders (black), it does not error the frame");

    match node.runtime_status() {
        Some(NodeRuntimeStatus::Fault(message)) => assert!(
            message.contains("recovery"),
            "the fault names the denial: {message}"
        ),
        other => panic!("a denied compile must be a Fault, got {other:?}"),
    }
    assert!(
        node.compilation_error().is_none(),
        "a quarantine is not a diagnostic — nothing for the editor's error strip to point at",
    );

    // `clear_fault` re-arms the compile: the node ATTEMPTS again, and since
    // the ledger is still red it faults again — honestly, rather than
    // sitting on a latch nothing would ever clear.
    node.clear_fault();
    assert!(node.compile_fault().is_none(), "the latch is released");
    assert_eq!(
        node.runtime_status(),
        None,
        "and with nothing else wrong the node reports clean until it retries"
    );

    lpc_engine::node::RenderNode::render_texture_into(
        &mut node,
        product,
        &request,
        &mut texture,
        &mut ctx,
    )
    .expect("retry render");
    assert!(
        matches!(node.runtime_status(), Some(NodeRuntimeStatus::Fault(_))),
        "the retry re-entered the gate (proving needs_compile was re-armed) and was denied again",
    );
}

/// Without the shader node kind there is no compile to deny.
#[cfg(not(feature = "node-shader"))]
fn denied_shader_compile_is_a_fault() {}
