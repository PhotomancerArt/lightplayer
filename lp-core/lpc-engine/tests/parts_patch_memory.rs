//! What a format-6 fixture costs the engine's heap, against its format ≤ 5
//! twin (mapping rebuild M2 P3; leans Q43, Q44).
//!
//! Four shapes, each built twice: as a format-6 fixture (an in-memory
//! `lpc_mapping` mapping, given to the fixture with `with_parts_mapping`, the
//! way M2 P4's loader will) and as its format ≤ 5 twin (the same geometry as
//! a map2d document, auto-flowed). One path of 256 and of 512 lamps (the
//! per-lamp slope), and a repeat of 32 and of 64 copies of an 8-lamp path
//! (256 and 512 lamps again: the per-segment figure). Each is loaded,
//! ticked four times (the compile defers to the second), and bracketed under
//! a tracking allocator.
//!
//! It asserts only what must hold at any width: **the per-lamp resident
//! slope of a format-6 fixture equals its twin's within 1 B** (the lamps'
//! positions are held once, in the carrier, not again in the tree). The rest
//! is printed under `--nocapture` as the input to Q44's device-width bar,
//! measured for real on fw-emu in M2 P5:
//!
//! ```bash
//! cargo test -p lpc-engine --test parts_patch_memory -- --nocapture
//! ```
//!
//! The counters are this thread's: the host compile's worker threads
//! allocate buffers this thread then frees, so the first tick reads tens of
//! KB negative, alike in every row. Compare rows; don't read an absolute.
//!
//! ⚠️ One `#[test]` per binary (the counters are per thread, but the JIT's
//! once-cells are process-wide), and honest numbers come only from
//! `cargo test -p lpc-engine` (a workspace run unifies host-only features in).

// A format-6 fixture painted by a shader: the probe needs both node kinds,
// and the gates-off clippy pass compiles this file to nothing.
#![cfg(all(feature = "node-fixture", feature = "node-shader"))]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::Arc;

use lpc_engine::nodes::FixtureNode;
use lpc_engine::{EngineServices, ProjectLoader};
use lpc_mapping::part::{Mapping, PartKind, Path, Repeat, Shape};
use lpc_mapping::patching::{Amount, Start};
use lpc_model::nodes::fixture::{FixtureSamplingConfig, MappingConfig};
use lpc_model::{NodeName, Revision, TreePath, current_revision};
use lpfs::{AsLpPath, LpFs, LpFsMemory};

/// The headline: per-lamp resident slopes of each format-6 shape against its
/// twin, within 1 B.
#[test]
fn parts_patch_memory() {
    let _ = env_logger::builder().is_test(true).try_init();
    // The first engine in a process pays the JIT's and the compiler's
    // once-cells (and frees in its first tick what they replaced): a
    // warm-up run takes that out of every row below.
    let _ = measure(Shape6::Path(256), false);
    let mut rows = Vec::new();
    for shape in [
        Shape6::Path(256),
        Shape6::Path(512),
        Shape6::Repeat(32),
        Shape6::Repeat(64),
    ] {
        for format6 in [false, true] {
            let phases = measure(shape, format6);
            print_table(shape, format6, &phases);
            rows.push((shape, format6, phases));
        }
    }

    let figure = |shape: Shape6, format6: bool, what: fn(&[Phase]) -> i64| -> i64 {
        let (_, _, phases) = rows
            .iter()
            .find(|(s, f, _)| *s == shape && *f == format6)
            .expect("measured");
        what(phases)
    };
    println!("\n== slopes ==");
    println!("{:<44} {:>14} {:>14}", "", "format 5 twin", "format 6");
    // Per lamp: one path at two lengths (one segment either way).
    let per_lamp = |format6| {
        (figure(Shape6::Path(512), format6, resident)
            - figure(Shape6::Path(256), format6, resident)) as f64
            / 256.0
    };
    let (lamp5, lamp6) = (per_lamp(false), per_lamp(true));
    println!(
        "{:<44} {lamp5:>14.2} {lamp6:>14.2}",
        "resident B/lamp (one path, 256 → 512)"
    );
    // Per segment: the same 512 lamps as one path and as 64 copies (63 more
    // segments), and 32 → 64 copies less the lamps' own share.
    for (label, figure_of) in [
        (
            "resident B/segment (512 lamps, 1 → 64 segments)",
            Box::new(|format6: bool| {
                (figure(Shape6::Repeat(64), format6, resident)
                    - figure(Shape6::Path(512), format6, resident)) as f64
                    / 63.0
            }) as Box<dyn Fn(bool) -> f64>,
        ),
        (
            "resident B/segment (32 → 64 copies, net of lamps)",
            Box::new(|format6: bool| {
                let lamps = if format6 { lamp6 } else { lamp5 };
                ((figure(Shape6::Repeat(64), format6, resident)
                    - figure(Shape6::Repeat(32), format6, resident)) as f64
                    - 256.0 * lamps)
                    / 32.0
            }),
        ),
    ] {
        println!(
            "{label:<44} {:>14.2} {:>14.2}",
            figure_of(false),
            figure_of(true)
        );
    }
    for shape in [Shape6::Path(512), Shape6::Repeat(64)] {
        let first_tick = |format6| figure(shape, format6, |p| p[2].transient());
        println!(
            "{:<44} {:>14} {:>14}",
            format!("tick 1 transient B ({}, first patch)", shape.label()),
            first_tick(false),
            first_tick(true)
        );
        let after = |format6| figure(shape, format6, resident);
        println!(
            "{:<44} {:>14} {:>14}",
            format!("resident after load + 4 ticks B ({})", shape.label()),
            after(false),
            after(true)
        );
        let peak = |format6| figure(shape, format6, peak_over_live_before_load);
        println!(
            "{:<44} {:>14} {:>14}",
            format!("peak over the start B ({})", shape.label()),
            peak(false),
            peak(true)
        );
    }

    assert!(
        (lamp6 - lamp5).abs() <= 1.0,
        "a format-6 fixture costs {lamp6:.2} B/lamp resident, its twin {lamp5:.2}: positions \
         held twice?"
    );
}

// --- the shapes ------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Debug)]
enum Shape6 {
    /// One straight path of this many lamps.
    Path(u32),
    /// A repeat of this many copies of an 8-lamp path.
    Repeat(u32),
}

impl Shape6 {
    fn label(self) -> String {
        match self {
            Shape6::Path(n) => format!("one path of {n}"),
            Shape6::Repeat(k) => format!("{k} × 8-lamp copies"),
        }
    }

    /// The format-6 mapping: from lamp 0 of output `main`.
    fn mapping(self) -> Mapping {
        let mut m = Mapping::new();
        let part = match self {
            Shape6::Path(n) => m.new_part(path(n)),
            Shape6::Repeat(k) => {
                let copy = m.new_part(path(8));
                m.new_part(PartKind::Repeat(Repeat::new(copy, [50.0, 50.0], k)))
            }
        };
        m.parts.push(part);
        m.start = Start::at("main", 0, Amount::lamps(0));
        m
    }

    /// The same geometry as a map2d document.
    fn map2d(self) -> String {
        let path = |count: u32| {
            format!(
                r#"{{ "path": {{ "points": [[54.0, 50.0], [96.0, 50.0]], "count": {count} }} }}"#
            )
        };
        let shape = match self {
            Shape6::Path(n) => path(n),
            Shape6::Repeat(k) => format!(
                r#"{{ "repeat": {{ "shape": {}, "center": [50.0, 50.0], "count": {k} }} }}"#,
                path(8)
            ),
        };
        format!(
            r#"{{
  "format": 3,
  "sample_diameter": 1.0,
  "canvas": [0.0, 0.0, 100.0, 100.0],
  "objects": [ {{ "name": "strip", "id": "strip", "shape": {shape} }} ]
}}"#
        )
    }
}

fn path(count: u32) -> PartKind {
    PartKind::Shape(Shape::Path(Path::through(
        &[[54.0, 50.0], [96.0, 50.0]],
        count,
    )))
}

// --- measurement -------------------------------------------------------------

const PHASES: [&str; 6] = [
    "load project",
    "set graphics",
    "tick 1",
    "tick 2 (compile)",
    "tick 3",
    "tick 4",
];

#[derive(Clone, Copy)]
struct Phase {
    label: &'static str,
    live_before: i64,
    step_peak: i64,
    live_after: i64,
}

impl Phase {
    fn resident(&self) -> i64 {
        self.live_after - self.live_before
    }

    fn transient(&self) -> i64 {
        (self.step_peak - self.live_before).max(0)
    }
}

/// Resident after the load and every tick, from just before the load.
fn resident(phases: &[Phase]) -> i64 {
    phases.last().expect("phases").live_after - phases[0].live_before
}

/// The highest the heap stood, from just before the load.
fn peak_over_live_before_load(phases: &[Phase]) -> i64 {
    phases.iter().map(|p| p.step_peak).max().expect("phases") - phases[0].live_before
}

/// Load `shape` (as format 6 or its twin), set graphics, tick four times;
/// every phase bracketed.
fn measure(shape: Shape6, format6: bool) -> Vec<Phase> {
    let fs = project_fs(shape, format6);
    let mapping = format6.then(|| shape.mapping());
    let mut phases = Vec::with_capacity(PHASES.len());
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
    let mut mapping = mapping;
    run(PHASES[0], &mut || {
        let services = EngineServices::new(TreePath::parse("/memory.show").expect("path"));
        let mut rt = ProjectLoader::load_from_root(&fs, services).expect("load");
        if let Some(mapping) = mapping.take() {
            // What M2 P4's loader will do: the format-6 fixture's runtime,
            // built from its mapping.
            let id = rt
                .tree()
                .lookup_sibling(rt.tree().root(), NodeName::parse("strip").expect("name"))
                .expect("fixture node");
            let fixture = FixtureNode::new(
                id,
                MappingConfig::path_points_vec(Vec::new(), 1.0),
                FixtureSamplingConfig::Direct,
                Revision::new(1),
            )
            .with_parts_mapping(
                Mapping {
                    canvas: Some([0.0, 0.0, 100.0, 100.0]),
                    sample_diameter: 1.0,
                    ..mapping
                },
                64,
                64,
            );
            rt.engine_mut()
                .attach_runtime_node(id, Box::new(fixture), current_revision())
                .expect("attach");
        }
        loaded = Some(rt);
    });
    let mut rt = loaded.expect("loaded");
    run(PHASES[1], &mut || {
        rt.engine_mut()
            .set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
                lp_shader::ShaderFrontend::LpsGlsl,
            ))));
    });
    for label in &PHASES[2..] {
        run(label, &mut || {
            rt.tick(16).expect("tick");
        });
    }
    drop(rt);
    phases
}

fn print_table(shape: Shape6, format6: bool, phases: &[Phase]) {
    println!(
        "\n== {} as format {} ==",
        shape.label(),
        if format6 { "6" } else { "5 (twin)" }
    );
    println!(
        "{:<20} {:>12} {:>12} {:>12}",
        "phase", "live-before", "transient", "resident-d"
    );
    for p in phases {
        println!(
            "{:<20} {:>12} {:>12} {:>12}",
            p.label,
            p.live_before,
            p.transient(),
            p.resident()
        );
    }
}

/// The project: a shader on `bus:visual.out`, one fixture `strip` (its map2d
/// is the twin's document, or a one-lamp placeholder for format 6), and one
/// output `main` whose one port takes everything.
fn project_fs(shape: Shape6, format6: bool) -> LpFsMemory {
    let fs = LpFsMemory::new();
    let write = |path: &str, text: &str| {
        fs.write_file(path.as_path(), text.as_bytes())
            .expect("write project file");
    };
    write("/project.json", "{\n  \"format\": 11\n}\n");
    write(
        "/module.json",
        r#"{ "kind": "Module", "nodes": {
  "shader": { "ref": "./shader.json" },
  "strip": { "ref": "./strip.json" },
  "out": { "ref": "./out.json" }
} }"#,
    );
    write(
        "/shader.json",
        r#"{ "kind": "Shader", "source": "shader.glsl", "render_order": 0,
  "bindings": { "output": { "target": "bus:visual.out" } }, "consumed": {} }"#,
    );
    write(
        "/shader.glsl",
        "layout(binding = 0) uniform vec2 outputSize;\n\
         vec4 render_2d(vec2 pos) {\n\
         \x20   vec2 uv = pos / outputSize;\n\
         \x20   return vec4(uv.x, uv.y, 0.5, 1.0);\n\
         }\n",
    );
    write(
        "/strip.json",
        r#"{
  "kind": "Fixture",
  "render_size": { "width": 64, "height": 64 },
  "bindings": { "input": { "source": "bus:visual.out" }, "output": { "target": "bus:control.out" } },
  "sampling": "direct",
  "diagnostic_mode": "off",
  "mapping": { "kind": "Map2d", "source": "strip.map2d.json" },
  "color_order": "rgb",
  "brightness": 1.0,
  "gamma_correction": false,
  "power": { "lamp_type": "ws2812b_5v", "budget_ma": 0 }
}"#,
    );
    let map2d = if format6 {
        Shape6::Path(1).map2d()
    } else {
        shape.map2d()
    };
    write("/strip.map2d.json", &map2d);
    write(
        "/out.json",
        r#"{
  "kind": "Output",
  "name": "main",
  "ports": { "0": { "endpoint": "ws281x:local:D10" } },
  "bindings": { "input": { "source": "bus:control.out" } }
}"#,
    );
    fs
}

// --- the tracking allocator -------------------------------------------------

struct TrackingAlloc;

thread_local! {
    /// Net bytes this thread allocated minus what it freed (signed: a thread
    /// may free what another allocated).
    static LIVE: Cell<isize> = const { Cell::new(0) };
    /// The highest `LIVE` since the last reset.
    static PEAK: Cell<isize> = const { Cell::new(0) };
}

fn add_live(delta: isize) {
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

fn live() -> i64 {
    LIVE.try_with(Cell::get).unwrap_or(0) as i64
}

fn reset_peak() {
    let level = LIVE.try_with(Cell::get).unwrap_or(0);
    let _ = PEAK.try_with(|peak| peak.set(level));
}

fn peak() -> i64 {
    PEAK.try_with(Cell::get).unwrap_or(0) as i64
}
