//! The look report: for every fixture of every repo project, how far each
//! answer to open question O1 ("how upgraded projects keep their look")
//! moves what its shaders see at its lamps.
//!
//! Today's side is the frozen format-12 copy (`lpa_upgrade::v12`, proved
//! against the live engine by `v12_sampling_live`). The new side is the
//! space core's T (`lpc_mapping::space`) under each option. Which shaders
//! feed a fixture is what the live engine samples for it (every playlist
//! entry run, as in `v12_sampling_live`).
//!
//! Without `LP_LOOK_REPORT` it runs its assertions and writes nothing; with
//! it, it writes the report there:
//!
//! ```bash
//! LP_LOOK_REPORT=/path/look-report.md cargo test -p lpa-upgrade --test look_report
//! ```

mod sampling_support;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use lpa_upgrade::v12::{V12Coords, V12Sampled, V12Space, sample_coordinates};
use lpc_mapping::space::{Fit, ProductBox, Sample, SpaceTransform, ViewBox, order_position_q16};
use sampling_support::live_streams::{attribute, record_frame};
use sampling_support::project_tree::{
    FixtureNode, ProjectTree, ShaderNode, repo_projects, workspace_dir,
};
use sampling_support::scratch_copy::ScratchCopy;
use sampling_support::space_options::{self, bounds, doc_pitch, fill, footprint, lean_box};

/// `shadertoy`'s default view, N × N (G1's O3 lean).
const SHADERTOY_N: u32 = 64;

/// Under the lean (a), every lamp a `unit` source feeds moves by less than
/// a hundredth of a lamp pitch; the report says how much, exactly.
#[test]
fn under_the_lean_no_unit_lamp_moves_a_hundredth_of_a_pitch() {
    let report = build_report();
    assert!(
        report.fixtures.len() > 120,
        "{} fixtures",
        report.fixtures.len()
    );
    assert!(
        report
            .fixtures
            .iter()
            .any(|f| f.project == "catalog/projects/playful-choker"),
        "the choker is in the report"
    );
    for fixture in &report.fixtures {
        for source in &fixture.sources {
            if source.coords == V12Coords::Pattern && source.two_d {
                let a = &source.moves[0];
                assert!(
                    a.pitches < 0.01,
                    "{} {} × {}: a unit source moves {} pitches under (a)",
                    fixture.project,
                    fixture.file,
                    source.shader,
                    a.pitches
                );
            }
        }
    }
    if let Some(path) = std::env::var_os("LP_LOOK_REPORT") {
        std::fs::write(&path, render(&report)).expect("write the look report");
        eprintln!("wrote {}", path.to_string_lossy());
    }
}

// ---- the measurements -------------------------------------------------------

struct Report {
    projects: usize,
    fixtures: Vec<FixtureRow>,
    /// Pixel shaders whose GLSL uses `outputSize` other than as
    /// `pos / outputSize`: (shader GLSL file, line number, the line).
    output_size_uses: BTreeMap<(String, String), Vec<(usize, String)>>,
    /// Shader files no fixture sampled (diagnostic fixtures, unused).
    unsampled_shaders: Vec<(String, String)>,
}

struct FixtureRow {
    project: String,
    file: String,
    lamps: usize,
    view_box: [f32; 4],
    lamp_bounds: [f32; 4],
    /// The lean's box: the lamp bounds, a thin axis padded to one pitch.
    lean_box: Option<[f32; 4]>,
    /// The lamp pitch in the drawing's own units.
    pitch: f64,
    render_size: [u32; 2],
    /// For a fixture a 2D pixel source feeds: how much of today's render
    /// its lamps span, as fractions of the width and the height.
    today_span: Option<[f64; 2]>,
    sources: Vec<SourceRow>,
    /// Why no shader is measured (texture-area, diagnostic, fluid…).
    note: Option<String>,
}

struct SourceRow {
    shader: String,
    glsl: String,
    coords: V12Coords,
    two_d: bool,
    /// [lean (a), old canvas (b), the render's footprint (c)].
    moves: [Move; 3],
}

#[derive(Default, Clone)]
struct Move {
    /// Largest move: for `unit`, in unit coordinates; for pixel sources,
    /// in pixels of today's render.
    units: f64,
    /// The same, in lamp pitches.
    pitches: f64,
    /// For `unit`, the largest move in Q16.16 steps.
    q16_steps: i64,
    /// Lamps outside the option's box (dark under it).
    dark: usize,
    /// `patternExtent` today and under the option, for `unit`.
    extent: Option<([f32; 2], [f32; 2])>,
    /// The fit the option uses for this fixture.
    fit: &'static str,
    /// The box could not be built (refused).
    refused: Option<String>,
}

impl FixtureRow {
    fn kind(&self) -> &'static str {
        let unit = self.sources.iter().any(|s| s.coords == V12Coords::Pattern);
        let pixel = self.sources.iter().any(|s| s.coords == V12Coords::Pixels);
        match (unit, pixel) {
            (true, true) => "mixed",
            (true, false) => "unit",
            (false, true) => "pixels",
            (false, false) => "none",
        }
    }

    fn unit_fed(&self) -> bool {
        self.sources.iter().any(|s| s.coords == V12Coords::Pattern)
    }

    fn worst(&self, option: usize) -> f64 {
        self.sources
            .iter()
            .map(|s| s.moves[option].pitches)
            .fold(0.0, f64::max)
    }

    fn unchanged(&self, option: usize) -> bool {
        self.sources.iter().all(|s| {
            let m = &s.moves[option];
            m.units == 0.0 && m.dark == 0 && m.refused.is_none()
        })
    }
}

fn build_report() -> Report {
    let workspace = workspace_dir();
    let projects = repo_projects(&workspace);
    let mut fixtures = Vec::new();
    let mut output_size_uses = BTreeMap::new();
    let mut unsampled_shaders = Vec::new();
    for dir in &projects {
        let tree = ProjectTree::read(&workspace, dir);
        // Which shaders feed each fixture: what the live engine samples,
        // every playlist entry run.
        let mut fed: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut runs = vec![(dir.clone(), None)];
        let mut failing = false;
        for playlist in &tree.playlists {
            for entry in &playlist.entries {
                let Ok(key) = entry.parse::<u64>() else {
                    continue;
                };
                if Some(key) != playlist.idle_entry {
                    let copy = ScratchCopy::with_idle_entry(dir, &playlist.file, key);
                    runs.push((copy.dir.clone(), Some(copy)));
                }
            }
        }
        for (run_dir, _keep) in &runs {
            // A frame that fails part-way (the fault demo) has nothing to say.
            let Ok((log, None)) = record_frame(run_dir) else {
                failing = true;
                continue;
            };
            for m in attribute(&tree, &log).matched {
                fed.entry(m.fixture).or_default().insert(m.shader);
            }
        }
        let sampled: BTreeSet<&String> = fed.values().flatten().collect();
        for shader in &tree.shaders {
            if !sampled.contains(&shader.file) {
                unsampled_shaders.push((tree.name.clone(), shader.file.clone()));
            }
            let two_d = matches!(shader.source.space, lpa_upgrade::v12::V12SourceSpace::TwoD);
            if shader.source.coords == V12Coords::Pixels && two_d {
                let uses = output_size_lines(&shader.glsl);
                if !uses.is_empty() {
                    output_size_uses.insert((tree.name.clone(), shader.glsl_file.clone()), uses);
                }
            }
        }
        for fixture in &tree.fixtures {
            let shaders: Vec<&ShaderNode> = fed
                .get(&fixture.file)
                .map(|files| {
                    tree.shaders
                        .iter()
                        .filter(|s| files.contains(&s.file))
                        .collect()
                })
                .unwrap_or_default();
            let mut row = fixture_row(&tree, fixture, &shaders);
            if row.sources.is_empty() && failing {
                row.note = Some("its frame fails today (a shader that never finishes)".to_string());
            }
            fixtures.push(row);
        }
    }
    Report {
        projects: projects.len(),
        fixtures,
        output_size_uses,
        unsampled_shaders,
    }
}

fn fixture_row(tree: &ProjectTree, fixture: &FixtureNode, shaders: &[&ShaderNode]) -> FixtureRow {
    let mut row = FixtureRow {
        project: tree.name.clone(),
        file: fixture.file.clone(),
        lamps: 0,
        view_box: [0.0; 4],
        lamp_bounds: [0.0; 4],
        lean_box: None,
        pitch: 0.0,
        render_size: fixture.render_size,
        today_span: None,
        sources: Vec::new(),
        note: None,
    };
    let lamps = match &fixture.lamps {
        Ok(lamps) => lamps,
        Err(why) => {
            row.note = Some(format!("its map2d: {why}"));
            return row;
        }
    };
    row.lamps = lamps.positions.len();
    row.view_box = lamps.view_box;
    row.lamp_bounds = bounds(&lamps.positions);
    row.pitch = doc_pitch(&lamps.positions, &lamps.segment_counts, row.lamp_bounds);
    row.lean_box = lean_box(row.lamp_bounds, row.pitch);
    if shaders.is_empty() {
        row.note = Some(if let Some(mode) = &fixture.diagnostic {
            format!("diagnostic mode `{mode}`: its lamps show a test pattern")
        } else if !tree.fluids.is_empty() {
            "fed by a fluid (an extent, P4), not a shader".to_string()
        } else {
            "no shader sampled it per lamp".to_string()
        });
        if fixture.sampling == lpa_upgrade::v12::V12Sampling::TextureArea {
            row.note = Some("texture-area sampling: no per-lamp coordinate today".to_string());
        }
        return row;
    }
    let v12 = fixture.v12().expect("lamps read");
    let unit_wins = shaders
        .iter()
        .any(|s| s.source.coords == V12Coords::Pattern);
    for shader in shaders {
        let Ok(today) = sample_coordinates(&v12, &shader.source) else {
            continue;
        };
        let two_d = today.shader_space == V12Space::TwoD && today.request == V12Space::TwoD;
        if two_d && shader.source.coords == V12Coords::Pixels {
            let [w, h] = today.uniforms.output_size;
            let axis = |lane: usize, size: f32| {
                let values = today
                    .words
                    .iter()
                    .skip(lane)
                    .step_by(2)
                    .map(|&v| f64::from(v));
                let (lo, hi) =
                    values.fold((f64::MAX, f64::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)));
                (hi - lo) / 65536.0 / f64::from(size)
            };
            row.today_span = Some([axis(0, w), axis(1, h)]);
        }
        let moves = [Answer::Lean, Answer::OldCanvas, Answer::Footprint].map(|answer| {
            measure(
                &row,
                &lamps.positions,
                &today,
                shader,
                unit_wins,
                answer,
                fixture,
            )
        });
        row.sources.push(SourceRow {
            shader: shader.file.clone(),
            glsl: shader.glsl_file.clone(),
            coords: shader.source.coords,
            two_d,
            moves,
        });
    }
    row
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Answer {
    /// (a): unit-fed → the lamps' bounds, cover; pixel-only → the old canvas, fill.
    Lean,
    /// (b): the old canvas everywhere; cover when a unit source feeds it, else fill.
    OldCanvas,
    /// (c): as (a) for unit-fed fixtures; pixel-only → the drawing region
    /// today's render covers (the old canvas grown to `render_size`'s
    /// aspect, centred), fill.
    Footprint,
}

/// How far one source's coordinates move under one option.
fn measure(
    row: &FixtureRow,
    positions: &[[f32; 2]],
    today: &V12Sampled,
    shader: &ShaderNode,
    unit_wins: bool,
    option: Answer,
    fixture: &FixtureNode,
) -> Move {
    let old_canvas = old_canvas(row);
    let (box_, fit, fit_name) = match (option, unit_wins) {
        (Answer::Lean, true) => (row.lean_box, Fit::DEFAULT, "cover"),
        (Answer::Lean, false) => (Some(old_canvas), fill(), "fill"),
        (Answer::OldCanvas, true) => (Some(old_canvas), Fit::DEFAULT, "cover"),
        (Answer::OldCanvas, false) => (Some(old_canvas), fill(), "fill"),
        (Answer::Footprint, true) => (row.lean_box, Fit::DEFAULT, "cover"),
        (Answer::Footprint, false) => {
            (Some(footprint(old_canvas, row.render_size)), fill(), "fill")
        }
    };
    let mut out = Move {
        fit: fit_name,
        ..Move::default()
    };
    let one_d = today.shader_space == V12Space::OneD;
    if one_d && today.request == V12Space::OneD {
        // Along the order (the default landing, G1 Q2's lean): `unit` is
        // k/(N−1) exactly; a pixel 1D shader keeps `k + 0.5` of `(N, 1)`.
        if shader.source.coords == V12Coords::Pattern {
            let n = today.words.len() as u32;
            for (k, &word) in today.words.iter().enumerate() {
                let index = if fixture.wire_reversed {
                    n - 1 - k as u32
                } else {
                    k as u32
                };
                let new = order_position_q16(index, n);
                let steps = i64::from(new) - i64::from(word);
                out.q16_steps = out.q16_steps.max(steps.abs());
            }
            out.units = out.q16_steps as f64 / 65536.0;
            let pitch = today.uniforms.pattern.map_or(1.0, |p| f64::from(p.pitch));
            out.pitches = out.units / pitch;
        }
        out.fit = "along the order";
        return out;
    }
    let Some(box_) = box_ else {
        out.refused = Some("the lamps are on one spot: no box".to_string());
        return out;
    };
    let view_box = match ViewBox::from_array(box_) {
        Ok(v) => v,
        Err(e) => {
            out.refused = Some(format!("{e}"));
            return out;
        }
    };
    match shader.source.coords {
        V12Coords::Pattern => {
            let t = SpaceTransform::new(view_box, fit, ProductBox::unit_2d()).unwrap();
            let pitch = today.uniforms.pattern.map_or(1.0, |p| f64::from(p.pitch));
            for (k, lamp) in positions.iter().enumerate() {
                let Sample::At(q) = t.apply(*lamp) else {
                    out.dark += 1;
                    continue;
                };
                let was = [today.words[2 * k], today.words[2 * k + 1]];
                let steps = (i64::from(q[0]) - i64::from(was[0]))
                    .abs()
                    .max((i64::from(q[1]) - i64::from(was[1])).abs());
                out.q16_steps = out.q16_steps.max(steps);
                let d = distance(q, was) / 65536.0;
                out.units = out.units.max(d);
                out.pitches = out.pitches.max(d / pitch);
            }
            if let Some(p) = today.uniforms.pattern {
                out.extent = Some((p.extent, t.box_half_extent()));
            }
        }
        V12Coords::Pixels => {
            // `shadertoy`: fragCoord in N × N, y up; the step's GLSL flip
            // (`pos.y = outputSize.y - pos.y`) turns it back down, so the
            // shader's `pos / outputSize` is (qx / N, 1 − qy / N).
            let n = SHADERTOY_N;
            let t =
                SpaceTransform::new(view_box, fit, ProductBox::shadertoy_2d(n).unwrap()).unwrap();
            let [w, h] = [today.uniforms.output_size[0], today.uniforms.output_size[1]];
            let pitch_px = today_pitch_px(today, positions, fixture);
            for (k, lamp) in positions.iter().enumerate() {
                let Sample::At(q) = t.apply(*lamp) else {
                    out.dark += 1;
                    continue;
                };
                let uv_new = [
                    f64::from(q[0]) / 65536.0 / f64::from(n),
                    1.0 - f64::from(q[1]) / 65536.0 / f64::from(n),
                ];
                let uv_was = [
                    f64::from(today.words[2 * k]) / 65536.0 / f64::from(w),
                    f64::from(today.words[2 * k + 1]) / 65536.0 / f64::from(h),
                ];
                let dx = (uv_new[0] - uv_was[0]) * f64::from(w);
                let dy = (uv_new[1] - uv_was[1]) * f64::from(h);
                let d = (dx * dx + dy * dy).sqrt();
                out.units = out.units.max(d);
                if pitch_px > 0.0 {
                    out.pitches = out.pitches.max(d / pitch_px);
                }
            }
        }
    }
    out
}

/// The box today's fit frames for this fixture.
fn old_canvas(row: &FixtureRow) -> [f32; 4] {
    space_options::old_canvas(row.view_box, row.lamp_bounds)
}

/// Today's pitch in render pixels, from the stream the frozen copy gives.
fn today_pitch_px(today: &V12Sampled, positions: &[[f32; 2]], fixture: &FixtureNode) -> f64 {
    let segments = fixture
        .lamps
        .as_ref()
        .map(|l| l.segment_counts.clone())
        .unwrap_or_default();
    let pixels: Vec<[f32; 2]> = today
        .words
        .chunks_exact(2)
        .map(|w| [w[0] as f32 / 65536.0, w[1] as f32 / 65536.0])
        .collect();
    let _ = positions;
    doc_pitch(&pixels, &segments, bounds(&pixels))
}

fn distance(a: [i32; 2], b: [i32; 2]) -> f64 {
    let dx = f64::from(a[0]) - f64::from(b[0]);
    let dy = f64::from(a[1]) - f64::from(b[1]);
    (dx * dx + dy * dy).sqrt()
}

/// Lines of a 2D pixel shader's GLSL that use `outputSize` other than as
/// `pos / outputSize` (or `pos * k / outputSize`, the same ratio scaled):
/// under `shadertoy`, `outputSize` is (N, N), so those lines change. A 1D
/// shader's `outputSize` stays `(N, 1)`.
fn output_size_lines(glsl: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (number, line) in glsl.lines().enumerate() {
        let code: String = line
            .split("//")
            .next()
            .unwrap_or("")
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        if code.contains("uniformvec2outputSize") {
            continue;
        }
        let plain = |at: usize| {
            let before = &code[..at];
            let after = &code[at + "outputSize".len()..];
            let ratio = before.ends_with("pos/") || before.ends_with("pos*REF_SIZE/");
            ratio && !after.starts_with('.')
        };
        if code.match_indices("outputSize").any(|(at, _)| !plain(at)) {
            out.push((number + 1, line.trim().to_string()));
        }
    }
    out
}

// ---- the report ----------------------------------------------------------------

const NAMED: &[(&str, &str)] = &[
    ("catalog/projects/playful-choker", "the PLAYFUL choker"),
    (
        "catalog/projects/playful-choker-tryout",
        "the PLAYFUL choker tryout",
    ),
    ("projects/test/eaves", "the eaves"),
    ("projects/test/eaves-wired", "the eaves, wired"),
    ("projects/test/circular-display-6", "the circular display"),
    (
        "projects/test/circular-display-walk",
        "the circular display walk",
    ),
];

fn render(report: &Report) -> String {
    let mut s = String::new();
    let measured: Vec<&FixtureRow> = report
        .fixtures
        .iter()
        .filter(|f| !f.sources.is_empty())
        .collect();
    let count = |kind: &str| measured.iter().filter(|f| f.kind() == kind).count();
    let unchanged = |option: usize| measured.iter().filter(|f| f.unchanged(option)).count();
    let moves = |option: usize| {
        measured
            .iter()
            .filter(|f| f.worst(option) > 0.0 || !f.unchanged(option))
            .count()
    };

    let _ = writeln!(
        s,
        "# Look report: how upgraded projects keep their look (O1)\n"
    );
    let _ = writeln!(
        s,
        "Generated by `lp-app/lpa-upgrade/tests/look_report.rs` (`LP_LOOK_REPORT=<path> cargo \
         test -p lpa-upgrade --test look_report`). It reads every project in `catalog/` and \
         `projects/`.\n"
    );
    let _ = writeln!(s, "## How it was measured\n");
    let _ = writeln!(
        s,
        "- **Today** is what each shader sees at each lamp now. The frozen copy of today's \
         arithmetic (`lpa_upgrade::v12`) computes it, and it matches the live engine bit for bit \
         on every project (`tests/v12_sampling_live.rs`).\n\
         - **The new side** puts each lamp through the space core's T (`lpc_mapping::space`). \
         T is built from the box and the fit each option gives the fixture.\n\
         - **Which shaders feed a fixture** is what the live engine samples for it. Every \
         playlist entry is run, one copy of the project per entry. That is the same answer \
         the Graph's \"Samples X on visual.out\" gives, read off the engine instead.\n\
         - **`unit` sources** (`\"coords\": \"pattern\"` today) are compared in unit \
         coordinates. A move is also given in lamp pitches (today's `patternPitch`) and in Q16.16 \
         steps (1/65536).\n\
         - **Pixel sources** (`coords` absent) become `shadertoy` at N = {SHADERTOY_N}, with the \
         step's GLSL y flip. A move is what `pos / outputSize` reads, given in pixels of today's \
         render and in lamp pitches (today's pitch in render pixels). A shader that uses \
         `outputSize` any other way changes in addition to this, and is listed below.\n\
         - **1D sources** all run along the strip today (no fixture says its order is \
         meaningless). They are measured along the order, G1 question 2's lean: `unit` lands on \
         exactly k/(N−1), and a pixel 1D shader keeps `k + 0.5` of `(N, 1)`.\n\
         - **Option (a), the lean.** A fixture fed by a `unit` source gets the lamps' bounds as \
         its box, an axis thinner than one pitch padded to one pitch, and cover. A fixture fed \
         only by pixel sources keeps the old canvas and gets fill. A mixed fixture: `unit` wins \
         (lamp bounds, cover), and its pixel sources are measured under cover.\n\
         - **Option (b), the old canvas everywhere.** The box is the old canvas: the map2d's \
         `view_box`, or the lamps' bounds where the view box has no size, which is what today's \
         fit frames. The fit is cover when a `unit` source feeds the fixture, else fill. A \
         fixture has one fit, so a mixed fixture's pixel sources are measured under cover here \
         too.\n\
         - **Option (c), the render's footprint for pixels.** This one is not in the plan. The \
         report found it while measuring (a). It is (a) for `unit`-fed fixtures. A fixture fed \
         only by pixel sources gets, as its box, the region of the drawing that today's \
         `render_size` rectangle covers: today's fit contains the old canvas in the render, \
         centred, so the render reaches past the canvas on one axis. The fit is fill. Fill on \
         that region is today's `pos / outputSize` for every render size, square or not.\n"
    );

    let _ = writeln!(s, "## Summary\n");
    let _ = writeln!(s, "| | count |\n|---|---|");
    let _ = writeln!(s, "| projects | {} |", report.projects);
    let _ = writeln!(s, "| fixtures | {} |", report.fixtures.len());
    let _ = writeln!(
        s,
        "| fixtures a shader samples per lamp | {} ({} `unit` only, {} pixels only, {} both) |",
        measured.len(),
        count("unit"),
        count("pixels"),
        count("mixed")
    );
    let _ = writeln!(
        s,
        "| not measured (no per-lamp shader) | {} |",
        report.fixtures.len() - measured.len()
    );
    let _ = writeln!(
        s,
        "| **unchanged, bit for bit, under (a)** | **{}** of {} |",
        unchanged(0),
        measured.len()
    );
    let _ = writeln!(
        s,
        "| **unchanged, bit for bit, under (b)** | **{}** of {} |",
        unchanged(1),
        measured.len()
    );
    let _ = writeln!(s, "| fixtures that move under (a) | {} |", moves(0));
    let _ = writeln!(s, "| fixtures that move under (b) | {} |", moves(1));
    let _ = writeln!(
        s,
        "| unchanged, bit for bit, under (c) | {} of {} |",
        unchanged(2),
        measured.len()
    );
    let _ = writeln!(s, "| fixtures that move under (c) | {} |", moves(2));
    for (option, label) in [(0usize, "(a)"), (1, "(b)"), (2, "(c)")] {
        let seen = measured.iter().filter(|f| f.worst(option) >= 0.01).count();
        let _ = writeln!(
            s,
            "| fixtures whose lamps move a hundredth of a pitch or more under {label} | {seen} |"
        );
    }
    for (option, label) in [(0usize, "(a)"), (1, "(b)"), (2, "(c)")] {
        if let Some(worst) = measured
            .iter()
            .max_by(|x, y| x.worst(option).total_cmp(&y.worst(option)))
        {
            let _ = writeln!(
                s,
                "| biggest move under {label} | {} `{}`: {} pitches |",
                worst.project,
                worst.file,
                fmt_pitches(worst.worst(option))
            );
        }
    }
    let worst_unit_steps = |option: usize| {
        measured
            .iter()
            .flat_map(|f| f.sources.iter())
            .filter(|src| src.coords == V12Coords::Pattern)
            .map(|src| src.moves[option].q16_steps)
            .max()
            .unwrap_or(0)
    };
    let _ = writeln!(
        s,
        "| largest `unit` move under (a), in Q16.16 steps | {} |",
        worst_unit_steps(0)
    );
    let _ = writeln!(
        s,
        "| fixtures fed by both a `unit` and a pixel source | {} |",
        count("mixed")
    );
    let _ = writeln!(s);

    let _ = writeln!(s, "## What stands out\n");
    let squeezed: Vec<&&FixtureRow> = measured
        .iter()
        .filter(|f| {
            f.today_span.is_some_and(|[x, y]| x.max(y) < 0.25) && aspect_mismatches(&[f]) == 1
        })
        .collect();
    let _ = writeln!(
        s,
        "- **`unit` sources keep their look under (a).** The largest move is {} Q16.16 steps \
         ({} pitches). It is not zero: today's engine quantizes the lamps to render pixels \
         before it frames them, and T frames the drawing's own numbers. Under (b), the same \
         sources move up to {} pitches.",
        worst_unit_steps(0),
        fmt_pitches(
            measured
                .iter()
                .filter(|f| f.unit_fed())
                .map(|f| f.worst(0))
                .fold(0.0, f64::max)
        ),
        fmt_pitches(
            measured
                .iter()
                .filter(|f| f.unit_fed())
                .map(|f| f.worst(1))
                .fold(0.0, f64::max)
        )
    );
    let _ = writeln!(
        s,
        "- **Pixel sources under (a) move wherever `render_size` does not have the box's aspect** \
         ({} fixtures; the list is under Flags). Fill on the old canvas is today's framing only \
         when the two aspects match. Today's fit contains the canvas in the render, so for a \
         square render the exact fit is cover, not fill.",
        aspect_mismatches(&measured)
    );
    let _ = writeln!(
        s,
        "- **Option (c) keeps every pixel source's `pos / outputSize`.** {} fixtures move a \
         hundredth of a pitch or more under it, at most {} pitches. Those are the squeezed ones \
         below: their lamps sit hundredths of a render pixel apart today, so quantization alone \
         is a visible fraction of a pitch. It keeps the look by making the box the region \
         today's render covers, which can be far bigger than the drawing. Pulse's box would be \
         {}.",
        measured.iter().filter(|f| f.worst(2) >= 0.01).count(),
        fmt_pitches(
            measured
                .iter()
                .filter(|f| !f.unit_fed())
                .map(|f| f.worst(2))
                .fold(0.0, f64::max)
        ),
        measured
            .iter()
            .find(|f| f.project == "catalog/patterns/pulse")
            .map_or("(not found)".to_string(), |f| fmt_box(footprint(
                old_canvas(f),
                f.render_size
            )))
    );
    let _ = writeln!(
        s,
        "- **{} fixtures look squeezed today.** Their lamps span less than a quarter of \
         today's render on both axes, because a square canvas is contained in a long, thin \
         `render_size`. (a) and (b) undo the squeeze, and that undoing is most of their \
         biggest moves; (c) keeps it. They are: {}.",
        squeezed.len(),
        squeezed
            .iter()
            .map(|f| {
                let [x, y] = f.today_span.unwrap_or_default();
                format!(
                    "{} `{}` ({:.0}% × {:.0}% of {}×{})",
                    short(&f.project),
                    f.file,
                    x * 100.0,
                    y * 100.0,
                    f.render_size[0],
                    f.render_size[1]
                )
            })
            .collect::<Vec<_>>()
            .join("; ")
    );
    let _ = writeln!(
        s,
        "- **{} pixel shaders use `outputSize` as more than `pos / outputSize`** (listed under \
         Flags). The PLAYFUL choker's own shader is one: `pos / outputSize.y` scales both axes \
         by the height. Under `shadertoy` that is 64 instead of 28, and its x range goes from \
         0…3.7 to 0…1, whatever the box and fit. No answer to O1 keeps those shaders' look \
         unless the step also rewrites those lines.",
        report.output_size_uses.len()
    );
    let one_d: Vec<String> = measured
        .iter()
        .filter(|f| f.sources.iter().any(|src| !src.two_d))
        .map(|f| format!("{} `{}`", short(&f.project), f.file))
        .collect();
    let one_d_unit = measured
        .iter()
        .flat_map(|f| f.sources.iter())
        .filter(|src| !src.two_d && src.coords == V12Coords::Pattern)
        .count();
    let _ = writeln!(
        s,
        "- **{} fixtures are fed by a 1D source**, all along the strip today: {}. {} of those \
         sources are `unit`; the rest are pixel shaders, which keep `k + 0.5` of `(N, 1)`. Along \
         the order (G1 question 2's lean) none of them moves. If the default were across the \
         drawing instead, every one would read the drawing in place of the wire order.",
        one_d.len(),
        one_d.join(", "),
        one_d_unit
    );
    let _ = writeln!(
        s,
        "- **No fixture is fed by both a `unit` and a pixel source** in the repo, so \"unit wins\" \
         never has to choose.\n"
    );

    let _ = writeln!(s, "## The named fixtures\n");
    for (project, name) in NAMED {
        for f in report.fixtures.iter().filter(|f| f.project == *project) {
            let _ = writeln!(
                s,
                "- **{name}** (`{}` `{}`): {}",
                f.project,
                f.file,
                notice(f, report)
            );
        }
    }
    let _ = writeln!(s);

    for (option, title) in [
        (
            0usize,
            "## Option (a): the lean (`unit` → the lamps' bounds, cover; pixels only → the old canvas, fill)",
        ),
        (
            1,
            "## Option (b): the old canvas everywhere (cover with `unit`, else fill)",
        ),
        (
            2,
            "## Option (c): as (a), but pixels only → the render's footprint, fill",
        ),
    ] {
        let _ = writeln!(s, "{title}\n");
        let _ = writeln!(
            s,
            "Every measured fixture, the largest move over its sources and lamps. \
             \"0\" is bit for bit.\n"
        );
        let _ = writeln!(
            s,
            "| project | fixture | fed by | box | fit | largest move | in pitches | dark | extent today → new |\n|---|---|---|---|---|---|---|---|---|"
        );
        for f in &measured {
            let (units, pitches, dark, extent, fit, refused) = f.sources.iter().fold(
                (0.0f64, 0.0f64, 0usize, None, "", None),
                |(u, p, d, e, fit, r), src| {
                    let m = &src.moves[option];
                    (
                        u.max(m.units),
                        p.max(m.pitches),
                        d.max(m.dark),
                        e.or(m.extent),
                        if fit.is_empty() { m.fit } else { fit },
                        r.or(m.refused.clone()),
                    )
                },
            );
            let box_ = match (option, f.unit_fed()) {
                (0 | 2, true) => f.lean_box.map_or("refused".to_string(), fmt_box),
                (2, false) => fmt_box(footprint(old_canvas(f), f.render_size)),
                _ => fmt_box(old_canvas(f)),
            };
            let unit = if f.unit_fed() { "unit" } else { "px" };
            let _ = writeln!(
                s,
                "| {} | `{}` | {} | {} | {} | {} | {} | {} | {} |",
                short(&f.project),
                f.file,
                f.kind(),
                box_,
                fit,
                refused.unwrap_or_else(|| fmt_units(units, unit)),
                fmt_pitches(pitches),
                dark,
                extent.map_or("".to_string(), |(a, b)| format!(
                    "({:.4}, {:.4}) → ({:.4}, {:.4})",
                    a[0], a[1], b[0], b[1]
                ))
            );
        }
        let _ = writeln!(s);
    }

    let _ = writeln!(s, "## Flags\n");
    let _ = writeln!(s, "### `unit` sources that are not bit for bit under (a)\n");
    let not_exact: Vec<String> = measured
        .iter()
        .flat_map(|f| {
            f.sources
                .iter()
                .filter(|src| src.coords == V12Coords::Pattern && src.moves[0].q16_steps > 0)
                .map(move |src| {
                    format!(
                        "- {} `{}` × `{}`: {} Q16.16 step(s), {} pitches{}",
                        f.project,
                        f.file,
                        src.shader,
                        src.moves[0].q16_steps,
                        fmt_pitches(src.moves[0].pitches),
                        if src.two_d {
                            ""
                        } else {
                            " (1D, along the order)"
                        }
                    )
                })
        })
        .collect();
    if not_exact.is_empty() {
        let _ = writeln!(s, "None.\n");
    } else {
        let _ = writeln!(
            s,
            "{} source(s). Why: today's frame is not taken from the drawing. Today's engine \
             first fits the lamps into `render_size` in `f32`, truncates them to Q16.16 render \
             pixels, and only then frames their box. T frames the drawing's own numbers, \
             exactly. The two agree to within these steps (a step is 1/65536 of the unit view's \
             half-width). In 1D, today's step is an `f32` 1/(N−1); T's is exact.\n",
            not_exact.len()
        );
        for line in &not_exact {
            let _ = writeln!(s, "{line}");
        }
        let _ = writeln!(s);
    }

    let _ = writeln!(
        s,
        "### Pixel sources whose `render_size` aspect is not the box's\n"
    );
    let mut aspect = Vec::new();
    for f in &measured {
        if f.sources
            .iter()
            .any(|src| src.coords == V12Coords::Pixels && src.two_d)
        {
            let [_, _, w, h] = old_canvas(f);
            let box_aspect = f64::from(w) / f64::from(h);
            let render_aspect = f64::from(f.render_size[0]) / f64::from(f.render_size[1]);
            if (box_aspect - render_aspect).abs() > 1e-6 * render_aspect {
                aspect.push(format!(
                    "- {} `{}`: the box is {:.4}:1 and `render_size` {}×{} is {:.4}:1, so fill is \
                     not today's framing. The largest move is {} pitches under (a).",
                    f.project,
                    f.file,
                    box_aspect,
                    f.render_size[0],
                    f.render_size[1],
                    render_aspect,
                    fmt_pitches(f.worst(0))
                ));
            }
        }
    }
    if aspect.is_empty() {
        let _ = writeln!(s, "None.\n");
    } else {
        for line in &aspect {
            let _ = writeln!(s, "{line}");
        }
        let _ = writeln!(s);
    }

    let _ = writeln!(
        s,
        "### Pixel shaders that use `outputSize` other than as `pos / outputSize`\n\n\
         Under `shadertoy`, `outputSize` is ({SHADERTOY_N}, {SHADERTOY_N}) whatever the old \
         `render_size` was, so these lines read differently. That is in addition to the moves \
         above, which only measure `pos / outputSize`.\n"
    );
    if report.output_size_uses.is_empty() {
        let _ = writeln!(s, "None.\n");
    } else {
        for ((project, file), lines) in &report.output_size_uses {
            for (number, line) in lines {
                let _ = writeln!(s, "- {project} `{file}:{number}`: `{line}`");
            }
        }
        let _ = writeln!(s);
    }

    let _ = writeln!(s, "### Mixed fixtures (`unit` and pixel sources)\n");
    let mixed: Vec<&&FixtureRow> = measured.iter().filter(|f| f.kind() == "mixed").collect();
    if mixed.is_empty() {
        let _ = writeln!(s, "None.\n");
    } else {
        for f in mixed {
            for src in f
                .sources
                .iter()
                .filter(|src| src.coords == V12Coords::Pixels)
            {
                let _ = writeln!(
                    s,
                    "- {} `{}`: the pixel source `{}` moves {} pitches under (a) and {} under (b).",
                    f.project,
                    f.file,
                    src.shader,
                    fmt_pitches(src.moves[0].pitches),
                    fmt_pitches(src.moves[1].pitches)
                );
            }
        }
        let _ = writeln!(s);
    }

    let _ = writeln!(s, "### Boxes the step would have to refuse\n");
    let refused: Vec<String> = report
        .fixtures
        .iter()
        .filter(|f| f.lamps > 0 && f.lean_box.is_none())
        .map(|f| {
            format!(
                "- {} `{}`: its lamps are on one spot (no pitch to pad by). {}",
                f.project,
                f.file,
                f.note.as_deref().unwrap_or("")
            )
        })
        .collect();
    if refused.is_empty() {
        let _ = writeln!(
            s,
            "None. Every fixture's lamps have a pitch, so a thin axis can always be padded.\n"
        );
    } else {
        for line in &refused {
            let _ = writeln!(s, "{line}");
        }
        let _ = writeln!(s);
    }

    let _ = writeln!(
        s,
        "### Engineered boxes from the format-12 upgrade (`[0, 0, 1, h]`)\n"
    );
    let engineered: Vec<&FixtureRow> = report
        .fixtures
        .iter()
        .filter(|f| f.view_box[0] == 0.0 && f.view_box[1] == 0.0 && f.view_box[2] == 1.0)
        .collect();
    if engineered.is_empty() {
        let _ = writeln!(s, "None.\n");
    } else {
        let _ = writeln!(
            s,
            "These boxes were chosen so today's fit lands every hand-placed lamp exactly. Their \
             rulers would read 0…1. Under (a), a `unit`-fed one gets the lamps' bounds instead.\n"
        );
        for f in engineered {
            let _ = writeln!(
                s,
                "- {} `{}`: {} ({}; under (a) {})",
                f.project,
                f.file,
                fmt_box(f.view_box),
                f.kind(),
                if f.unit_fed() {
                    "the lamps' bounds"
                } else {
                    "kept, with fill"
                }
            );
        }
        let _ = writeln!(s);
    }

    let _ = writeln!(s, "### Lamps outside the old canvas\n");
    let outside: Vec<String> = measured
        .iter()
        .filter_map(|f| {
            let dark = |option: usize| {
                f.sources
                    .iter()
                    .map(|src| src.moves[option].dark)
                    .max()
                    .unwrap_or(0)
            };
            (dark(1) > 0).then(|| {
                format!(
                    "- {} `{}`: {} of {} lamps are outside the old canvas. Today the engine \
                     clamps them to the render's edge. Dark under (a): {}, under (b): {}, \
                     under (c): {}.",
                    f.project,
                    f.file,
                    dark(1),
                    f.lamps,
                    dark(0),
                    dark(1),
                    dark(2)
                )
            })
        })
        .collect();
    if outside.is_empty() {
        let _ = writeln!(s, "None.\n");
    } else {
        for line in &outside {
            let _ = writeln!(s, "{line}");
        }
        let _ = writeln!(s);
    }

    let _ = writeln!(s, "## Not measured\n");
    for f in report.fixtures.iter().filter(|f| f.sources.is_empty()) {
        let _ = writeln!(
            s,
            "- {} `{}`: {}",
            f.project,
            f.file,
            f.note.as_deref().unwrap_or("no shader")
        );
    }
    if !report.unsampled_shaders.is_empty() {
        let _ = writeln!(s, "\nShaders no fixture samples per lamp:\n");
        for (project, file) in &report.unsampled_shaders {
            let _ = writeln!(s, "- {project} `{file}`");
        }
    }
    s
}

fn aspect_mismatches(measured: &[&FixtureRow]) -> usize {
    measured
        .iter()
        .filter(|f| {
            f.sources
                .iter()
                .any(|src| src.coords == V12Coords::Pixels && src.two_d)
                && {
                    let [_, _, w, h] = old_canvas(f);
                    let box_aspect = f64::from(w) / f64::from(h);
                    let render_aspect = f64::from(f.render_size[0]) / f64::from(f.render_size[1]);
                    (box_aspect - render_aspect).abs() > 1e-6 * render_aspect
                }
        })
        .count()
}

/// One plain line on what a person would notice on this fixture.
fn notice(f: &FixtureRow, report: &Report) -> String {
    if f.sources.is_empty() {
        return f.note.clone().unwrap_or_default();
    }
    let words = |option: usize| {
        let worst = f.worst(option);
        if worst == 0.0 {
            "nothing moves".to_string()
        } else if worst < 0.01 {
            format!(
                "nothing you could see ({} pitches at most)",
                fmt_pitches(worst)
            )
        } else {
            format!("lamps move up to {} pitches", fmt_pitches(worst))
        }
    };
    let extent = f
        .sources
        .iter()
        .find_map(|src| src.moves[1].extent.map(|(today, new)| (today, new)));
    let mut line = format!(
        "{} lamps, fed by {} ({}). Under (a): {}. Under (b): {}. Under (c): {}.",
        f.lamps,
        f.sources
            .iter()
            .map(|src| format!("`{}`", src.shader))
            .collect::<Vec<_>>()
            .join(", "),
        f.kind(),
        words(0),
        words(1),
        words(2)
    );
    for src in &f.sources {
        if let Some(lines) = report
            .output_size_uses
            .get(&(f.project.clone(), src.glsl.clone()))
        {
            for (number, text) in lines {
                let _ = write!(
                    line,
                    " Its shader also reads `{text}` (`{}:{number}`), which changes with \
                     `outputSize` = ({SHADERTOY_N}, {SHADERTOY_N}) under every option.",
                    src.glsl
                );
            }
        }
    }
    if let Some((today, new)) = extent
        && today != new
    {
        let _ = write!(
            line,
            " The pattern's y extent goes from ±{:.4} to ±{:.4} under (b).",
            today[1], new[1]
        );
    }
    line
}

fn short(project: &str) -> &str {
    project
        .strip_prefix("catalog/")
        .or_else(|| project.strip_prefix("projects/"))
        .unwrap_or(project)
}

fn fmt_box(b: [f32; 4]) -> String {
    format!(
        "[{} {} {} {}]",
        trim(b[0]),
        trim(b[1]),
        trim(b[2]),
        trim(b[3])
    )
}

fn trim(v: f32) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

fn fmt_units(v: f64, unit: &str) -> String {
    if v == 0.0 {
        return "0".to_string();
    }
    match unit {
        "unit" => format!("{v:.6}"),
        _ => format!("{v:.3} px"),
    }
}

fn fmt_pitches(v: f64) -> String {
    if v == 0.0 {
        "0".to_string()
    } else if v < 0.001 {
        format!("{v:.1e}")
    } else {
        format!("{v:.3}")
    }
}
