//! Two fixtures, one wire: the output-fragment path end to end.
//!
//! The unit tests in `output_node.rs` pin the placement arithmetic and the
//! degrade-and-report rules. This file pins the thing only a real load can
//! prove: that two fixtures publishing to one `control.out` actually reach one
//! output as an ordered fragment set, because `OutputDef::input` declares
//! `merge = "fragments"` and the resolver honours it. Before this phase the
//! same project was an `AmbiguousBusBinding` — two producers on one channel
//! had no defined meaning.
//!
//! ```bash
//! cargo test -p lpc-engine --test output_fragments
//! ```

use lpc_engine::engine::LoadedProjectRuntime;
use lpc_engine::{EngineServices, ProjectLoader};
use lpc_model::{NodeRuntimeStatus, TreePath};
use lpfs::{AsLpPath, LpFs, LpFsMemory};

/// A module wiring `fixtures` (name → lamp count) to one output on
/// `bus:control.out`, all of them reading one solid-white shader.
///
/// Lamp counts differ per fixture so a concatenation's *order* is legible in
/// the buffer's byte length alone, and brightness differs so the boundary
/// between two fragments is legible in its contents.
fn project_fs(fixtures: &[(&str, u32, f32)]) -> LpFsMemory {
    let fs = LpFsMemory::new();
    fs.write_file("/project.json".as_path(), b"{\n  \"format\": 10\n}\n")
        .expect("container manifest");

    let mut nodes = String::from(
        "\"shader\": { \"ref\": \"./shader.json\" }, \"output\": { \"ref\": \"./output.json\" }",
    );
    for (name, _, _) in fixtures {
        nodes.push_str(&format!(", {name:?}: {{ \"ref\": \"./{name}.json\" }}"));
    }
    fs.write_file(
        "/module.json".as_path(),
        format!("{{ \"kind\": \"Module\", \"nodes\": {{ {nodes} }} }}").as_bytes(),
    )
    .expect("module.json");

    fs.write_file(
        "/shader.json".as_path(),
        br#"{
  "kind": "Shader",
  "source": "shader.glsl",
  "bindings": { "output": { "target": "bus:visual.out" } }
}"#,
    )
    .expect("shader.json");
    fs.write_file(
        "/shader.glsl".as_path(),
        b"vec4 render_2d(vec2 p) { return vec4(1.0, 1.0, 1.0, 1.0); }",
    )
    .expect("shader.glsl");

    fs.write_file(
        "/output.json".as_path(),
        br#"{
  "kind": "Output",
  "ports": { "0": { "endpoint": "ws281x:local:D10" } },
  "bindings": { "input": { "source": "bus:control.out" } }
}"#,
    )
    .expect("output.json");

    for (name, lamps, brightness) in fixtures {
        fs.write_file(
            format!("/{name}.json").as_path(),
            format!(
                r#"{{
  "kind": "Fixture",
  "render_size": {{ "width": {lamps}, "height": 1 }},
  "bindings": {{
    "input": {{ "source": "bus:visual.out" }},
    "output": {{ "target": "bus:control.out" }}
  }},
  "sampling": "direct",
  "mapping": {{ "kind": "Map2d", "source": "{name}.map2d.json" }},
  "color_order": "rgb",
  "brightness": {brightness},
  "gamma_correction": false
}}"#
            )
            .as_bytes(),
        )
        .expect("fixture def");
        fs.write_file(
            format!("/{name}.map2d.json").as_path(),
            format!(
                r#"{{
  "format": 1,
  "sample_diameter": 1.0,
  "canvas": [0.0, 0.0, {lamps}.0, 1.0],
  "objects": [
    {{ "name": "strip", "shape": {{ "grid": {{ "origin": [0.5, 0.5], "cols": {lamps}, "rows": 1, "pitch": 1 }} }} }}
  ]
}}"#
            )
            .as_bytes(),
        )
        .expect("map2d");
    }
    fs
}

fn load(fs: &LpFsMemory) -> LoadedProjectRuntime {
    let services = EngineServices::new(TreePath::parse("/fragments.show").expect("path"));
    ProjectLoader::load_from_root(fs, services).expect("load fragment project")
}

/// Ticks before a buffer is read. The first frame compiles the shader and
/// establishes extents, so it is black by construction — three is settled.
const SETTLE_TICKS: usize = 3;

/// The output's published samples once the graph has settled, as u16s.
fn published_samples(fixtures: &[(&str, u32, f32)]) -> Vec<u16> {
    let fs = project_fs(fixtures);
    let mut rt = load(&fs);
    rt.engine_mut().set_graphics(Some(std::sync::Arc::new(
        lp_gfx_lpvm::TargetLpvmGraphics::new(lp_shader::ShaderFrontend::LpsGlsl),
    )));
    for _ in 0..SETTLE_TICKS {
        rt.tick(16).expect("tick");
    }

    let engine = rt.engine();
    let entry = engine
        .tree()
        .entries()
        .find(|entry| entry.path.to_string().ends_with("output.output"))
        .expect("output node in tree");
    let buffer_id = engine
        .runtime_output_sink_buffer_id(entry.id)
        .expect("output sink buffer");
    engine
        .runtime_buffers()
        .get(buffer_id)
        .expect("sink buffer")
        .value()
        .samples16()
        .expect("output channels are u16 samples")
        .to_vec()
}

/// The status of the output node once the graph has settled.
fn output_status(fixtures: &[(&str, u32, f32)]) -> Option<NodeRuntimeStatus> {
    let fs = project_fs(fixtures);
    let mut rt = load(&fs);
    rt.engine_mut().set_graphics(Some(std::sync::Arc::new(
        lp_gfx_lpvm::TargetLpvmGraphics::new(lp_shader::ShaderFrontend::LpsGlsl),
    )));
    for _ in 0..SETTLE_TICKS {
        rt.tick(16).expect("tick");
    }
    let engine = rt.engine();
    let entry = engine
        .tree()
        .entries()
        .find(|entry| entry.path.to_string().ends_with("output.output"))
        .expect("output node in tree");
    let lpc_engine::node::NodeEntryState::Alive(node) = entry.state.value() else {
        panic!("output node alive");
    };
    node.runtime_status()
}

/// The baseline the whole phase protects: one fixture is still one fragment
/// covering the whole buffer, and the bytes are the fixture's, unchanged.
#[test]
fn one_fixture_fills_the_whole_buffer() {
    let samples = published_samples(&[("strip", 4, 1.0)]);

    assert_eq!(samples.len(), 12, "four RGB lamps");
    assert!(
        samples.iter().all(|sample| *sample == u16::MAX),
        "a solid-white shader at full brightness, {samples:?}"
    );
    assert_eq!(output_status(&[("strip", 4, 1.0)]), None);
}

/// The claim: two fixtures on one channel concatenate rather than collide.
#[test]
fn two_fixtures_on_one_channel_concatenate_into_one_buffer() {
    let samples = published_samples(&[("a_strip", 2, 1.0), ("b_strip", 3, 0.25)]);

    assert_eq!(samples.len(), 15, "2 lamps then 3 lamps, RGB");
    assert!(
        samples[..6].iter().all(|sample| *sample == u16::MAX),
        "the full-brightness fixture's two lamps come first: {samples:?}"
    );
    assert!(
        samples[6..].iter().all(|sample| *sample < u16::MAX / 2),
        "the quarter-brightness fixture's three lamps follow: {samples:?}"
    );
    assert_eq!(
        output_status(&[("a_strip", 2, 1.0), ("b_strip", 3, 0.25)]),
        None,
        "auto-flow can neither overlap nor gap, so a merged output is clean",
    );
}

/// Placement is DETERMINISTIC and it follows the module's node order, not the
/// order the files happen to be written in or any hash iteration. The two
/// projects below differ only in which name carries which fixture, and the
/// buffer follows the names.
#[test]
fn fragment_order_follows_module_node_order() {
    let a_first = published_samples(&[("a_strip", 2, 1.0), ("b_strip", 3, 0.25)]);
    let b_first = published_samples(&[("a_strip", 3, 0.25), ("b_strip", 2, 1.0)]);

    assert_eq!(a_first.len(), 15);
    assert_eq!(b_first.len(), 15);
    assert!(
        a_first[..6].iter().all(|sample| *sample == u16::MAX),
        "`a_strip` leads in both: {a_first:?}"
    );
    assert!(
        b_first[..9].iter().all(|sample| *sample < u16::MAX / 2),
        "`a_strip` leads in both, and here it is the dim three-lamp one: {b_first:?}"
    );
}

/// Repeatability, stated as its own claim: the same project loaded twice
/// places its fragments the same way. A placement that depended on hash order
/// would pass the ordering test above and fail here.
#[test]
fn fragment_placement_is_stable_across_loads() {
    let fixtures = [("a_strip", 2, 1.0), ("b_strip", 3, 0.25)];
    assert_eq!(published_samples(&fixtures), published_samples(&fixtures));
}

/// Three producers, to prove the fold is a running sum and not a special case
/// for pairs.
#[test]
fn three_fixtures_chain_end_to_end() {
    let samples = published_samples(&[("a", 1, 1.0), ("b", 2, 0.25), ("c", 3, 1.0)]);

    assert_eq!(samples.len(), 18);
    assert!(samples[..3].iter().all(|sample| *sample == u16::MAX));
    assert!(samples[3..9].iter().all(|sample| *sample < u16::MAX / 2));
    assert!(samples[9..].iter().all(|sample| *sample == u16::MAX));
}
