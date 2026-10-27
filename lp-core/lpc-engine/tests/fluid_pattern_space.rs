//! The Fluid node in pattern space, end to end through the real engine,
//! fixture and output.
//!
//! A Fluid that declares `"coords": "pattern"` (the shader def's key and
//! values, ADR `docs/adr/2026-09-24-pattern-space.md`, 2026-10-20 amendment)
//! spans its solver grid over the lamps' box and reads its emitters in
//! pattern units, y up. A Fluid that does not — no key, or `"pixels"` —
//! publishes exactly what it published before the key existed.
//!
//! ```bash
//! cargo test -p lpc-engine --test fluid_pattern_space
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lpc_engine::engine::LoadedProjectRuntime;
use lpc_engine::{EngineServices, ProjectLoader};
use lpc_model::TreePath;
use lpfs::LpFsStd;

/// Ticks the pixels-mode identity run renders; each lands a solver step
/// (40 ms ≥ 1 / 25 Hz), so the dye is well developed by the end.
const IDENTITY_TICKS: usize = 16;
const IDENTITY_DELTA_MS: u32 = 40;

/// `projects/test/fluid`'s published output, per tick, digested — captured
/// from a code path WITHOUT the pattern-space opt-in: first 2026-10-20 at
/// `a118638e8`, then again 2026-10-27 on plain `main` at `777989e5e`, after
/// main's solver rework (#1282, #1285) changed the fluid's bits on purpose.
/// Pixels mode must keep producing exactly what main without the opt-in
/// produces; never re-capture this from the branch to make a change pass.
const PIXELS_DIRECT_GOLDEN: &[(usize, u64)] = &[
    (1446, 0xe156c6ddf22bc947),
    (1446, 0xa7cba13baa3c42d9),
    (1446, 0x2146a89551d3db95),
    (1446, 0x8a3c9444190ddc53),
    (1446, 0xbd029161069d6a26),
    (1446, 0xbd029161069d6a26),
    (1446, 0x31123d93f64bf377),
    (1446, 0x31123d93f64bf377),
    (1446, 0xd3093e23b3d73e27),
    (1446, 0xd3093e23b3d73e27),
    (1446, 0x212f152240e7282b),
    (1446, 0x212f152240e7282b),
    (1446, 0x9963913a9bb975ac),
    (1446, 0xfb6efed6f446caaf),
    (1446, 0x510f66a51c4b0c88),
    (1446, 0x510f66a51c4b0c88),
];

/// The same project with the fixture switched to `texture_area` sampling,
/// which reaches the Fluid's `render_texture_into` rather than its sample
/// stream. Captured with [`PIXELS_DIRECT_GOLDEN`].
const PIXELS_TEXTURE_AREA_GOLDEN: &[(usize, u64)] = &[
    (1446, 0xa5847bcafb22a1e0),
    (1446, 0xf871eb26b111bb0b),
    (1446, 0xf61a59563e025737),
    (1446, 0xe27be92c1510843c),
    (1446, 0x4b41e46da5bbdb01),
    (1446, 0x4b41e46da5bbdb01),
    (1446, 0x336c22de9a591c76),
    (1446, 0x336c22de9a591c76),
    (1446, 0x2229ecb698ab7709),
    (1446, 0x2229ecb698ab7709),
    (1446, 0x680e19206d45a09f),
    (1446, 0x680e19206d45a09f),
    (1446, 0x3a453acedab303e2),
    (1446, 0xd202a0dd04c7c16d),
    (1446, 0x3fd346cc9e1a7258),
    (1446, 0x3fd346cc9e1a7258),
];

// ---- tests -----------------------------------------------------------------

/// Pixels mode is byte-identical to the code before the opt-in, on both the
/// direct sample stream and the texture path, and an explicit `"pixels"` is
/// the same as no key.
#[test]
fn pixels_mode_publishes_what_it_did_before_the_opt_in() {
    let absent = Scratch::from(&fluid_dir(), "pixels-absent");
    let direct = render_digests(absent.path());
    assert_eq!(direct, PIXELS_DIRECT_GOLDEN, "direct sampling moved");

    let explicit = Scratch::from(&fluid_dir(), "pixels-explicit");
    explicit.edit("fluid.json", |text| {
        text.replacen(
            "\"kind\": \"Fluid\",",
            "\"kind\": \"Fluid\",\n  \"coords\": \"pixels\",",
            1,
        )
    });
    assert_eq!(
        render_digests(explicit.path()),
        direct,
        "absent == \"pixels\""
    );

    let area = Scratch::from(&fluid_dir(), "pixels-texture-area");
    area.edit("fixture.json", |text| {
        text.replace("\"sampling\": \"direct\"", "\"sampling\": \"texture_area\"")
    });
    assert_eq!(
        render_digests(area.path()),
        PIXELS_TEXTURE_AREA_GOLDEN,
        "texture-area sampling moved"
    );
}

/// Pattern mode on a non-square, letterboxed fixture: a 17 × 9 grid of
/// lamps, 0.125 pattern units apart, in a 2:1 box that sits up and left of
/// the 32 × 32 render's centre. Dye from an emitter at pattern (0, 0) is
/// centred on the lamps — the middle lamp is lit at full strength, the dye's
/// centroid is the box's centre, mirror lamps match — not on the texture,
/// whose centre is outside the box altogether.
#[test]
fn a_centre_emitter_is_centred_on_the_lamps_not_the_texture() {
    let lamps = letterboxed_lamps();
    let red = render_pattern_red(&lamps, [0.0, 0.0], 0.3, "centre");
    print_grid("centre emitter", &red);
    let max = *red.iter().max().unwrap();
    assert!(max > 0, "dye reached the lamps");
    let middle = lamp(8, 4);
    assert_eq!(red[middle], max, "the middle lamp is fully lit");
    let [cx, cy] = centroid(&red);
    assert!((cx - 8.0).abs() < 0.25, "centroid column {cx}");
    assert!((cy - 4.0).abs() < 0.25, "centroid row {cy}");
    // Round on the lamps: as far right as left, as far up as down, and the
    // same lamp distance across as up and down (0.25 = two lamps each way),
    // to within the Q16.16 placement's rounding (a lamp on a cell centre
    // reads a few parts in 10⁴ of its neighbour).
    for (a, b) in [
        (lamp(6, 4), lamp(10, 4)),
        (lamp(8, 2), lamp(8, 6)),
        (lamp(6, 4), lamp(8, 2)),
    ] {
        assert!(red[a] > max / 2, "lamp {a} is two lamps from the middle");
        assert!(red[a].abs_diff(red[b]) <= max / 1000, "lamps {a} and {b}");
    }
    // …and no further: three lamps out (0.375) is outside the 0.3 radius.
    for outside in [lamp(5, 4), lamp(11, 4), lamp(8, 1), lamp(8, 7)] {
        assert!(
            red[outside] <= max / 1000,
            "lamp {outside} is outside the stamp"
        );
    }
    // The texture's centre is past the box's bottom-right corner: dark.
    assert_eq!(red[lamp(16, 8)], 0, "the corner nearest the texture centre");
}

/// Emitters are read in pattern units, y up: `(0.5, 0.25)` on a box of
/// extent `(1, 0.5)` is three quarters of the way across and a quarter of
/// the way down the lamps — column 12, row 2 of the 17 × 9 grid.
#[test]
fn an_emitter_lands_on_the_lamp_at_its_pattern_position() {
    let lamps = letterboxed_lamps();
    let red = render_pattern_red(&lamps, [0.5, 0.25], 0.2, "off-centre");
    print_grid("emitter at (0.5, 0.25)", &red);
    let max = *red.iter().max().unwrap();
    assert!(max > 0, "dye reached the lamps");
    assert_eq!(red[lamp(12, 2)], max, "column 12, row 2 is fully lit");
    let [cx, cy] = centroid(&red);
    assert!((cx - 12.0).abs() < 0.25, "centroid column {cx}");
    assert!((cy - 2.0).abs() < 0.25, "centroid row {cy}");
}

/// Capture helper for the two goldens above. Ignored — run it explicitly
/// (`--ignored --nocapture`) only to capture from a known-good code path.
#[test]
#[ignore = "capture helper, not an assertion"]
fn capture_pixels_goldens() {
    let direct = Scratch::from(&fluid_dir(), "capture-direct");
    println!("direct: {:#x?}", render_digests(direct.path()));
    let area = Scratch::from(&fluid_dir(), "capture-area");
    area.edit("fixture.json", |text| {
        text.replace("\"sampling\": \"direct\"", "\"sampling\": \"texture_area\"")
    });
    println!("texture_area: {:#x?}", render_digests(area.path()));
}

// ---- the letterboxed pattern-mode project ----------------------------------

const LAMP_COLS: usize = 17;
const LAMP_ROWS: usize = 9;

/// Index of the lamp at `col`, `row` (row 0 at the top).
fn lamp(col: usize, row: usize) -> usize {
    row * LAMP_COLS + col
}

fn print_grid(what: &str, red: &[u32]) {
    println!("{what}: red per lamp, top row first");
    for row in red.chunks_exact(LAMP_COLS) {
        let cells: Vec<String> = row.iter().map(|v| format!("{v:5}")).collect();
        println!("  {}", cells.join(" "));
    }
}

/// Lamp centres, normalized over the 32 × 32 render, row-major from the top
/// left: a 2:1 box (0.6 × 0.3 of a square render) centred at (0.35, 0.25),
/// so the render's centre (0.5, 0.5) is outside it.
fn letterboxed_lamps() -> Vec<[f32; 2]> {
    let mut lamps = Vec::new();
    for row in 0..LAMP_ROWS {
        for col in 0..LAMP_COLS {
            lamps.push([
                0.05 + 0.6 * col as f32 / (LAMP_COLS - 1) as f32,
                0.10 + 0.3 * row as f32 / (LAMP_ROWS - 1) as f32,
            ]);
        }
    }
    lamps
}

/// Render a pattern-mode fluid with one still red emitter at `pos` of
/// `radius` (pattern units) on `lamps`, and return each lamp's published red
/// channel. The dye has no diffusion and no fade, so it is the stamp itself.
fn render_pattern_red(lamps: &[[f32; 2]], pos: [f32; 2], radius: f32, name: &str) -> Vec<u32> {
    let project = PatternProject::new(lamps, pos, radius, name);
    let mut rt = load(project.path());
    for tick in 0..6 {
        rt.tick(IDENTITY_DELTA_MS)
            .unwrap_or_else(|e| panic!("tick {tick}: {e:?}"));
    }
    let bytes = published_bytes(&rt);
    assert_eq!(bytes.len() % (lamps.len() * 3), 0, "{} bytes", bytes.len());
    let width = bytes.len() / (lamps.len() * 3);
    bytes
        .chunks_exact(width * 3)
        .map(|lamp| match width {
            1 => u32::from(lamp[0]),
            2 => u32::from(u16::from_le_bytes([lamp[0], lamp[1]])),
            other => panic!("{other} bytes per channel"),
        })
        .collect()
}

/// The dye-weighted mean `[column, row]` of the lamp grid.
fn centroid(red: &[u32]) -> [f64; 2] {
    let (mut sum, mut x, mut y) = (0.0, 0.0, 0.0);
    for (index, value) in red.iter().enumerate() {
        let weight = f64::from(*value);
        sum += weight;
        x += weight * (index % LAMP_COLS) as f64;
        y += weight * (index / LAMP_COLS) as f64;
    }
    assert!(sum > 0.0, "no dye reached the lamps");
    [x / sum, y / sum]
}

/// A clock, a pattern-mode fluid, a direct-sampling fixture on the given
/// lamps (a 32 × 32 render) and an output with every colour stage off.
///
/// The fluid's 20 × 10 grid has square cells on the 2:1 lamp box (0.1
/// pattern units each way), so the lamp lattice and the cell lattice are
/// both symmetric about the box centre and a mirror pair reads mirror cells.
struct PatternProject {
    dir: PathBuf,
}

impl PatternProject {
    fn new(lamps: &[[f32; 2]], pos: [f32; 2], radius: f32, name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "lp-fluid-pattern-{}-project-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp project dir");
        let write = |file: &str, text: &str| {
            std::fs::write(dir.join(file), text).expect("write project file");
        };
        write(
            "project.json",
            "{\n  \"format\": 11,\n  \"name\": \"fluid-pattern\"\n}\n",
        );
        write(
            "module.json",
            r#"{
  "kind": "Module",
  "nodes": {
    "clock": { "ref": "./clock.json" },
    "fluid": { "ref": "./fluid.json" },
    "fixture": { "ref": "./fixture.json" },
    "output": { "ref": "./output.json" }
  }
}
"#,
        );
        write("clock.json", "{ \"kind\": \"Clock\" }\n");
        write(
            "fluid.json",
            &format!(
                r#"{{
  "kind": "Fluid",
  "coords": "pattern",
  "size": {{ "width": 20, "height": 10 }},
  "solver_iterations": 3,
  "step_hz": 25,
  "fade_speed": 0.0,
  "viscosity": 0.00003,
  "emitters": {{
    "1": {{
      "id": 1,
      "pos": [{}, {}],
      "dir": [1.0, 0.0],
      "radius": {radius},
      "color": [1.0, 0.0, 0.0],
      "velocity": 0.0,
      "intensity": 0.05
    }}
  }}
}}
"#,
                pos[0], pos[1]
            ),
        );
        let points: Vec<String> = lamps
            .iter()
            .enumerate()
            .map(|(k, [x, y])| format!("\"{k}\": [{x}, {y}]"))
            .collect();
        write(
            "fixture.json",
            &format!(
                r#"{{
  "kind": "Fixture",
  "render_size": {{ "width": 32, "height": 32 }},
  "bindings": {{
    "input": {{ "source": "bus:visual.out" }},
    "output": {{ "target": "bus:control.out" }}
  }},
  "sampling": "direct",
  "mapping": {{
    "kind": "PathPoints",
    "paths": {{ "0": {{ "kind": "PointList", "first_channel": 0, "points": {{ {} }} }} }},
    "sample_diameter": 1.0
  }},
  "color_order": "rgb",
  "brightness": 1.0,
  "gamma_correction": false
}}
"#,
                points.join(", ")
            ),
        );
        write(
            "output.json",
            r#"{
  "kind": "Output",
  "ports": { "0": { "endpoint": "ws281x:local:D10" } },
  "bindings": { "input": { "source": "bus:control.out" } },
  "options": {
    "white_point": [1, 1, 1],
    "interpolation_enabled": false,
    "dithering_enabled": false,
    "lut_enabled": false
  }
}
"#,
        );
        Self { dir }
    }

    fn path(&self) -> &Path {
        &self.dir
    }
}

impl Drop for PatternProject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

// ---- running a project -----------------------------------------------------

fn load(dir: &Path) -> LoadedProjectRuntime {
    let fs = LpFsStd::new(dir.to_path_buf());
    let services = EngineServices::new(TreePath::parse("/fluid.show").expect("root path"));
    let mut rt = ProjectLoader::load_from_root(&fs, services)
        .unwrap_or_else(|e| panic!("load {}: {e:?}", dir.display()));
    rt.engine_mut()
        .set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
    rt
}

/// Tick the project and digest every output's published bytes per tick.
fn render_digests(dir: &Path) -> Vec<(usize, u64)> {
    let mut rt = load(dir);
    let mut out = Vec::new();
    for tick in 0..IDENTITY_TICKS {
        rt.tick(IDENTITY_DELTA_MS)
            .unwrap_or_else(|e| panic!("tick {tick}: {e:?}"));
        let bytes = published_bytes(&rt);
        out.push((bytes.len(), fnv1a64(&bytes)));
    }
    out
}

/// Every output node's published bytes, concatenated in tree order.
fn published_bytes(rt: &LoadedProjectRuntime) -> Vec<u8> {
    let engine = rt.engine();
    let mut out = Vec::new();
    for entry in engine.tree().entries() {
        let Some(buffer_id) = engine.runtime_output_sink_buffer_id(entry.id) else {
            continue;
        };
        let Some(buffer) = engine.runtime_buffers().get(buffer_id) else {
            continue;
        };
        out.extend_from_slice(&buffer.value().bytes());
    }
    out
}

/// FNV-1a, 64-bit.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

// ---- scratch projects ------------------------------------------------------

struct Scratch {
    dir: PathBuf,
}

impl Scratch {
    /// A private copy of the project at `src` (top-level files only).
    fn from(src: &Path, name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("lp-fluid-pattern-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp project dir");
        for entry in std::fs::read_dir(src).expect("read project") {
            let entry = entry.expect("dir entry");
            if entry.file_type().unwrap().is_file() {
                std::fs::copy(entry.path(), dir.join(entry.file_name())).expect("copy");
            }
        }
        Self { dir }
    }

    fn path(&self) -> &Path {
        &self.dir
    }

    fn edit(&self, file: &str, f: impl FnOnce(&str) -> String) {
        let path = self.dir.join(file);
        let before = std::fs::read_to_string(&path).expect("read");
        let after = f(&before);
        assert_ne!(before, after, "{file}: edit changed nothing");
        std::fs::write(path, after).expect("write");
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn fluid_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace dir")
        .join("projects/test/fluid")
}
