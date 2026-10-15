//! How long the model's per-frame work takes on the radiance dome (and the
//! two small projects), natively. The page asks the model these questions
//! every time it draws; this times them one by one, plus a "page frame"
//! that asks what the canvas and the panels ask.
//!
//! ```bash
//! cargo run --release -p lab-mapping-files --example scale_bench
//! cargo run --release -p lab-mapping-files --example scale_bench -- radiance-dome 50   # or `all 50`
//! ```
//!
//! Each figure is the median of N runs (default 20), in milliseconds. The
//! model caches what it derives from a project until the project changes,
//! so "cold" is the first ask after an edit and "warm" every ask after it.

use std::time::Instant;

use lab_mapping_files::{Opened, embedded};
use lab_mapping_model::{Editor, Mods, Project, Target, Vec2, common_props};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let only = args.first().filter(|k| *k != "all").cloned();
    let runs: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(20);
    for p in embedded::ALL {
        if only.as_ref().is_some_and(|k| k != p.key) {
            continue;
        }
        bench(p.key, &p.files(), runs);
    }
}

fn bench(key: &str, files: &lab_mapping_files::MemFiles, runs: usize) {
    let open = median(runs.min(5), || {
        let _ = Opened::open(files).unwrap();
    });
    let project = Opened::open(files).unwrap().project;
    println!(
        "\n== {key}: {} lamps, {} objects, {} nodes ({runs} runs, median ms)",
        project.lamp_count(),
        project.objects().len(),
        project.all_nodes().len()
    );
    row("open (parse + build)", open);

    // Cold: a fresh clone each run has nothing derived yet.
    let cold = |f: &dyn Fn(&Project)| {
        median(runs, || {
            let p = fresh(&project);
            f(&p);
        })
    };
    let warm = |f: &dyn Fn(&Project)| {
        let p = fresh(&project);
        f(&p);
        median(runs, || f(&p))
    };
    let both = |name: &str, f: &dyn Fn(&Project)| {
        println!("  {name:<34} cold {:>9.3}  warm {:>9.3}", cold(f), warm(f));
    };
    both("layout", &|p| {
        let _ = p.layout();
    });
    both("placed objects", &|p| {
        let _ = p.placed_objects();
    });
    both("effective patch", &|p| {
        let _ = p.effective_patch();
    });
    both("device frame (nothing selected)", &|p| {
        let _ = p.device_frame(&[], 1.0);
    });
    let first_component = project
        .objects()
        .first()
        .map(|o| Target::Component(o.id.component.clone()));
    if let Some(t) = &first_component {
        let sel = vec![t.clone()];
        both("device frame (a repeat selected)", &|p| {
            let _ = p.device_frame(&sel, 1.0);
        });
    }
    both("tree rows", &|p| {
        let _ = p.tree_rows();
    });
    let fixtures: Vec<_> = project.fixtures().iter().map(|n| n.id.clone()).collect();
    both("data path (every fixture)", &|p| {
        for f in &fixtures {
            let _ = p.data_path(f);
        }
    });

    // The editor's questions, at the whole view and zoomed in on a lamp.
    let mut e = Editor::new(fresh(&project));
    e.resize(Vec2::new(1200.0, 800.0));
    e.fit();
    let lamp = project
        .objects()
        .get(project.objects().len() / 2)
        .and_then(|o| o.lamps.first().map(|l| l.pos))
        .unwrap_or_default();
    for (view, zoom) in [("whole", 1.0), ("zoomed ×8", 8.0)] {
        let mut ed = e.clone();
        let at = ed.camera.to_screen(lamp);
        ed.camera.zoom_at(at, zoom);
        let at = ed.camera.to_screen(lamp);
        ed.pointer_move(at, Mods::default());
        let hover = ed.hover_target();
        println!(
            "  -- {view}: hovering {:?}",
            hover.map(|t| ed.project.label(&t))
        );
        let warm_ed = |name: &str, f: &dyn Fn(&Editor)| {
            f(&ed);
            println!(
                "  {name:<34}                 warm {:>9.3}",
                median(runs, || f(&ed))
            );
        };
        warm_ed("hover target", &|ed| {
            let _ = ed.hover_target();
        });
        warm_ed("things at the pointer", &|ed| {
            let _ = ed.project.things_at(&ed.camera, at);
        });
        warm_ed("edges", &|ed| {
            let _ = ed.edges();
        });
        warm_ed("hint bar", &|ed| {
            let _ = ed.hint_bar();
        });
        warm_ed("page frame (canvas + panels)", &|ed| page_frame(ed));
        // A pointer move: the editor takes the event, then the page asks.
        let mut moving = ed.clone();
        let mut k = 0.0;
        let t = median(runs, || {
            k += 1.0;
            moving.pointer_move(at + Vec2::new(k % 7.0, 0.0), Mods::default());
            page_frame(&moving);
        });
        row("pointer move + page frame", t);
    }
}

/// What the page asks the model each time it draws (canvas.rs, panels.rs).
fn page_frame(e: &Editor) {
    let p = &e.project;
    let _ = p.effective_patch();
    let _ = e.context();
    let _ = e.hover_target();
    let _ = e.open_fixtures();
    for n in p.all_nodes() {
        let _ = p.node_rect(&n.id);
        if let Some(o) = n.output() {
            for port in &o.ports {
                let _ = p.pixel_grid(&n.id, &port.key);
                let _ = p.port_pixels(&n.id, &port.key);
            }
        }
    }
    let _ = e.edges();
    let _ = e.marked_lamps();
    let _ = e.marked_pixels();
    for (f, o) in p.placed_objects() {
        let t = p.object_target(&o.id);
        let _ = p.chain(&t);
        let _ = p.node_rect(&f);
    }
    for n in p.fixtures() {
        let _ = p.data_path(&n.id);
    }
    let _ = e.handles();
    for t in &e.selection {
        let _ = p.area(&e.camera, t);
    }
    let _ = e.cursor_note();
    let _ = e.device_frame(1.0);
    // Panels.
    let _ = p.tree_rows();
    let _ = common_props(p, &e.selection);
    let _ = e.hint_bar();
}

/// The project as if just opened: nothing derived yet.
fn fresh(p: &Project) -> Project {
    let mut q = p.clone();
    q.name.push(' ');
    q.name.pop();
    q
}

fn median(runs: usize, mut f: impl FnMut()) -> f64 {
    let mut times: Vec<f64> = (0..runs.max(1))
        .map(|_| {
            let t = Instant::now();
            f();
            t.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    times.sort_by(f64::total_cmp);
    times[times.len() / 2]
}

fn row(name: &str, ms: f64) {
    println!("  {name:<34}                 warm {ms:>9.3}");
}
