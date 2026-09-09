//! The per-node memory table: what one fixture+output pair costs the engine.
//!
//! `per_lamp_memory_table.rs` asks how many bytes each *lamp* costs, by running
//! one project shape at two lamp counts and taking the slope. This probe asks
//! the sibling question — *how many bytes does one fixture+output pair cost*,
//! independent of the lamps hanging off it — by running the same project shape
//! at k ∈ {1, 2, 4} pairs and taking the slope over k.
//!
//! A per-pair slope taken that way is still contaminated: adding a pair adds
//! its lamps too. So a fourth fixture runs k = 2 at half the lamps per pair.
//! The `2n` → `2n-half` difference over the lamp difference is *this tree's*
//! per-lamp figure on *this* instrument, and subtracting it from the all-in
//! per-pair slope leaves the per-node remainder — the dataflow/bus/registry
//! entries, the node structs, the per-port buffers, the names and paths.
//!
//! The fixtures are generated in-test from `projects/test/basic` (241 lamps: a 1×1
//! centre grid plus a 240-lamp 8-ring disc on a 10×10 canvas, Direct sampling,
//! one output with interpolation and LUT on) by the same rules as the committed
//! projects `projects/test/basic-{2n,4n,2n-half}`, so the host probe and the
//! emulator's alloc-diff cannot drift. Results are written up in
//! `docs/reports/2026-09-06-classic-not-enough-heap.md`.
//!
//! An in-memory output provider is installed so the flush leg is measured too
//! (`EngineServices::flush_samples`, the provider's per-port buffers) — this is
//! where a good part of the per-pair cost lives.
//!
//! ⚠️ Honest numbers come only from `cargo test -p lpc-engine`: a
//! workspace-wide run unifies lpvm-native's `debug` feature into this binary
//! and its regalloc trace adds host-only allocations to every compile. And one
//! `#[test]` per binary — the counters are process-wide.
//!
//! ⚠️ **Which tick compiles depends on the node count.** The shader node defers
//! its first compile by one frame (the memory-pressure safe point, ADR
//! `2026-08-03-memory-pressure-at-compile-safe-points`) by setting a
//! `compile_window_requested` flag and returning; the *next* render past that
//! flag compiles. With one fixture that next render is tick 2. With two or more
//! fixtures the shader is rendered once per fixture, so the second render of
//! **tick 1** already clears the flag and the compile lands there. The phase
//! table therefore never labels a tick "the compile"; [`derived`] finds it per
//! run as the tick with the largest transient, and reports it on its own. The
//! host compile is wasmtime's (~0.6–1.0 MB resident, ~±200 KB run to run) and
//! does not transfer to the device, so it swamps the per-pair frame signal —
//! the frame leg is the emulator's `lp-cli profile --collect alloc` column, not
//! this one. The load leg here is reproducible to the byte.

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

/// A project directory to measure: `pairs` fixture+output pairs, `lamps` lamps
/// in total across them.
struct Fixture {
    label: &'static str,
    dir: PathBuf,
    pairs: u32,
    lamps: u32,
}

/// `projects/test/basic`'s 8-ring disc, per ring, inner..outer as authored.
const BASIC_RING_COUNTS: [u32; 8] = [60, 48, 40, 32, 24, 16, 12, 8];

/// One board pin per pair, in `projects/test/quad-strips`'s order and starting
/// on the parent's own D10. These are WS281x endpoints the permissive manifest
/// resolves; an invented label loads fine and then fails to *open*, which
/// silently removes the per-port buffers from the measurement.
const PINS: [&str; 4] = ["D10", "D9", "D8", "D7"];

/// Ring counts for the half-lamp variant: the same eight rings, halved.
fn half_ring_counts() -> Vec<u32> {
    BASIC_RING_COUNTS.iter().map(|c| c / 2).collect()
}

/// Generate a `basic`-shaped project in a temp dir with `pairs` fixture+output
/// pairs, each on its own `bus:control.out/ch{i}` channel and its own port.
/// `half` halves every ring of the disc, so the node count is unchanged and the
/// lamp count is not. This mirrors `projects/test/basic-2n/generate.py` — keep
/// the two in step, and regenerate the committed projects if this changes.
fn synthetic_from_basic(label: &'static str, pairs: u32, half: bool) -> Fixture {
    assert!(
        pairs as usize <= PINS.len(),
        "only {} board pins are wired up",
        PINS.len()
    );
    let src = workspace_dir().join("projects/test/basic");
    let dir = std::env::temp_dir().join(format!(
        "lp-per-node-{}-{label}-{pairs}{}",
        std::process::id(),
        if half { "-half" } else { "" }
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp project dir");
    for shared in ["clock.json", "shader.json", "shader.glsl"] {
        std::fs::copy(src.join(shared), dir.join(shared)).expect("copy shared project file");
    }

    let counts: Vec<u32> = if half {
        half_ring_counts()
    } else {
        BASIC_RING_COUNTS.to_vec()
    };
    // 1 centre lamp + the disc's rings, per pair.
    let lamps_per_pair = 1 + counts.iter().sum::<u32>();

    let base_map: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(src.join("fixture.map2d.json")).unwrap())
            .expect("basic map2d parses");
    let base_fixture: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(src.join("fixture.json")).unwrap())
            .expect("basic fixture parses");
    let base_output: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(src.join("output.json")).unwrap())
            .expect("basic output parses");

    let mut nodes = String::from(
        "    \"clock\": { \"ref\": \"./clock.json\" },\n    \"shader\": { \"ref\": \"./shader.json\" }",
    );
    for index in 1..=pairs {
        let mut map = base_map.clone();
        let ring = &mut map["objects"][1]["shape"]["ring"];
        ring["counts"] = serde_json::json!(counts);
        ring["outer_count"] = serde_json::json!(counts[0]);
        std::fs::write(
            dir.join(format!("fixture{index}.map2d.json")),
            serde_json::to_string_pretty(&map).unwrap(),
        )
        .expect("write map2d");

        // Written through the file's own key order rather than a fresh
        // `serde_json::Map`: the node-def loader reads `kind` as a leading
        // header, and a sorting serializer would put `bindings` first and the
        // definition would not load. `serde_json`'s default (preserve_order off)
        // keeps `Value::Object` sorted, which is why only the *values* below are
        // edited and the leading `"kind"` is written by hand.
        let mut fixture = base_fixture.clone();
        fixture["mapping"]["source"] = serde_json::json!(format!("fixture{index}.map2d.json"));
        fixture["bindings"]["output"]["target"] =
            serde_json::json!(format!("bus:control.out/ch{index}"));
        write_node_def(
            &dir.join(format!("fixture{index}.json")),
            "Fixture",
            &fixture,
        );

        let mut output = base_output.clone();
        output["ports"]["0"]["endpoint"] =
            serde_json::json!(format!("ws281x:local:{}", PINS[index as usize - 1]));
        output["bindings"]["input"]["source"] =
            serde_json::json!(format!("bus:control.out/ch{index}"));
        write_node_def(&dir.join(format!("output{index}.json")), "Output", &output);

        nodes.push_str(&format!(
            ",\n    \"fixture{index}\": {{ \"ref\": \"./fixture{index}.json\" }},\n    \"output{index}\": {{ \"ref\": \"./output{index}.json\" }}"
        ));
    }
    std::fs::write(
        dir.join("module.json"),
        format!("{{\n  \"kind\": \"Module\",\n  \"nodes\": {{\n{nodes}\n  }}\n}}\n"),
    )
    .expect("write module.json");
    std::fs::write(
        dir.join("project.json"),
        format!("{{\n  \"format\": 10,\n  \"name\": \"per-node {label}\"\n}}\n"),
    )
    .expect("write project.json");

    Fixture {
        label,
        dir,
        pairs,
        lamps: pairs * lamps_per_pair,
    }
}

/// Write one node definition with `kind` as the leading header, followed by the
/// rest of the object's fields (whatever order `serde_json` gives them — only
/// the header position matters to the loader).
fn write_node_def(path: &Path, kind: &str, value: &serde_json::Value) {
    let body = value
        .as_object()
        .expect("node def is an object")
        .iter()
        .filter(|(k, _)| k.as_str() != "kind")
        .map(|(k, v)| {
            format!(
                "  {}: {}",
                serde_json::to_string(k).expect("key"),
                serde_json::to_string_pretty(v).expect("value")
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    std::fs::write(path, format!("{{\n  \"kind\": \"{kind}\",\n{body}\n}}\n"))
        .expect("write node def");
}

/// `projects/test/basic` itself: the k = 1 point, and the warm-up run.
fn parent_fixture(label: &'static str) -> Fixture {
    Fixture {
        label,
        dir: workspace_dir().join("projects/test/basic"),
        pairs: 1,
        lamps: 241,
    }
}

fn fixtures() -> Vec<Fixture> {
    vec![
        parent_fixture("basic-1n"),
        synthetic_from_basic("basic-2n", 2, false),
        synthetic_from_basic("basic-4n", 4, false),
        synthetic_from_basic("basic-2n-half", 2, true),
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
    "tick 1",
    "tick 2",
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
            engine
                .tick(registry, 16)
                .unwrap_or_else(|e| panic!("{} tick {}: {e}", fixture.label, tick + 1));
        });
    }
    run(PHASES[8], &mut || {
        drop(parts.take());
    });
    phases
}

fn print_table(fixture: &Fixture, phases: &[Phase]) {
    println!(
        "\n== {} ({} pairs, {} lamps): memory phases (host bytes) ==",
        fixture.label, fixture.pairs, fixture.lamps
    );
    println!(
        "{:<34} {:>12} {:>12} {:>12} {:>12}",
        "phase", "live-before", "transient", "resident-d", "resident/pair"
    );
    for p in phases {
        println!(
            "{:<34} {:>12} {:>12} {:>12} {:>12.1}",
            p.label,
            p.live_before,
            p.transient(),
            p.resident(),
            p.resident() as f64 / fixture.pairs as f64,
        );
    }
}

/// Slope of a phase figure between two runs of the same shape, per unit of `x`
/// (pairs or lamps, whichever the caller varied).
fn slope(a: (u32, i64), b: (u32, i64)) -> f64 {
    (b.1 - a.1) as f64 / (b.0 as f64 - a.0 as f64)
}

/// Both figures of one phase, as `(resident, transient)`.
fn phase_of(phases: &[Phase], phase: &str) -> (i64, i64) {
    let p = phases
        .iter()
        .find(|p| p.label.starts_with(phase))
        .unwrap_or_else(|| panic!("phase {phase} recorded"));
    (p.resident(), p.transient() as i64)
}

/// Which axis a pair of runs differs along — the one the slope divides by.
#[derive(Clone, Copy)]
enum Axis {
    Pairs,
    Lamps,
}

impl Axis {
    fn of(self, fixture: &Fixture) -> u32 {
        match self {
            Axis::Pairs => fixture.pairs,
            Axis::Lamps => fixture.lamps,
        }
    }
    fn unit(self) -> &'static str {
        match self {
            Axis::Pairs => "pair",
            Axis::Lamps => "lamp",
        }
    }
}

fn print_slopes(label: &str, axis: Axis, a: (&Fixture, &[Phase]), b: (&Fixture, &[Phase])) {
    let unit = axis.unit();
    println!(
        "\n== {label}: B/{unit} slopes ({} → {} {unit}s) ==",
        axis.of(a.0),
        axis.of(b.0)
    );
    println!(
        "{:<34} {:>18} {:>18}",
        "phase",
        format!("resident B/{unit}"),
        format!("transient B/{unit}")
    );
    for (pa, pb) in a.1.iter().zip(b.1.iter()) {
        println!(
            "{:<34} {:>18.2} {:>18.2}",
            pa.label,
            slope((axis.of(a.0), pa.resident()), (axis.of(b.0), pb.resident())),
            slope(
                (axis.of(a.0), pa.transient() as i64),
                (axis.of(b.0), pb.transient() as i64)
            ),
        );
    }
}

/// The figures the slopes are taken on.
///
/// ⚠️ The phase table cannot be sloped row by row: the host compile does not
/// land in the same tick for every fixture (`projects/test/basic` defers it to tick
/// 2, a multi-pair project takes it in tick 1), and it is a ~1 MB wasmtime cost
/// that dwarfs the per-pair signal wherever it lands. So the compile tick is
/// found per fixture (the tick with the largest transient), reported on its own
/// — it is host-only and does not transfer to the device — and the *frames*
/// figure sums the ticks either side of it.
const DERIVED: [&str; 4] = [
    "load project",
    "install graphics + provider",
    "frames (excl. the compile tick)",
    "host shader compile (does not transfer)",
];

/// [`DERIVED`]'s figures for one run, each `(resident, transient)`.
fn derived(phases: &[Phase]) -> Vec<(i64, i64)> {
    let ticks = &phases[2..8];
    let compile = ticks
        .iter()
        .enumerate()
        .max_by_key(|(_, p)| p.transient())
        .map(|(i, _)| i)
        .expect("six ticks");
    let frames: Vec<&Phase> = ticks
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != compile)
        .map(|(_, p)| p)
        .collect();
    vec![
        phase_of(phases, PHASES[0]),
        phase_of(phases, PHASES[1]),
        (
            frames.iter().map(|p| p.resident()).sum(),
            frames
                .iter()
                .map(|p| p.transient() as i64)
                .max()
                .unwrap_or(0),
        ),
        (ticks[compile].resident(), ticks[compile].transient() as i64),
    ]
}

/// The decomposition of the per-pair cost: all-in (the slope over pairs at a
/// fixed lamps-per-pair), the per-lamp part (this instrument's own B/lamp
/// slope, from the same-node-count half-lamp run, times the lamps one pair
/// carries), and the remainder — what the *node* itself costs.
fn print_decomposition(
    all_in: (&Fixture, &[Phase]),
    all_in_b: (&Fixture, &[Phase]),
    lamp_a: (&Fixture, &[Phase]),
    lamp_b: (&Fixture, &[Phase]),
) {
    let lamps_per_pair = all_in.0.lamps / all_in.0.pairs;
    println!(
        "\n== per-pair decomposition (all-in {} → {} pairs; per-lamp from {} → {}) ==",
        all_in.0.pairs, all_in_b.0.pairs, lamp_a.0.label, lamp_b.0.label
    );
    println!("   one pair carries {lamps_per_pair} lamps");
    println!(
        "{:<41} {:>11} {:>10} {:>11} {:>11} {:>11}",
        "figure", "all-in R", "B/lamp R", "per-node R", "all-in T", "per-node T"
    );
    let (a, b) = (derived(all_in.1), derived(all_in_b.1));
    let (la, lb) = (derived(lamp_a.1), derived(lamp_b.1));
    for (i, label) in DERIVED.iter().enumerate() {
        let all_in_r = slope((all_in.0.pairs, a[i].0), (all_in_b.0.pairs, b[i].0));
        let all_in_t = slope((all_in.0.pairs, a[i].1), (all_in_b.0.pairs, b[i].1));
        let per_lamp_r = slope((lamp_a.0.lamps, la[i].0), (lamp_b.0.lamps, lb[i].0));
        let per_lamp_t = slope((lamp_a.0.lamps, la[i].1), (lamp_b.0.lamps, lb[i].1));
        println!(
            "{label:<41} {:>11.1} {:>10.2} {:>11.1} {:>11.1} {:>11.1}",
            all_in_r,
            per_lamp_r,
            all_in_r - per_lamp_r * lamps_per_pair as f64,
            all_in_t,
            all_in_t - per_lamp_t * lamps_per_pair as f64,
        );
    }
}

fn print_derived(fixture: &Fixture, phases: &[Phase]) {
    println!(
        "\n-- {} derived figures (compile tick found by peak transient) --",
        fixture.label
    );
    for (label, (r, t)) in DERIVED.iter().zip(derived(phases)) {
        println!("{label:<41} resident {r:>10}   transient {t:>10}");
    }
}

/// The per-node table. Prints everything under `--nocapture`; asserts only
/// that every fixture loaded and ticked, and the two invariants the sibling
/// probe pins (steady ticks do not grow, nothing outlives the engine). It does
/// not assert byte figures — like `per_lamp_memory_table.rs` it is a probe, and
/// the numbers it prints are read into
/// `docs/reports/2026-09-06-classic-not-enough-heap.md`.
#[test]
fn per_node_memory_table() {
    let _ = env_logger::builder().is_test(true).try_init();

    // A discarded warm-up run: the first load in a process pays the JIT
    // runtime's once-cells and wasmtime's interned tables, and measuring one
    // fixture cold and the rest warm puts that difference straight into the
    // slope.
    let _ = measure(&parent_fixture("warm-up"));

    let fixtures = fixtures();
    let baseline = live();
    let mut results: Vec<(&Fixture, Vec<Phase>)> = Vec::with_capacity(fixtures.len());
    for fixture in &fixtures {
        let phases = measure(fixture);
        assert_eq!(
            phases.len(),
            PHASES.len(),
            "{}: every phase ran (load and six ticks succeed)",
            fixture.label
        );
        print_table(fixture, &phases);
        print_derived(fixture, &phases);
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
    print_slopes(
        "1n → 2n",
        Axis::Pairs,
        by_label("basic-1n"),
        by_label("basic-2n"),
    );
    print_slopes(
        "2n → 4n",
        Axis::Pairs,
        by_label("basic-2n"),
        by_label("basic-4n"),
    );
    print_slopes(
        "half-lamp (same node count, lamp axis)",
        Axis::Lamps,
        by_label("basic-2n-half"),
        by_label("basic-2n"),
    );
    print_decomposition(
        by_label("basic-2n"),
        by_label("basic-4n"),
        by_label("basic-2n-half"),
        by_label("basic-2n"),
    );

    // The two invariants this probe shares with `per_lamp_memory_table.rs`.
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
