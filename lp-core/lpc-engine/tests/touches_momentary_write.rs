//! A touch set rides a momentary panel write into a compute shader
//! (drawing-on-the-picture plan P1, D6–D8).
//!
//! The project is a pattern shaped like the catalog's: a root module with a
//! clock, an `effect` module, a fixture and an output. Inside `effect`, a
//! compute shader reads `touches` as a sentinel map of `lp::touch::Touch`
//! and copies it into its produced `marks` map; a render shader reads the
//! marks, which is what makes the fixture demand the compute every frame.
//!
//! A touch set is written the way Studio writes one: a `u32`-keyed map on
//! `touches`, momentary (`ttl_ms`), renewed by writing again.
//!
//! ```bash
//! cargo test -p lpc-engine --test touches_momentary_write
//! ```

use std::sync::Arc;

use lpc_engine::engine::LoadedProjectRuntime;
use lpc_engine::node::{NodeEntryState, ScopeRef};
use lpc_engine::{EngineServices, ProjectLoader};
use lpc_model::{
    ChannelName, FromLpValue, NodeId, PanelWriteValue, SlotDataAccess, SlotPath, TOUCHES_CHANNEL,
    ToLpValue, Touch, TreePath, lookup_slot_data,
};
use lpfs::{AsLpPath, LpFs, LpFsMemory};

const TTL_MS: u32 = 400;
const FRAME_MS: u32 = 16;

#[test]
fn a_momentary_touch_set_reaches_the_compute_shader_and_lets_go() {
    let mut drawing = Drawing::boot();
    drawing.run(5);
    assert!(drawing.marks().is_empty(), "nothing touches at boot");

    let scope = drawing.effect_scope();
    drawing.touch(scope, &[Touch::new(1, [0.25, -0.5])]);
    drawing.run(2);
    assert_eq!(drawing.marks(), vec![Touch::new(1, [0.25, -0.5])]);

    // Two fingers, the first one moved: the set is replaced whole.
    drawing.touch(
        scope,
        &[Touch::new(1, [0.5, -0.25]), Touch::new(2, [-1.0, 0.75])],
    );
    drawing.run(2);
    assert_eq!(
        drawing.marks(),
        vec![Touch::new(1, [0.5, -0.25]), Touch::new(2, [-1.0, 0.75])]
    );

    // Renewal keeps a resting finger past the first deadline.
    for _ in 0..4 {
        drawing.run(9);
        drawing.touch(scope, &[Touch::new(1, [0.5, -0.25])]);
    }
    drawing.run(2);
    assert_eq!(drawing.marks(), vec![Touch::new(1, [0.5, -0.25])]);

    // Silence past the ttl despawns the writer: the set is empty again.
    drawing.run((TTL_MS / FRAME_MS) as usize + 2);
    assert!(
        drawing
            .rt
            .engine()
            .panel_writers()
            .get(scope, &ChannelName(String::from(TOUCHES_CHANNEL)))
            .is_none(),
        "an expired touch set despawns"
    );
    drawing.run(2);
    assert!(drawing.marks().is_empty(), "letting go empties the set");
}

#[test]
fn a_touch_set_at_the_root_reaches_a_reader_inside_the_pattern() {
    let mut drawing = Drawing::boot();
    drawing.run(5);
    let root = drawing.root_scope();
    assert_ne!(root, drawing.effect_scope());

    drawing.touch(root, &[Touch::new(3, [0.0, 0.0])]);
    drawing.run(2);
    assert_eq!(drawing.marks(), vec![Touch::new(3, [0.0, 0.0])]);
}

#[test]
fn an_empty_set_is_a_release() {
    let mut drawing = Drawing::boot();
    drawing.run(5);
    let scope = drawing.effect_scope();
    drawing.touch(scope, &[Touch::new(1, [0.25, 0.25])]);
    drawing.run(2);
    assert_eq!(drawing.marks().len(), 1);

    drawing.touch(scope, &[]);
    drawing.run(2);
    assert!(drawing.marks().is_empty());
}

struct Drawing {
    fs: LpFsMemory,
    rt: LoadedProjectRuntime,
}

impl Drawing {
    fn boot() -> Self {
        let fs = project_fs();
        let services = EngineServices::new(TreePath::parse("/touch.show").expect("path"));
        let mut rt = ProjectLoader::load_from_root(&fs, services).expect("load touch project");
        rt.engine_mut()
            .set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
                lp_shader::ShaderFrontend::LpsGlsl,
            ))));
        Self { fs, rt }
    }

    fn run(&mut self, ticks: usize) {
        for tick in 0..ticks {
            self.rt
                .tick_with_residency(&self.fs, FRAME_MS)
                .unwrap_or_else(|e| panic!("tick {tick}: {e}"));
        }
    }

    fn touch(&mut self, scope: ScopeRef, touches: &[Touch]) {
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

    fn node(&self, suffix: &str) -> NodeId {
        self.rt
            .engine()
            .tree()
            .entries()
            .find(|entry| entry.path.to_string().ends_with(suffix))
            .unwrap_or_else(|| panic!("{suffix} in tree"))
            .id
    }

    /// The scope the compute shader reads from: the `effect` module's own.
    fn effect_scope(&self) -> ScopeRef {
        self.rt
            .engine()
            .tree()
            .bus_read_scope(self.node("sim.compute_shader"))
            .expect("sim reads a scope")
    }

    fn root_scope(&self) -> ScopeRef {
        self.rt
            .engine()
            .tree()
            .bus_read_scope(self.node("fixture.fixture"))
            .expect("fixture reads the root scope")
    }

    /// The touches the compute shader copied out, in slot order.
    fn marks(&self) -> Vec<Touch> {
        let engine = self.rt.engine();
        let entry = engine
            .tree()
            .entries()
            .find(|entry| entry.path.to_string().ends_with("sim.compute_shader"))
            .expect("sim in tree");
        let NodeEntryState::Alive(node) = entry.state.value() else {
            panic!("sim alive");
        };
        let Some(state) = node.runtime_state_slots() else {
            return Vec::new();
        };
        let Ok(SlotDataAccess::Map(map)) = lookup_slot_data(
            state,
            engine.slot_shapes(),
            &SlotPath::parse("marks").expect("path"),
        ) else {
            return Vec::new();
        };
        map.keys()
            .into_iter()
            .filter_map(|key| match map.get(&key)? {
                SlotDataAccess::Value(value) => Touch::from_lp_value(&value.value()).ok(),
                _ => None,
            })
            .filter(|touch| touch.id != 0)
            .collect()
    }
}

const SIM_GLSL: &str = "\
// Copy the touch set out, slot for slot. Constant indices: a runtime index
// into a uniform struct array has no element address to lower against.
void tick() {
    marks[0].id = touches[0].id; marks[0].pos = touches[0].pos;
    marks[1].id = touches[1].id; marks[1].pos = touches[1].pos;
    marks[2].id = touches[2].id; marks[2].pos = touches[2].pos;
    marks[3].id = touches[3].id; marks[3].pos = touches[3].pos;
}
";

const RENDER_GLSL: &str = "\
struct Touch {
    uint id;
    vec2 pos;
};

layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform Touch marks[4];

vec4 render_2d(vec2 pos) {
    float lit = marks[0].id == 0u ? 0.0 : 1.0;
    return vec4(lit, 0.0, 0.0, 1.0);
}
";

fn touch_map_slot(direction_note: &str) -> String {
    format!(
        r#"{{
      "kind": "map",
      "value": "lp::touch::Touch",
      "key": "u32",
      "default": 0,
      "mapping": {{ "kind": "sentinel", "len": 4, "key": "id", "empty_key": 0 }},
      "label": "",
      "description": "{direction_note}"
    }}"#
    )
}

fn project_fs() -> LpFsMemory {
    let fs = LpFsMemory::new();
    write(&fs, "/project.json", b"{\n  \"format\": 11\n}\n");
    write(
        &fs,
        "/module.json",
        br#"{
  "kind": "Module",
  "nodes": {
    "clock": { "ref": "./clock.json" },
    "effect": { "ref": "./effect/module.json" },
    "fixture": { "ref": "./fixture.json" },
    "output": { "ref": "./output.json" }
  }
}"#,
    );
    write(&fs, "/clock.json", br#"{ "kind": "Clock" }"#);
    write(
        &fs,
        "/effect/module.json",
        br#"{
  "kind": "Module",
  "nodes": {
    "render": { "ref": "./render.json" },
    "sim": { "ref": "./sim.json" }
  }
}"#,
    );
    write(
        &fs,
        "/effect/sim.json",
        format!(
            r#"{{
  "kind": "ComputeShader",
  "source": "sim.glsl",
  "bindings": {{ "touches": {{ "source": "bus:touches" }} }},
  "consumed": {{ "touches": {touches} }},
  "produced": {{ "marks": {marks} }}
}}"#,
            touches = touch_map_slot("fingers"),
            marks = touch_map_slot("copied"),
        )
        .as_bytes(),
    );
    write(&fs, "/effect/sim.glsl", SIM_GLSL.as_bytes());
    write(
        &fs,
        "/effect/render.json",
        format!(
            r#"{{
  "kind": "Shader",
  "source": "render.glsl",
  "render_order": 0,
  "bindings": {{
    "marks": {{ "source": "node:../sim#marks" }},
    "output": {{ "target": "bus:visual.out" }}
  }},
  "consumed": {{ "marks": {marks} }}
}}"#,
            marks = touch_map_slot("copied"),
        )
        .as_bytes(),
    );
    write(&fs, "/effect/render.glsl", RENDER_GLSL.as_bytes());
    write(
        &fs,
        "/fixture.json",
        include_bytes!("../../../projects/test/basic/fixture.json"),
    );
    write(
        &fs,
        "/fixture.map2d.json",
        include_bytes!("../../../projects/test/basic/fixture.map2d.json"),
    );
    write(
        &fs,
        "/output.json",
        include_bytes!("../../../projects/test/basic/output.json"),
    );
    fs
}

fn write(fs: &LpFsMemory, path: &str, bytes: &[u8]) {
    fs.write_file(path.as_path(), bytes).expect("write fixture");
}
