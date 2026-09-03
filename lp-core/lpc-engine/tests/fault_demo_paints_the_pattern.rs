//! The checked-in `examples/fault-demo` must actually reach the never-black
//! path, end to end, on the same engine the device and the browser sim run.
//!
//! The example's shader compiles and then traps on fuel every frame — the
//! deterministic stand-in for the bench case (a compute node the crash
//! ledger quarantined after an OOM), without crashing a board. Two things
//! have to be true or the whole feature is theatre: the trapping node's
//! status is `Fault`, and the project-level verdict picks it up so every
//! output paints.
//!
//! The pixels themselves are pinned by the output node's own tests; what
//! this file proves is that a REAL project trips the policy.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lpc_engine::engine::LoadedProjectRuntime;
use lpc_engine::{EngineServices, ProjectLoader};
use lpc_model::{NodeRuntimeStatus, TreePath};
use lpfs::LpFsStd;

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lpc-engine lives two levels under the workspace root")
        .parent()
        .expect("workspace dir")
        .to_path_buf()
}

fn load_fault_demo() -> LoadedProjectRuntime {
    let fs = LpFsStd::new(workspace_dir().join("examples/fault-demo"));
    let services = EngineServices::new(TreePath::parse("/fault_demo.show").expect("root path"));
    let mut rt = ProjectLoader::load_from_root(&fs, services).expect("load examples/fault-demo");
    rt.engine_mut()
        .set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
    rt
}

#[test]
fn the_fault_demo_shader_faults_and_the_project_with_it() {
    let mut rt = load_fault_demo();

    // Warm up past the compile-window deferral: the first render only
    // REQUESTS a window, so the trap cannot happen until the compile has.
    // The ticks are expected to fail — that is the point — so the result is
    // deliberately not unwrapped.
    for _ in 0..4 {
        let _ = rt.tick(16);
    }

    let engine = rt.engine();
    let faulted: Vec<(String, String)> = engine
        .tree()
        .entries()
        .filter_map(|entry| match entry.status.value() {
            NodeRuntimeStatus::Fault(message) => Some((entry.path.to_string(), message.clone())),
            _ => None,
        })
        .collect();

    assert!(
        faulted
            .iter()
            .any(|(path, message)| path.contains("shader") && message.contains("fuel exhausted")),
        "the trapping shader must carry a Fault naming fuel, got {faulted:?}",
    );

    let fault = engine
        .project_fault()
        .expect("a faulted node means a faulted project");
    assert_eq!(
        fault.nodes.len(),
        faulted.len(),
        "the project verdict lists exactly the faulted entries: {:?}",
        fault.nodes,
    );
}

/// And the wall actually shows it. This is the end of the chain the whole
/// plan exists for: a real project, a real trap, and an output buffer that
/// is RED rather than black once the persistence delay has passed.
#[test]
fn the_fault_demo_output_breathes_red_instead_of_going_black() {
    let mut rt = load_fault_demo();

    // Past the compile-window deferral and then past the one-second
    // persistence delay, in frame time (the engine's only clock).
    for _ in 0..40 {
        let _ = rt.tick(50);
    }

    let samples = published_output_samples(&rt).expect("an output published a frame");
    assert!(!samples.is_empty(), "the output established an extent");
    assert!(
        samples.iter().any(|sample| *sample > 0),
        "the wall is not black: {:?}",
        &samples[..samples.len().min(12)],
    );
    // The pattern's shape: exactly one lit channel per RGB lamp (red, in
    // whatever order the run declares), the other two at zero.
    for (lamp, chunk) in samples.chunks_exact(3).enumerate() {
        let lit = chunk.iter().filter(|sample| **sample > 0).count();
        assert_eq!(lit, 1, "lamp {lamp} is not the fault pattern: {chunk:?}");
    }
}

/// The bytes an output last published, decoded back to u16 samples.
fn published_output_samples(rt: &LoadedProjectRuntime) -> Option<Vec<u16>> {
    let engine = rt.engine();
    let buffer_id = engine
        .tree()
        .entries()
        .find_map(|entry| engine.runtime_output_sink_buffer_id(entry.id))?;
    let buffer = engine.runtime_buffers().get(buffer_id)?;
    Some(
        buffer
            .value()
            .bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect(),
    )
}
