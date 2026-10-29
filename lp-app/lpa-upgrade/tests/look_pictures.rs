//! The look pictures: what a fixture's own shader actually shows at its
//! lamps under each answer to O1 (and G1 question 3's two `shadertoy`
//! shapes), drawn as dots where the lamps are.
//!
//! Each picture runs the project in today's live engine, with the real CPU
//! graphics tier. A dev-only seam in the recording backend
//! (`sampling_support::recording_graphics::Swap`) hands that fixture's
//! shader the coordinates and `outputSize` / pattern uniforms an answer
//! gives, in place of today's, and keeps the colour it answers per lamp.
//! Everything else (time, phasors, palettes, the compile) is the engine's
//! own. A pixel shader under `shadertoy` gets `pos` after the format step's
//! GLSL y flip, which is the same as running the flipped GLSL.
//!
//! Without `LP_LOOK_PICTURES` it checks the seam on the choker; with it,
//! it writes one HTML contact sheet per fixture into that directory, with
//! `sheets.txt` listing each sheet's size for the screenshot step:
//!
//! ```bash
//! LP_LOOK_PICTURES=/path/look-pictures cargo test -p lpa-upgrade --test look_pictures
//! ```

mod sampling_support;

use std::fmt::Write as _;
use std::path::Path;

use lpa_upgrade::v12::{V12Coords, V12Sampled, sample_coordinates};
use lpc_mapping::space::{
    Fit, ProductBox, ProductBoxKind, Sample, SpaceTransform, UnitScope, ViewBox,
};
use lps_shared::LpsValueF32;
use sampling_support::live_streams::swapped_frames;
use sampling_support::project_tree::{ProjectTree, workspace_dir};
use sampling_support::recording_graphics::Swap;
use sampling_support::scratch_copy::ScratchCopy;
use sampling_support::space_options::{bounds, doc_pitch, fill, footprint, lean_box, old_canvas};

/// `shadertoy`'s default view, N × N (G1's O3 lean).
const N: f32 = 64.0;

/// The moments drawn, from load.
const TIMES_MS: [u32; 3] = [1_000, 3_000, 5_500];

/// Today's coordinates, handed back through the seam, light every lamp of
/// the choker.
#[test]
fn todays_coordinates_through_the_seam_light_every_choker_lamp() {
    let workspace = workspace_dir();
    let row = &rows()[0];
    let setup = Setup::new(&workspace, row);
    let frames = setup.frames(&Column::Today, &[1_000]);
    assert!(frames[0].iter().all(|c| c.is_some()), "every lamp answered");
    assert!(
        frames[0]
            .iter()
            .flatten()
            .any(|c| c[0].max(c[1]).max(c[2]) > 2_000),
        "the choker shows something"
    );
    if let Some(dir) = std::env::var_os("LP_LOOK_PICTURES") {
        write_pictures(&workspace, Path::new(&dir));
    }
}

// ---- what is drawn ----------------------------------------------------------

struct Row {
    title: &'static str,
    project: &'static str,
    fixture: &'static str,
    shader: &'static str,
    /// A playlist entry to start on (playlist file, the entry's node ref
    /// contains this).
    entry: Option<(&'static str, &'static str)>,
    /// Today's lamps sit in a sliver of a long, thin render (the hybrid
    /// un-squeezes these).
    squeezed: bool,
}

fn rows() -> Vec<Row> {
    let row = |title, project, fixture, shader, squeezed| Row {
        title,
        project,
        fixture,
        shader,
        entry: None,
        squeezed,
    };
    vec![
        row(
            "PLAYFUL choker",
            "catalog/projects/playful-choker",
            "fixture.json",
            "shader.json",
            false,
        ),
        row(
            "Sean's eaves",
            "projects/test/eaves",
            "fixture.json",
            "effect/shader.json",
            false,
        ),
        row(
            "F-YEAH sign (idle)",
            "catalog/projects/fyeah-sign",
            "fixture.json",
            "idle.json",
            false,
        ),
        row(
            "Pulse (catalog)",
            "catalog/patterns/pulse",
            "fixture.json",
            "effect/shader.json",
            true,
        ),
        row(
            "Meteor (catalog)",
            "catalog/patterns/meteor",
            "fixture.json",
            "effect/render.json",
            true,
        ),
        row(
            "Quad test strip",
            "projects/test/quad-strips-1fix",
            "fixture1.json",
            "shader.json",
            true,
        ),
        row(
            "Five-wire",
            "projects/test/five-wire",
            "fixture.json",
            "shader.json",
            false,
        ),
        row(
            "Choker tryout (noise soft)",
            "catalog/projects/playful-choker-tryout",
            "fixture.json",
            "modules/noise_soft/shader.json",
            false,
        ),
        Row {
            entry: Some(("playlist.json", "spiral")),
            ..row(
                "Choker tryout (spiral)",
                "catalog/projects/playful-choker-tryout",
                "fixture.json",
                "modules/spiral/shader.json",
                false,
            )
        },
        row(
            "Circular display",
            "projects/test/circular-display-6",
            "fixture.json",
            "shader.json",
            false,
        ),
    ]
}

/// One column: today, or the box, fit and `shadertoy` shape an answer gives.
#[derive(Clone, Copy)]
enum Column {
    Today,
    /// (a): unit → the lamps' bounds, cover; pixels → the old canvas, fill.
    A,
    /// (b): the old canvas everywhere.
    B,
    /// (c): unit → the lamps' bounds, cover; pixels → the render's footprint, fill.
    C,
    /// (c), but a squeezed fixture's pixels take (a)'s old canvas.
    Hybrid,
    /// G1 Q3: a 64 × 64 square `shadertoy` view, on (c)'s box.
    SquareView,
    /// G1 Q3: a `shadertoy` view of the old canvas's shape, 64 on its short
    /// side, on the old canvas.
    BoxShapedView,
}

impl Column {
    /// The column's heading, in plain words for this fixture's shader.
    fn name(self, unit: bool) -> &'static str {
        match (self, unit) {
            (Self::A, true) => return "(a) the lean: the lamps' bounds + cover",
            (Self::B, true) => return "(b) old canvas + cover",
            (Self::C, true) => return "(c): same as (a) for a unit shader",
            (Self::Hybrid, true) => {
                return "Hybrid, the upgrade rule: same as (a) for a unit shader";
            }
            _ => {}
        }
        match self {
            Self::Today => "Today",
            Self::A => "(a) the lean: old canvas + fill",
            Self::B => "(b) old canvas everywhere",
            Self::C => "(c) render footprint + fill",
            Self::Hybrid => {
                "Hybrid, the upgrade rule: (c), squeezed strips on their lamps' bounds, shadertoy the box's shape"
            }
            Self::SquareView => "shadertoy 64 × 64 square",
            Self::BoxShapedView => "shadertoy the box's shape, 64 on the short side",
        }
    }
}

const MAIN: [Column; 5] = [
    Column::Today,
    Column::A,
    Column::B,
    Column::C,
    Column::Hybrid,
];
const Q3: [Column; 3] = [Column::Today, Column::SquareView, Column::BoxShapedView];

/// A row read and ready to run.
struct Setup {
    run_dir: std::path::PathBuf,
    _copy: Option<ScratchCopy>,
    glsl: String,
    glsl_file: String,
    unit: bool,
    squeezed: bool,
    positions: Vec<[f32; 2]>,
    lamp_bounds: [f32; 4],
    pitch: f64,
    view_box: [f32; 4],
    render_size: [u32; 2],
    today: V12Sampled,
}

impl Setup {
    fn new(workspace: &Path, row: &Row) -> Self {
        let dir = workspace.join(row.project);
        let tree = ProjectTree::read(workspace, &dir);
        let fixture = tree
            .fixtures
            .iter()
            .find(|f| f.file == row.fixture)
            .unwrap_or_else(|| panic!("{}: no {}", row.project, row.fixture));
        let shader = tree
            .shaders
            .iter()
            .find(|s| s.file == row.shader)
            .unwrap_or_else(|| panic!("{}: no {}", row.project, row.shader));
        let lamps = fixture.lamps.as_ref().expect("lamps");
        let today = sample_coordinates(&fixture.v12().unwrap(), &shader.source).expect("today");
        let copy = row.entry.map(|(playlist, entry)| {
            let text = std::fs::read_to_string(dir.join(playlist)).unwrap();
            let json: serde_json::Value = serde_json::from_str(&text).unwrap();
            let key = json["entries"]
                .as_object()
                .unwrap()
                .iter()
                .find(|(_, e)| e["node"]["ref"].as_str().is_some_and(|r| r.contains(entry)))
                .map(|(k, _)| k.parse::<u64>().unwrap())
                .expect("the entry");
            ScratchCopy::with_idle_entry(&dir, playlist, key)
        });
        let lamp_bounds = bounds(&lamps.positions);
        Self {
            run_dir: copy.as_ref().map_or(dir.clone(), |c| c.dir.clone()),
            _copy: copy,
            glsl: shader.glsl.clone(),
            glsl_file: shader.glsl_file.clone(),
            unit: shader.source.coords == V12Coords::Pattern,
            squeezed: row.squeezed,
            pitch: doc_pitch(&lamps.positions, &lamps.segment_counts, lamp_bounds),
            positions: lamps.positions.clone(),
            lamp_bounds,
            view_box: lamps.view_box,
            render_size: fixture.render_size,
            today,
        }
    }

    /// The box, fit and product a column gives; `None` for today.
    fn space(&self, column: Column) -> Option<([f32; 4], Fit, ProductBox)> {
        let canvas = old_canvas(self.view_box, self.lamp_bounds);
        let lean = lean_box(self.lamp_bounds, self.pitch).unwrap_or(canvas);
        let square = ProductBox::shadertoy_2d(N as u32).unwrap();
        if self.unit {
            let unit = ProductBox::unit_2d();
            return match column {
                Column::Today => None,
                Column::B => Some((canvas, Fit::DEFAULT, unit)),
                _ => Some((lean, Fit::DEFAULT, unit)),
            };
        }
        let footprint = footprint(canvas, self.render_size);
        Some(match column {
            Column::Today => return None,
            Column::A | Column::B => (canvas, fill(), square),
            Column::C | Column::SquareView => (footprint, fill(), square),
            // The upgrade rule (G1): a squeezed strip's box is its lamps'
            // bounds, any other pixel fixture's the render's footprint, and
            // `shadertoy` takes the box's shape, 64 on the short side.
            Column::Hybrid => {
                let box_ = if self.squeezed { lean } else { footprint };
                let view = ViewBox::from_array(box_).unwrap();
                (
                    box_,
                    fill(),
                    ProductBox::shadertoy_for(Some(&view), N as u32).unwrap(),
                )
            }
            Column::BoxShapedView => {
                let [_, _, w, h] = canvas;
                let (pw, ph) = if w >= h {
                    (N * w / h, N)
                } else {
                    (N, N * h / w)
                };
                let product =
                    ProductBox::new(ProductBoxKind::View, 0.0, 0.0, pw, ph, true).unwrap();
                (canvas, fill(), product)
            }
        })
    }

    /// The words and uniforms a column hands the shader, and which lamps it
    /// leaves dark (outside the box).
    fn swap(&self, column: Column) -> (Swap, Vec<bool>) {
        let lamps = self.positions.len();
        let mut swap = Swap {
            glsl: self.glsl.clone(),
            lanes: self.today.lanes(),
            today: self.today.words.clone(),
            words: self.today.words.clone(),
            uniforms: Vec::new(),
            colors: vec![None; lamps],
        };
        let mut dark = vec![false; lamps];
        let Some((box_, fit, product)) = self.space(column) else {
            return (swap, dark);
        };
        let t = SpaceTransform::new(ViewBox::from_array(box_).unwrap(), fit, product).unwrap();
        let height_q16 = (f64::from(product.height()) * 65536.0).round() as i32;
        for (k, lamp) in self.positions.iter().enumerate() {
            match t.apply(*lamp) {
                Sample::At(q) | Sample::Outside(q) => {
                    swap.words[2 * k] = q[0];
                    // A pixel shader runs after the step's GLSL flip
                    // (`pos.y = outputSize.y - pos.y`).
                    swap.words[2 * k + 1] = if self.unit { q[1] } else { height_q16 - q[1] };
                }
                Sample::Dark => dark[k] = true,
            }
        }
        if self.unit {
            let scope = UnitScope::new(&t, self.pitch as f32, lamps as u32);
            swap.uniforms = vec![
                ("patternExtent".into(), LpsValueF32::Vec2(scope.extent)),
                ("patternPitch".into(), LpsValueF32::F32(scope.pitch)),
                ("lampCount".into(), LpsValueF32::F32(lamps as f32)),
            ];
        } else {
            swap.uniforms = vec![(
                "outputSize".into(),
                LpsValueF32::Vec2([product.width(), product.height()]),
            )];
        }
        (swap, dark)
    }

    fn frames(&self, column: &Column, times: &[u32]) -> Vec<Vec<Option<[u16; 4]>>> {
        let (swap, _) = self.swap(*column);
        swapped_frames(&self.run_dir, swap, times).expect("frames")
    }
}

// ---- the sheets ------------------------------------------------------------------

/// Cell width, CSS pixels.
const CELL_W: f64 = 220.0;
const CELL_MAX_H: f64 = 200.0;

fn write_pictures(workspace: &Path, out: &Path) {
    std::fs::create_dir_all(out).unwrap();
    let mut sheets = String::new();
    let mut findings = String::new();
    for (index, row) in rows().iter().enumerate() {
        let setup = Setup::new(workspace, row);
        let name = format!("{:02}-{}", index + 1, slug(row.title));
        let (html, size, notes) = sheet(&setup, row.title, &MAIN);
        std::fs::write(out.join(format!("{name}.html")), html).unwrap();
        let _ = writeln!(sheets, "{name} {} {}", size.0, size.1);
        let _ = writeln!(findings, "{} ({}): {}", row.title, setup.glsl_file, notes);
    }
    let q3 = [
        rows().into_iter().next().unwrap(),
        Row {
            title: "F-YEAH sign (blast)",
            project: "catalog/projects/fyeah-sign",
            fixture: "fixture.json",
            shader: "blast.json",
            entry: Some(("playlist.json", "blast")),
            squeezed: false,
        },
    ];
    for (index, row) in q3.iter().enumerate() {
        let setup = Setup::new(workspace, row);
        let title = format!("{}: the shadertoy view's shape (G1 question 3)", row.title);
        let name = format!("q3-{:02}-{}", index + 1, slug(row.title));
        let (html, size, notes) = sheet(&setup, &title, &Q3);
        std::fs::write(out.join(format!("{name}.html")), html).unwrap();
        let _ = writeln!(sheets, "{name} {} {}", size.0, size.1);
        let _ = writeln!(findings, "{title} ({}): {}", setup.glsl_file, notes);
    }
    std::fs::write(out.join("sheets.txt"), sheets).unwrap();
    std::fs::write(out.join("findings.txt"), findings).unwrap();
}

/// One fixture's sheet: a column per answer, a row per moment. Returns the
/// page, its size in CSS pixels, and a plain line on any cell that looks
/// broken.
fn sheet(setup: &Setup, title: &str, columns: &[Column]) -> (String, (u32, u32), String) {
    let [bx, by, bw, bh] = setup.lamp_bounds;
    let pad = (setup.pitch as f32).max(bw.max(bh) * 0.02);
    let (x0, y0) = (bx - pad, by - pad);
    let (w, h) = (f64::from(bw + 2.0 * pad), f64::from(bh + 2.0 * pad));
    let scale = (CELL_W / w).min(CELL_MAX_H / h);
    let (cw, ch) = (w * scale, (h * scale).max(24.0));
    let radius = (setup.pitch * scale * 0.42).clamp(1.6, 9.0);

    let mut body = String::new();
    let mut notes = Vec::new();
    let _ = write!(body, "<tr><th></th>");
    for column in columns {
        let _ = write!(body, "<th>{}</th>", column.name(setup.unit));
    }
    let _ = write!(body, "</tr>");
    let per_column: Vec<(Vec<Vec<Option<[u16; 4]>>>, Vec<bool>)> = columns
        .iter()
        .map(|c| (setup.frames(c, &TIMES_MS), setup.swap(*c).1))
        .collect();
    for (t, at) in TIMES_MS.iter().enumerate() {
        let _ = write!(
            body,
            "<tr><td class=t>{:.1} s</td>",
            f64::from(*at) / 1000.0
        );
        for (column, (frames, dark)) in columns.iter().zip(&per_column) {
            let colors = &frames[t];
            let verdict = judge(colors, dark);
            if let Some(v) = &verdict {
                notes.push(format!(
                    "{} at {:.1} s: {v}",
                    column.name(setup.unit),
                    f64::from(*at) / 1000.0
                ));
            }
            let mut svg =
                format!("<svg width='{cw:.0}' height='{ch:.0}' viewBox='0 0 {cw:.0} {ch:.0}'>");
            for (k, p) in setup.positions.iter().enumerate() {
                let x = f64::from(p[0] - x0) * scale;
                let y = f64::from(p[1] - y0) * scale + (ch - h * scale).max(0.0) / 2.0;
                if dark[k] {
                    let _ = write!(
                        svg,
                        "<circle cx='{x:.1}' cy='{y:.1}' r='{radius:.1}' fill='none' stroke='#666' stroke-dasharray='2 1.5'/>"
                    );
                    continue;
                }
                let c = colors[k].unwrap_or([0, 0, 0, 0]);
                let _ = write!(
                    svg,
                    "<circle cx='{x:.1}' cy='{y:.1}' r='{radius:.1}' fill='rgb({},{},{})'/>",
                    c[0] >> 8,
                    c[1] >> 8,
                    c[2] >> 8
                );
            }
            svg.push_str("</svg>");
            let _ = write!(
                body,
                "<td>{svg}<div class=v>{}</div></td>",
                verdict.as_deref().unwrap_or("&nbsp;")
            );
        }
        let _ = write!(body, "</tr>");
    }
    let width = 70 + columns.len() as u32 * (CELL_W as u32 + 24) + 24;
    let height = 118 + TIMES_MS.len() as u32 * (ch as u32 + 34);
    let html = format!(
        "<!doctype html><meta charset=utf-8><style>\
         body{{margin:0;padding:12px;background:#1b1b1d;color:#e8e8ea;font:13px -apple-system,Helvetica,sans-serif}}\
         h1{{font-size:16px;margin:0 0 2px}} .sub{{color:#9a9aa0;margin-bottom:10px}}\
         table{{border-collapse:separate;border-spacing:8px 6px}} th{{font-weight:600;text-align:left;width:{CELL_W}px;vertical-align:bottom}}\
         td{{vertical-align:top}} svg{{background:#050506;border-radius:4px;display:block}}\
         .t{{color:#9a9aa0;width:40px}} .v{{color:#ff9f43;font-size:12px;height:16px}}\
         </style><h1>{title}</h1><div class=sub>shader <code>{}</code> · {} lamps · dots where the lamps are, coloured by what the shader answers there</div>\
         <table>{body}</table>",
        setup.glsl_file,
        setup.positions.len()
    );
    let notes = if notes.is_empty() {
        "nothing looks broken".to_string()
    } else {
        notes.join("; ")
    };
    (html, (width, height), notes)
}

/// A plain word for a cell that looks broken: every lamp black, every lamp
/// near white, or every lamp one colour.
fn judge(colors: &[Option<[u16; 4]>], dark: &[bool]) -> Option<String> {
    let lit: Vec<[f64; 3]> = colors
        .iter()
        .zip(dark)
        .filter(|(_, d)| !**d)
        .map(|(c, _)| {
            let c = c.unwrap_or([0; 4]);
            [c[0], c[1], c[2]].map(|v| f64::from(v) / 65535.0)
        })
        .collect();
    let dark_count = dark.iter().filter(|d| **d).count();
    let mut words = Vec::new();
    if dark_count > 0 {
        words.push(format!("{dark_count} lamps off (outside the box)"));
    }
    if lit.is_empty() {
        return Some(words.join(", "));
    }
    let max = lit
        .iter()
        .map(|c| c[0].max(c[1]).max(c[2]))
        .fold(0.0, f64::max);
    let min_luma = lit
        .iter()
        .map(|c| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2])
        .fold(1.0, f64::min);
    let spread = (0..3)
        .map(|i| {
            let (lo, hi) = lit
                .iter()
                .fold((1.0f64, 0.0f64), |(lo, hi), c| (lo.min(c[i]), hi.max(c[i])));
            hi - lo
        })
        .fold(0.0, f64::max);
    if max < 0.03 {
        words.push("black".to_string());
    } else if min_luma > 0.85 {
        words.push("washed out".to_string());
    } else if spread < 0.03 {
        words.push("one flat colour".to_string());
    }
    (!words.is_empty()).then(|| words.join(", "))
}

fn slug(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}
