//! The fault pattern must reach the OUTPUT PROVIDER — the wall — not just
//! the engine's published buffer.
//!
//! G1 bench, 2026-09-02: `examples/fault-demo` breathed red in the browser
//! sim and stayed DARK on the C6. The sim reads the output node's published
//! buffer; the LEDs get whatever `Engine::tick` flushes to the provider,
//! and a tick whose walk failed used to return before the flush. The frame
//! an output's own render fails is exactly the frame it paints the pattern,
//! so the pattern was published every frame and delivered never.
//!
//! This test is the one the engine-level check could not be: it asserts on
//! the bytes the provider was handed.

extern crate alloc;

use alloc::rc::Rc;
use alloc::sync::Arc;
use core::cell::RefCell;
use std::path::{Path, PathBuf};

use lp_gfx_lpvm::TargetLpvmGraphics;
use lpa_server::{LpGraphics, LpServer};
use lpc_model::{AsLpPath, AsLpPathBuf};
use lpc_shared::output::{MemoryOutputProvider, OutputProvider};
use lpfs::LpFsStd;

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lpa-server lives two levels under the workspace root")
        .parent()
        .expect("workspace dir")
        .to_path_buf()
}

/// A server over the checked-in `examples/` with a memory provider we keep a
/// CONCRETE handle to, so the bytes it was handed can be read back.
fn server_with_memory_provider() -> (LpServer, Rc<RefCell<MemoryOutputProvider>>) {
    let memory = Rc::new(RefCell::new(MemoryOutputProvider::new_permissive()));
    let as_provider: Rc<RefCell<dyn OutputProvider>> = memory.clone();
    let graphics: Arc<dyn LpGraphics> =
        Arc::new(TargetLpvmGraphics::new(lpa_server::DEVICE_SHADER_FRONTEND));
    let base_fs = Box::new(LpFsStd::new(workspace_dir().join("examples")));
    let server = LpServer::new(as_provider, base_fs, "/".as_path(), None, None, graphics);
    (server, memory)
}

fn load(server: &mut LpServer, output: Rc<RefCell<dyn OutputProvider>>, name: &str) {
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

/// Every open port's last written samples, concatenated.
fn written_samples(memory: &MemoryOutputProvider) -> Vec<u16> {
    memory
        .get_all_handles()
        .into_iter()
        .filter_map(|handle| memory.get_data(handle))
        .flatten()
        .collect()
}

#[test]
fn the_fault_demo_pattern_is_flushed_to_the_provider_even_though_the_tick_fails() {
    let (mut server, memory) = server_with_memory_provider();
    let as_provider: Rc<RefCell<dyn OutputProvider>> = memory.clone();
    load(&mut server, as_provider, "fault-demo");

    // Past the compile-window deferral and past the one-second persistence
    // delay, in frame time. Every frame after the compile FAILS (the trap
    // propagates out of the output's own render) — that is the subject.
    for _ in 0..40 {
        let _ = server.advance_frame(50);
    }

    let samples = written_samples(&memory.borrow());
    assert!(
        !samples.is_empty(),
        "the provider was handed a frame despite the failing walk"
    );
    assert!(
        samples.iter().any(|sample| *sample > 0),
        "the wall is not black: {:?}",
        &samples[..samples.len().min(12)],
    );
    // The pattern's shape on the wire: one lit channel per RGB lamp.
    for (lamp, chunk) in samples.chunks_exact(3).enumerate() {
        let lit = chunk.iter().filter(|sample| **sample > 0).count();
        assert_eq!(lit, 1, "lamp {lamp} is not the fault pattern: {chunk:?}");
    }
}

#[test]
fn a_healthy_project_still_reaches_the_provider() {
    // The control: the flush path this fixes must not change for a project
    // whose walk succeeds.
    let (mut server, memory) = server_with_memory_provider();
    let as_provider: Rc<RefCell<dyn OutputProvider>> = memory.clone();
    load(&mut server, as_provider, "pulse");

    for _ in 0..10 {
        server.advance_frame(50).expect("pulse ticks cleanly");
    }

    let samples = written_samples(&memory.borrow());
    assert!(!samples.is_empty(), "pulse wrote a frame");
    assert!(
        samples.iter().any(|sample| *sample > 0),
        "pulse is lit: {:?}",
        &samples[..samples.len().min(12)],
    );
}
