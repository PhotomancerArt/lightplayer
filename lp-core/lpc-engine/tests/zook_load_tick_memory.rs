//! Where does zook-dome's heap actually go? Load + first-ticks memory
//! phases on the host, for the classic compile-transient defect
//! (`docs/defects/2026-08-29-shader-jit-compile-transient-starves-classic-heap.md`).
//!
//! The defect attributes the classic's OOM to the shader compile's
//! transient; the sibling probe in
//! `lp-shader/lpvm-native/tests/xt_compile_peak_memory.rs` measures that
//! transient at ~46 KB host / est. ~25–35 KB device — far short of the
//! ~126 KB the board lost. This test measures the other candidate: the
//! engine's own resident working set as the project loads and the first
//! ticks run (fixture maps, composed frames, sample buffers, per-output
//! state).
//!
//! Phases are bracketed so the compile lands in its own tick: the shader
//! node's compile-window deferral means tick 1 renders black and only
//! REQUESTS a compile; the window opens on tick 2. Tick 1's growth is
//! therefore the pure non-compile working set. (On the host the tick-2
//! figure includes wasmtime's own compile machinery, so only tick 1 and
//! the steady-state ticks transfer to the device; the device compile cost
//! comes from the sibling probe.)

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use lpc_engine::{EngineServices, ProjectLoader};
use lpc_model::TreePath;
use lpfs::LpFsStd;

struct TrackingAlloc;

thread_local! {
    /// Net bytes this thread allocated minus the bytes it freed. Signed: a
    /// thread may free what another allocated. `const`-initialised with no
    /// destructor, so reaching it from inside the allocator never allocates.
    static LIVE: Cell<isize> = const { Cell::new(0) };
    /// The highest `LIVE` this thread has reached since the last reset.
    static PEAK: Cell<isize> = const { Cell::new(0) };
}

fn add_live(delta: isize) {
    // `try_with`: a thread tearing down may still allocate or free.
    let _ = LIVE.try_with(|live| {
        let level = live.get() + delta;
        live.set(level);
        let _ = PEAK.try_with(|peak| {
            if level > peak.get() {
                peak.set(level);
            }
        });
    });
}

unsafe impl GlobalAlloc for TrackingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            add_live(layout.size() as isize);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        add_live(-(layout.size() as isize));
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOC: TrackingAlloc = TrackingAlloc;

/// This thread's live heap.
fn live() -> usize {
    LIVE.try_with(Cell::get).unwrap_or(0).max(0) as usize
}

fn reset_peak() {
    let level = LIVE.try_with(Cell::get).unwrap_or(0);
    let _ = PEAK.try_with(|peak| peak.set(level));
}

/// This thread's peak live heap since the last [`reset_peak`].
fn peak() -> usize {
    PEAK.try_with(Cell::get).unwrap_or(0).max(0) as usize
}

#[derive(Clone, Copy)]
struct Phase {
    label: &'static str,
    live_before: usize,
    step_peak: usize,
    live_after: usize,
}

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lpc-engine lives two levels under the workspace root")
        .parent()
        .expect("workspace dir")
        .to_path_buf()
}

/// Load catalog/projects/zook-dome and run the first ticks, bracketing each phase
/// under the tracking allocator. Prints the phase table (`--nocapture`).
#[test]
fn zook_dome_load_and_tick_memory_phases() {
    let mut phases: Vec<Phase> = Vec::with_capacity(32);
    let mut run = |label: &'static str, f: &mut dyn FnMut()| {
        let live_before = live();
        reset_peak();
        f();
        phases.push(Phase {
            label,
            live_before,
            step_peak: peak(),
            live_after: live(),
        });
    };

    let baseline = live();

    let mut loaded = None;
    run("load project", &mut || {
        let fs = LpFsStd::new(workspace_dir().join("catalog/projects/zook-dome"));
        let services = EngineServices::new(TreePath::parse("/zook_dome.show").expect("root path"));
        loaded = Some(
            ProjectLoader::load_from_root(&fs, services).expect("load catalog/projects/zook-dome"),
        );
    });
    let mut rt = loaded.expect("loaded");

    run("set graphics (engine ctor)", &mut || {
        rt.engine_mut()
            .set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
                lp_shader::ShaderFrontend::LpsGlsl,
            ))));
    });

    let (mut engine, registry) = rt.into_parts();
    for tick in 1..=6u32 {
        let label: &'static str = match tick {
            1 => "tick 1 (no compile: window requested)",
            2 => "tick 2 (compile window opens)",
            3 => "tick 3",
            4 => "tick 4",
            5 => "tick 5",
            _ => "tick 6",
        };
        run(label, &mut || {
            engine.tick(&registry, 16).expect("tick");
        });
    }

    println!(
        "\n== zook-dome load + first ticks, memory phases (host bytes, baseline {baseline} B) =="
    );
    println!(
        "{:<40} {:>12} {:>12} {:>12} {:>12} {:>12}",
        "phase", "live-before", "step-peak", "live-after", "transient", "resident-d"
    );
    for p in &phases {
        println!(
            "{:<40} {:>12} {:>12} {:>12} {:>12} {:>12}",
            p.label,
            p.live_before - baseline.min(p.live_before),
            p.step_peak - baseline.min(p.step_peak),
            p.live_after - baseline.min(p.live_after),
            p.step_peak.saturating_sub(p.live_before),
            p.live_after as i64 - p.live_before as i64,
        );
    }

    // The transferable figures, pinned (2026-08-29: load +49,226 B resident,
    // tick 1 +56,297 B resident — together the ~105 KB host analog of the
    // residents that leave the classic only ~22.6 KB free before its compile
    // ever runs). Gated at ~1.6x; if either trips, the classic lost real
    // headroom — re-measure before raising.
    let resident = |label_prefix: &str| -> i64 {
        let p = phases
            .iter()
            .find(|p| p.label.starts_with(label_prefix))
            .unwrap_or_else(|| panic!("phase {label_prefix} recorded"));
        p.live_after as i64 - p.live_before as i64
    };
    let load_resident = resident("load project");
    let tick1_resident = resident("tick 1");
    assert!(
        load_resident <= 80 * 1024,
        "zook-dome load resident {load_resident} B exceeds the 80 KiB ceiling"
    );
    assert!(
        tick1_resident <= 96 * 1024,
        "zook-dome first-tick resident {tick1_resident} B exceeds the 96 KiB ceiling"
    );
    // Steady state must not grow: ticks 4–6 ran after every buffer existed,
    // so any sustained growth here is a per-tick leak.
    let steady: i64 = phases
        .iter()
        .filter(|p| matches!(p.label, "tick 4" | "tick 5" | "tick 6"))
        .map(|p| p.live_after as i64 - p.live_before as i64)
        .sum();
    assert!(
        steady <= 4 * 1024,
        "steady-state ticks grew the heap by {steady} B — per-tick leak"
    );
}
