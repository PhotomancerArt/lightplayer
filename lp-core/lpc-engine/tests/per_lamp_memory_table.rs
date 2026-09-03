//! The per-lamp memory table: what each lamp costs the engine, measured.
//!
//! `zook_load_tick_memory.rs` brackets zook's load and first ticks under a
//! tracking allocator and reports phase totals. This probe asks the next
//! question — *how many bytes per lamp, and in which phase* — by running each
//! fixture at two lamp counts and taking the slope. A slope is a measurement;
//! a formula summed from code reading is a guess, and the two are compared in
//! `docs/reports/2026-09-02-per-lamp-memory-table.md`, which also carries the
//! owner attribution (which struct holds each buffer) and the device-width
//! figures from `lp-cli profile --collect alloc`.
//!
//! Three fixtures:
//! - zook (`examples/zook-dome`, 1,500 lamps) and a generated 2× copy (3,000);
//! - small-dome (`examples/small-dome`, 6,310 lamps: 5,950 dome + 360 doors),
//!   as authored only — its map is ten repeat objects plus patch documents
//!   keyed by object path, so a scaled copy is not a faithful variant;
//! - a dome-scale synthetic (190 panels × 119 = 22,610 lamps, the big dome's
//!   shape per `docs/glossary.md`) and its 2× (45,220), generated from zook's
//!   files with a bigger repeat and enough output ports. Host only: no board
//!   runs the whole dome, and the emulator's guest heap is 320 K.
//!
//! An in-memory output provider is installed so the flush leg is measured
//! too (`EngineServices::flush_samples`, the provider's per-port buffers) —
//! the sibling probe never opens a port.
//!
//! ⚠️ Honest numbers come only from `cargo test -p lpc-engine`: a
//! workspace-wide run unifies lpvm-native's `debug` feature into this binary
//! and its regalloc trace adds host-only allocations to every compile. And one
//! `#[test]` per binary — the counters are process-wide.
//!
//! Phases are bracketed so the compile lands in its own tick (the shader
//! node's compile-window deferral): tick 1 is the pure non-compile working
//! set, tick 2 is the compile, ticks 3–6 are steady state.

use std::alloc::{GlobalAlloc, Layout, System};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use lpc_engine::{EngineServices, ProjectLoader};
use lpc_model::TreePath;
use lpc_shared::output::MemoryOutputProvider;
use lpfs::LpFsStd;

struct TrackingAlloc;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for TrackingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOC: TrackingAlloc = TrackingAlloc;

fn live() -> usize {
    LIVE.load(Ordering::Relaxed)
}

fn reset_peak() {
    PEAK.store(live(), Ordering::Relaxed);
}

fn peak() -> usize {
    PEAK.load(Ordering::Relaxed)
}

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lpc-engine lives two levels under the workspace root")
        .parent()
        .expect("workspace dir")
        .to_path_buf()
}

// ---- fixtures -----------------------------------------------------------

/// A project directory to measure, with its lamp count for the slope.
struct Fixture {
    label: &'static str,
    dir: PathBuf,
    lamps: u32,
}

/// The largest lamp count one authored output port carries in the synthetic
/// projects — under `WS281X_MAX_LEDS_PER_PORT` (1,024) so no wire is capped.
const SYNTHETIC_LAMPS_PER_PORT: u32 = 1000;

/// Generate a zook-shaped project in a temp dir: `strands` rotated copies of
/// a `per_strand`-lamp path, and output ports summing to exactly that many
/// lamps. Everything else (shader, clock, fixture settings) is zook's.
fn synthetic_from_zook(label: &'static str, strands: u32, per_strand: u32) -> Fixture {
    let src = workspace_dir().join("examples/zook-dome");
    let dir = std::env::temp_dir().join(format!(
        "lp-per-lamp-{}-{label}-{strands}x{per_strand}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp project dir");
    for entry in std::fs::read_dir(&src).expect("read examples/zook-dome") {
        let entry = entry.expect("dir entry");
        std::fs::copy(entry.path(), dir.join(entry.file_name())).expect("copy project file");
    }

    let mut map: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(src.join("fixture.map2d.json")).unwrap())
            .expect("zook map2d parses");
    let repeat = &mut map["objects"][0]["shape"]["repeat"];
    repeat["count"] = serde_json::json!(strands);
    repeat["shape"]["path"]["count"] = serde_json::json!(per_strand);
    std::fs::write(
        dir.join("fixture.map2d.json"),
        serde_json::to_string_pretty(&map).unwrap(),
    )
    .expect("write map2d");

    // Written by hand rather than through `serde_json::json!`: the node-def
    // loader reads `kind` as a leading header, and `serde_json::Map` sorts keys
    // (`bindings` would land first and the definition would not load).
    let lamps = strands * per_strand;
    let mut ports = String::new();
    let mut remaining = lamps;
    let mut port = 0u32;
    while remaining > 0 {
        let count = remaining.min(SYNTHETIC_LAMPS_PER_PORT);
        if port > 0 {
            ports.push_str(",\n");
        }
        ports.push_str(&format!(
            "    \"{port}\": {{ \"endpoint\": \"ws281x:local:P{port}\", \"count\": {count} }}"
        ));
        remaining -= count;
        port += 1;
    }
    let output = format!(
        "{{\n  \"kind\": \"Output\",\n  \"ports\": {{\n{ports}\n  }},\n  \"bindings\": {{ \"input\": {{ \"source\": \"bus:control.out\" }} }}\n}}\n"
    );
    std::fs::write(dir.join("output.json"), output).expect("write output.json");

    Fixture { label, dir, lamps }
}

fn fixtures() -> Vec<Fixture> {
    vec![
        Fixture {
            label: "zook",
            dir: workspace_dir().join("examples/zook-dome"),
            lamps: 1500,
        },
        synthetic_from_zook("zook-2x", 10, 300),
        Fixture {
            label: "small-dome",
            dir: workspace_dir().join("examples/small-dome"),
            lamps: 6310,
        },
        synthetic_from_zook("dome-scale", 190, 119),
        synthetic_from_zook("dome-scale-2x", 380, 119),
    ]
}

// ---- measurement --------------------------------------------------------

#[derive(Clone, Copy)]
struct Phase {
    label: &'static str,
    live_before: usize,
    step_peak: usize,
    live_after: usize,
}

impl Phase {
    fn resident(&self) -> i64 {
        self.live_after as i64 - self.live_before as i64
    }
    fn transient(&self) -> usize {
        self.step_peak.saturating_sub(self.live_before)
    }
}

const PHASES: [&str; 9] = [
    "load project",
    "set graphics + output provider",
    "tick 1 (no compile)",
    "tick 2 (compile)",
    "tick 3",
    "tick 4",
    "tick 5",
    "tick 6",
    "drop engine",
];

/// Load one fixture, run six ticks, bracket every phase. Returns the phases
/// in [`PHASES`] order.
fn measure(fixture: &Fixture) -> Vec<Phase> {
    let mut phases: Vec<Phase> = Vec::with_capacity(PHASES.len());
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

    let mut loaded = None;
    run(PHASES[0], &mut || {
        let fs = LpFsStd::new(fixture.dir.clone());
        let services = EngineServices::new(TreePath::parse("/probe.show").expect("root path"));
        loaded = Some(
            ProjectLoader::load_from_root(&fs, services)
                .unwrap_or_else(|e| panic!("load {}: {e}", fixture.label)),
        );
    });
    let mut rt = loaded.expect("loaded");

    run(PHASES[1], &mut || {
        rt.engine_mut()
            .set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
                lp_shader::ShaderFrontend::LpsGlsl,
            ))));
        rt.engine_mut()
            .services_mut()
            .set_output_provider(Some(Box::new(MemoryOutputProvider::new_permissive())));
    });

    let mut parts = Some(rt.into_parts());
    for tick in 0..6usize {
        let (engine, registry) = parts.as_mut().expect("engine");
        run(PHASES[2 + tick], &mut || {
            engine.tick(registry, 16).expect("tick");
        });
    }
    run(PHASES[8], &mut || {
        drop(parts.take());
    });
    phases
}

fn print_table(fixture: &Fixture, phases: &[Phase]) {
    println!(
        "\n== {} ({} lamps): memory phases (host bytes) ==",
        fixture.label, fixture.lamps
    );
    println!(
        "{:<34} {:>12} {:>12} {:>12} {:>12}",
        "phase", "live-before", "transient", "resident-d", "resident/lamp"
    );
    for p in phases {
        println!(
            "{:<34} {:>12} {:>12} {:>12} {:>12.2}",
            p.label,
            p.live_before,
            p.transient(),
            p.resident(),
            p.resident() as f64 / fixture.lamps as f64,
        );
    }
}

/// B/lamp slope of a phase figure between two runs of the same shape.
fn slope(a: (u32, i64), b: (u32, i64)) -> f64 {
    (b.1 - a.1) as f64 / (b.0 as f64 - a.0 as f64)
}

fn print_slopes(label: &str, a: (&Fixture, &[Phase]), b: (&Fixture, &[Phase])) {
    println!(
        "\n== {label}: B/lamp slopes ({} → {} lamps) ==",
        a.0.lamps, b.0.lamps
    );
    println!(
        "{:<34} {:>16} {:>16}",
        "phase", "resident B/lamp", "transient B/lamp"
    );
    for (pa, pb) in a.1.iter().zip(b.1.iter()) {
        println!(
            "{:<34} {:>16.2} {:>16.2}",
            pa.label,
            slope((a.0.lamps, pa.resident()), (b.0.lamps, pb.resident())),
            slope(
                (a.0.lamps, pa.transient() as i64),
                (b.0.lamps, pb.transient() as i64)
            ),
        );
    }
}

/// The per-lamp table. Prints everything under `--nocapture`; pins only
/// what must never regress silently (steady ticks do not grow; nothing is
/// left resident after the engine is dropped).
#[test]
fn per_lamp_memory_table() {
    let _ = env_logger::builder().is_test(true).try_init();
    let fixtures = fixtures();
    let baseline = live();
    let mut results: Vec<(&Fixture, Vec<Phase>)> = Vec::with_capacity(fixtures.len());
    for fixture in &fixtures {
        let phases = measure(fixture);
        print_table(fixture, &phases);
        results.push((fixture, phases));
    }
    println!("\n(process baseline before the first load: {baseline} B)");

    let by_label = |label: &str| -> (&Fixture, &[Phase]) {
        let (f, p) = results
            .iter()
            .find(|(f, _)| f.label == label)
            .unwrap_or_else(|| panic!("fixture {label} measured"));
        (f, p.as_slice())
    };
    print_slopes("zook", by_label("zook"), by_label("zook-2x"));
    print_slopes(
        "dome-scale",
        by_label("dome-scale"),
        by_label("dome-scale-2x"),
    );

    // Dropping the first engine leaves a process-global residue (the JIT
    // runtime's once-cells, interned tables — ~95 KB host, paid once); every
    // later fixture must return to within a small margin of that floor, or a
    // per-project buffer outlived its engine.
    let floor = results[0].1.last().expect("phases").live_after;
    for (fixture, phases) in &results {
        // Ticks 4–6 run after every buffer exists: sustained growth there is a
        // per-tick leak. 4 KiB matches the sibling probe's ceiling.
        let steady: i64 = phases
            .iter()
            .filter(|p| matches!(p.label, "tick 4" | "tick 5" | "tick 6"))
            .map(Phase::resident)
            .sum();
        assert!(
            steady <= 4 * 1024,
            "{}: steady-state ticks grew the heap by {steady} B — per-tick leak",
            fixture.label
        );
        let after_drop = phases.last().expect("phases").live_after;
        let above_floor = after_drop as i64 - floor as i64;
        assert!(
            above_floor <= 16 * 1024,
            "{}: {above_floor} B above the process floor after dropping the engine",
            fixture.label
        );
    }

    for fixture in &fixtures {
        if fixture.dir.starts_with(std::env::temp_dir()) {
            let _ = std::fs::remove_dir_all(&fixture.dir);
        }
    }
}
