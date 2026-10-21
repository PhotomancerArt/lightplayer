//! The `fluid-draw` catalog pattern, drawn on (drawing-on-the-picture plan
//! P3, D10).
//!
//! The pattern is loaded from `catalog/patterns/fluid-draw` with one change:
//! its fixture is swapped for a 17 × 9 grid of lamps in a 2:1 box, so every
//! lamp's place in pattern space is known (columns −1…1 left to right, rows
//! 0.5…−0.5 top to bottom). Touches are written the way Studio writes them:
//! momentary map panel writes on `touches`, ≤ 30 a second, ttl 400 ms, in
//! the scope the pattern reads them from.
//!
//! ```bash
//! cargo test -p lpc-engine --test fluid_draw_pattern -- --nocapture
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lpc_engine::engine::LoadedProjectRuntime;
use lpc_engine::node::{NodeEntryState, ScopeRef};
use lpc_engine::{EngineServices, ProjectLoader};
use lpc_model::{
    ChannelName, FluidEmitter, FromLpValue, PanelWriteValue, SlotDataAccess, SlotPath,
    TOUCHES_CHANNEL, ToLpValue, Touch, TreePath, lookup_slot_data,
};
use lpfs::{AsLpPath, LpFs, LpFsMemory};

const FRAME_MS: u32 = 33;
const TTL_MS: u32 = 400;
const LAMP_COLS: usize = 17;
const LAMP_ROWS: usize = 9;
/// Friends are ids 1..=3; stroke emitters 16 and up.
const STROKE_ID_BASE: u32 = 16;

#[test]
fn a_drag_leaves_dye_along_the_lamps_it_crossed() {
    let mut drawing = Drawing::boot();
    drawing.run(10);
    // Let the friends' dye fade before drawing, so what is lit is the stroke.
    drawing.touch(&[Touch::new(1, [-0.8, 0.0])]);
    drawing.run(1);
    drawing.release();
    drawing.run(60);

    // Left to right along the middle row, 30 samples a second, over 1 s.
    for step in 0..=30 {
        let x = -0.8 + 1.6 * step as f32 / 30.0;
        drawing.touch(&[Touch::new(1, [x, 0.0])]);
        drawing.run(1);
    }
    let lit = drawing.brightness();
    print_grid("after a left-to-right drag", &lit);
    let middle: Vec<u32> = (2..=14).map(|col| lit[lamp(col, 4)]).collect();
    assert!(
        middle.iter().all(|value| *value > 0),
        "every lamp the stroke crossed holds dye: {middle:?}"
    );
    // Where the finger passed early in the stroke (the left), its row holds
    // more dye than the rows above and below it.
    let left = |row: usize| (2..=6).map(|col| lit[lamp(col, row)]).sum::<u32>();
    assert!(
        left(4) > 2 * left(0) && left(4) > 2 * left(8),
        "the stroke's row is brighter than the rows away from it ({} vs {} and {})",
        left(4),
        left(0),
        left(8)
    );

    let strokes = drawing.stroke_emitters();
    assert!(!strokes.is_empty(), "a moving finger stamps emitters");
    assert!(
        strokes.iter().all(|e| e.dir[0] > 0.9),
        "the push follows the stroke's direction: {:?}",
        strokes.iter().map(|e| e.dir).collect::<Vec<_>>()
    );
    assert!(
        strokes.len() > 1,
        "a moving finger stamps along the chord, not one dab"
    );
}

#[test]
fn letting_go_ends_the_stroke_and_the_friends_come_back() {
    let mut drawing = Drawing::boot();
    drawing.run(10);
    assert_eq!(
        drawing.friend_ids(),
        [1, 2, 3],
        "friends stir an idle fluid"
    );

    for step in 0..20 {
        drawing.touch(&[Touch::new(1, [0.3 * (step as f32 / 20.0), 0.2])]);
        drawing.run(1);
    }
    assert!(
        drawing.friend_ids().is_empty(),
        "the friends fade out within a third of a second of a touch"
    );
    assert!(!drawing.stroke_emitters().is_empty());

    // No release: the writer despawns on its own after the ttl.
    drawing.run((TTL_MS / FRAME_MS) as usize + 3);
    assert!(
        drawing.stroke_emitters().is_empty(),
        "no stroke emitter outlives the ttl"
    );
    assert!(
        drawing.friend_ids().is_empty(),
        "the friends wait for the idle time"
    );

    drawing.run((2_000 / FRAME_MS) as usize + 20);
    assert_eq!(
        drawing.friend_ids(),
        [1, 2, 3],
        "the friends are back after two idle seconds"
    );
}

#[test]
fn every_node_of_the_pattern_runs() {
    let mut drawing = Drawing::boot();
    drawing.run(5);
    let broken: Vec<String> = drawing
        .rt
        .engine()
        .tree()
        .entries()
        // The root stays `Created`: it is the show, not a node that ticks.
        .filter(|entry| entry.path.to_string().matches('/').count() > 1)
        .filter(|entry| !matches!(entry.status.value(), lpc_model::NodeRuntimeStatus::Ok))
        .map(|entry| format!("{}: {:?}", entry.path, entry.status.value()))
        .collect();
    assert!(broken.is_empty(), "{broken:#?}");
}

#[test]
fn a_new_finger_dabs_once_and_two_fingers_keep_apart() {
    let mut drawing = Drawing::boot();
    drawing.run(5);
    drawing.touch(&[Touch::new(1, [-0.5, 0.0]), Touch::new(2, [0.5, 0.0])]);
    drawing.run(1);
    let first = drawing.stroke_emitters();
    assert_eq!(first.len(), 2, "one dab per new finger: {first:?}");
    let ids: Vec<u32> = first.iter().map(|e| e.id).collect();
    assert_ne!(ids[0], ids[1]);
    assert_ne!(
        first[0].color, first[1].color,
        "each finger takes its hue from where it started"
    );

    drawing.touch(&[Touch::new(1, [-0.4, 0.1]), Touch::new(2, [0.6, -0.1])]);
    drawing.run(1);
    assert_eq!(
        drawing.stroke_emitters().len(),
        8,
        "two moving fingers stamp four steps each"
    );
}

struct Drawing {
    fs: LpFsMemory,
    rt: LoadedProjectRuntime,
}

impl Drawing {
    fn boot() -> Self {
        let fs = pattern_on_a_grid();
        let services = EngineServices::new(TreePath::parse("/fluid_draw.show").expect("path"));
        let mut rt = ProjectLoader::load_from_root(&fs, services).expect("load fluid-draw");
        rt.engine_mut()
            .set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
                lp_shader::ShaderFrontend::LpsGlsl,
            ))));
        Self { fs, rt }
    }

    fn run(&mut self, frames: usize) {
        for frame in 0..frames {
            self.rt
                .tick_with_residency(&self.fs, FRAME_MS)
                .unwrap_or_else(|e| panic!("frame {frame}: {e}"));
        }
    }

    fn touch(&mut self, touches: &[Touch]) {
        let scope = self.touches_scope();
        let set = touches
            .iter()
            .map(|touch| (touch.id, touch.to_lp_value()))
            .collect();
        self.rt.engine_mut().panel_write_value(
            scope,
            ChannelName(String::from(TOUCHES_CHANNEL)),
            PanelWriteValue::Map(set),
            Some(TTL_MS),
        );
    }

    fn release(&mut self) {
        self.touch(&[]);
    }

    fn touches_scope(&self) -> ScopeRef {
        let draw = self
            .rt
            .engine()
            .tree()
            .entries()
            .find(|entry| entry.path.to_string().ends_with("draw.compute_shader"))
            .expect("draw in tree")
            .id;
        self.rt
            .engine()
            .tree()
            .bus_read_scope(draw)
            .expect("draw reads a scope")
    }

    /// Every emitter the pattern's two shaders produce (`draw`'s strokes,
    /// then `friends`), in slot order, empty slots left out.
    fn emitters(&self) -> Vec<FluidEmitter> {
        let mut all = self.emitters_of("draw.compute_shader");
        all.extend(self.emitters_of("friends.compute_shader"));
        all
    }

    fn emitters_of(&self, suffix: &str) -> Vec<FluidEmitter> {
        let engine = self.rt.engine();
        let entry = engine
            .tree()
            .entries()
            .find(|entry| entry.path.to_string().ends_with(suffix))
            .unwrap_or_else(|| panic!("{suffix} in tree"));
        let NodeEntryState::Alive(node) = entry.state.value() else {
            panic!("{suffix} alive");
        };
        let Some(state) = node.runtime_state_slots() else {
            return Vec::new();
        };
        let Ok(SlotDataAccess::Map(map)) = lookup_slot_data(
            state,
            engine.slot_shapes(),
            &SlotPath::parse("emitters").expect("path"),
        ) else {
            return Vec::new();
        };
        map.keys()
            .into_iter()
            .filter_map(|key| match map.get(&key)? {
                SlotDataAccess::Value(value) => FluidEmitter::from_lp_value(&value.value()).ok(),
                _ => None,
            })
            .filter(|emitter| emitter.id != 0)
            .collect()
    }

    fn friend_ids(&self) -> Vec<u32> {
        self.emitters()
            .iter()
            .map(|e| e.id)
            .filter(|id| *id < STROKE_ID_BASE)
            .collect()
    }

    fn stroke_emitters(&self) -> Vec<FluidEmitter> {
        self.emitters()
            .into_iter()
            .filter(|e| e.id >= STROKE_ID_BASE)
            .collect()
    }

    /// r + g + b per lamp, from the output's published bytes.
    fn brightness(&self) -> Vec<u32> {
        let engine = self.rt.engine();
        let mut bytes = Vec::new();
        for entry in engine.tree().entries() {
            let Some(id) = engine.runtime_output_sink_buffer_id(entry.id) else {
                continue;
            };
            if let Some(buffer) = engine.runtime_buffers().get(id) {
                bytes.extend_from_slice(&buffer.value().bytes());
            }
        }
        let lamps = LAMP_COLS * LAMP_ROWS;
        assert_eq!(bytes.len() % (lamps * 3), 0, "{} bytes", bytes.len());
        let width = bytes.len() / (lamps * 3);
        bytes
            .chunks_exact(width * 3)
            .map(|lamp| {
                lamp.chunks_exact(width)
                    .map(|channel| match width {
                        1 => u32::from(channel[0]),
                        2 => u32::from(u16::from_le_bytes([channel[0], channel[1]])),
                        other => panic!("{other} bytes per channel"),
                    })
                    .sum()
            })
            .collect()
    }
}

fn lamp(col: usize, row: usize) -> usize {
    row * LAMP_COLS + col
}

fn print_grid(what: &str, lit: &[u32]) {
    println!("{what}: r+g+b per lamp, top row first");
    for row in lit.chunks_exact(LAMP_COLS) {
        let cells: Vec<String> = row.iter().map(|v| format!("{v:6}")).collect();
        println!("  {}", cells.join(""));
    }
}

/// The catalog pattern, in memory, with its fixture swapped for the grid.
fn pattern_on_a_grid() -> LpFsMemory {
    let fs = LpFsMemory::new();
    let root = workspace_dir().join("catalog/patterns/fluid-draw");
    copy_tree(&fs, &root, &root);
    fs.write_file("/fixture.json".as_path(), grid_fixture().as_bytes())
        .expect("write fixture");
    fs
}

fn copy_tree(fs: &LpFsMemory, root: &Path, dir: &Path) {
    for entry in std::fs::read_dir(dir).expect("read pattern dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            copy_tree(fs, root, &path);
            continue;
        }
        let rel = path.strip_prefix(root).expect("under root");
        let bytes = std::fs::read(&path).expect("read pattern file");
        fs.write_file(format!("/{}", rel.display()).as_path(), &bytes)
            .expect("write pattern file");
    }
}

/// 17 × 9 lamps in a 2:1 box, letterboxed inside a square render.
fn grid_fixture() -> String {
    let mut points = Vec::new();
    for row in 0..LAMP_ROWS {
        for col in 0..LAMP_COLS {
            let x = 0.05 + 0.6 * col as f32 / (LAMP_COLS - 1) as f32;
            let y = 0.10 + 0.3 * row as f32 / (LAMP_ROWS - 1) as f32;
            points.push(format!("\"{}\": [{x}, {y}]", row * LAMP_COLS + col));
        }
    }
    format!(
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
    )
}

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace dir")
        .to_path_buf()
}
