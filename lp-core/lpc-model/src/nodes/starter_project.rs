//! Shared "blank-with-content" starter project composition.
//!
//! The single source of truth for the default demo project (clock + texture +
//! shader + GLSL + output + fixture) that `lp create` / `lp serve --init`
//! scaffold. It is built on the per-kind starters in
//! [`crate::nodes::starter`], with the cross-node bus wiring and the richer
//! demo shader layered on top.
//!
//! Pure data + serialization: the composition returns `(relative path,
//! bytes)` pairs and never touches a filesystem — writing is the caller's
//! edge concern.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lp_collection::VecMap;

use crate::node::kind::NodeKind;
use crate::nodes::fixture::{ColorOrder, FixtureSamplingConfig};
use crate::nodes::node_def::NodeDefWriteError;
use crate::nodes::starter::{starter_def_for_kind, starter_for_kind};
use crate::{
    ArtifactSpec, BindingDef, BindingDefs, BindingRef, BusSlotRef, ChannelName, MapSlot, NodeDef,
    NodeInvocation, NodeInvocationSlot, OptionSlot, ProjectDef, SlotShapeRegistry, ValueSlot,
};

/// The starter project's complete file set as `(relative path, bytes)` —
/// deterministic, ready to write into an empty project directory. Every
/// `.json` entry is canonical [`NodeDef::write_json`] output.
pub fn starter_project_files(
    name: &str,
    registry: &SlotShapeRegistry,
) -> Result<Vec<(String, Vec<u8>)>, NodeDefWriteError> {
    let mut files = Vec::new();

    files.push((
        String::from("project.json"),
        node_json(&starter_project_def(name), registry)?,
    ));

    files.push((
        String::from("clock.json"),
        node_json(&starter_def_for_kind(NodeKind::Clock), registry)?,
    ));

    let mut texture = starter_def_for_kind(NodeKind::Texture);
    if let NodeDef::Texture(texture) = &mut texture {
        texture.bindings = bus_input_binding_defs("visual.out");
    }
    files.push((String::from("texture.json"), node_json(&texture, registry)?));

    let shader = starter_for_kind(NodeKind::Shader)
        .expect("shader starter exists")
        .for_stem("shader");
    let mut shader_def = shader.def;
    if let NodeDef::Shader(shader_def) = &mut shader_def {
        shader_def.bindings = bus_output_binding_defs("visual.out");
    }
    files.push((
        String::from("shader.json"),
        node_json(&shader_def, registry)?,
    ));
    // The demo project ships the rainbow color wheel instead of the minimal
    // red-pulse scaffold: `lp create` output should show something pretty.
    files.push((
        String::from("shader.glsl"),
        STARTER_PROJECT_SHADER_GLSL.as_bytes().to_vec(),
    ));

    let mut output = starter_def_for_kind(NodeKind::Output);
    if let NodeDef::Output(output) = &mut output {
        output.bindings = bus_input_binding_defs("control.out");
    }
    files.push((String::from("output.json"), node_json(&output, registry)?));

    let mut fixture = starter_def_for_kind(NodeKind::Fixture);
    if let NodeDef::Fixture(fixture) = &mut fixture {
        fixture.bindings = fixture_binding_defs();
        fixture.sampling = ValueSlot::new(FixtureSamplingConfig::TextureArea);
        fixture.color_order = ValueSlot::new(ColorOrder::Rgb);
    }
    files.push((String::from("fixture.json"), node_json(&fixture, registry)?));

    Ok(files)
}

/// A minimal effect folder as `(relative path, bytes)` pairs — folder-
/// relative, ready to rebase under `effects/<name>/` in a host project or
/// to open standalone (effects-are-projects ADR). One working promoted
/// control (`speed`) out of the box; consumed `time` inherits the host
/// clock through the scoped bus.
pub fn effect_starter_files(
    name: &str,
    registry: &SlotShapeRegistry,
) -> Result<Vec<(String, Vec<u8>)>, NodeDefWriteError> {
    use crate::AssetSlot;
    use crate::nodes::project::PromotedControlDef;
    use crate::nodes::shader::{ShaderDef, ShaderSlotDef};
    use crate::nodes::starter::{starter_glsl_opts, starter_time_consumed_slots};

    let mut files = Vec::new();

    let mut nodes = VecMap::new();
    nodes.insert(
        String::from("shader"),
        NodeInvocationSlot::new(NodeInvocation::path(ArtifactSpec::path("./shader.json"))),
    );
    let mut controls = VecMap::new();
    controls.insert(
        String::from("speed"),
        PromotedControlDef::to_target(
            BindingRef::parse("node:shader#speed").expect("starter control target"),
        ),
    );
    let project = ProjectDef {
        format: ProjectDef::current_format_slot(),
        name: OptionSlot::some(ValueSlot::new(String::from(name))),
        nodes: MapSlot::new(nodes),
        controls: MapSlot::new(controls),
        ..ProjectDef::default()
    };
    files.push((
        String::from("project.json"),
        node_json(&NodeDef::Project(project), registry)?,
    ));

    let mut consumed = starter_time_consumed_slots();
    let mut speed = ShaderSlotDef::value_f32("Speed", "Pulse rate multiplier", 1.0, Some(0.0));
    speed.max = OptionSlot::some(ValueSlot::new(4.0));
    speed.panel = OptionSlot::some(ValueSlot::new(true));
    consumed.entries.insert(String::from("speed"), speed);
    let shader = ShaderDef {
        source: AssetSlot::path("./main.glsl"),
        glsl_opts: starter_glsl_opts(),
        consumed_slots: consumed,
        ..ShaderDef::default()
    };
    files.push((
        String::from("shader.json"),
        node_json(&NodeDef::Shader(shader), registry)?,
    ));

    files.push((
        String::from("main.glsl"),
        EFFECT_STARTER_GLSL.as_bytes().to_vec(),
    ));

    Ok(files)
}

/// The effect starter's scaffold body: the red-pulse starter driven by the
/// promoted `speed` control.
const EFFECT_STARTER_GLSL: &str = "layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float time;
layout(binding = 2) uniform float speed;

vec4 render(vec2 pos) {
    return vec4(mod(time * speed, 1.0), 0.0, 0.0, 1.0);
}
";

fn starter_project_def(name: &str) -> NodeDef {
    let mut nodes = VecMap::new();
    for node in ["output", "clock", "texture", "shader", "fixture"] {
        nodes.insert(
            String::from(node),
            NodeInvocationSlot::new(NodeInvocation::path(ArtifactSpec::path(format!(
                "./{node}.json"
            )))),
        );
    }
    NodeDef::Project(ProjectDef {
        format: ProjectDef::current_format_slot(),
        name: OptionSlot::some(ValueSlot::new(String::from(name))),
        nodes: MapSlot::new(nodes),
        ..ProjectDef::default()
    })
}

fn node_json(node: &NodeDef, registry: &SlotShapeRegistry) -> Result<Vec<u8>, NodeDefWriteError> {
    node.write_json(registry).map(String::into_bytes)
}

fn bus_input_binding_defs(slot: &str) -> BindingDefs {
    single_binding_defs("input", BindingDef::source(bus_ref(slot)))
}

fn bus_output_binding_defs(slot: &str) -> BindingDefs {
    single_binding_defs("output", BindingDef::target(bus_ref(slot)))
}

fn fixture_binding_defs() -> BindingDefs {
    let mut entries = VecMap::new();
    entries.insert(
        String::from("input"),
        BindingDef::source(bus_ref("visual.out")),
    );
    entries.insert(
        String::from("output"),
        BindingDef::target(bus_ref("control.out")),
    );
    BindingDefs::new(entries)
}

fn single_binding_defs(slot: &str, binding: BindingDef) -> BindingDefs {
    let mut entries = VecMap::new();
    entries.insert(String::from(slot), binding);
    BindingDefs::new(entries)
}

fn bus_ref(slot: &str) -> BindingRef {
    BindingRef::Bus(BusSlotRef::new(ChannelName(String::from(slot))))
}

/// Demo shader for the starter project: a rotating rainbow color wheel.
const STARTER_PROJECT_SHADER_GLSL: &str = r#"// HSV to RGB conversion function
vec3 hsv_to_rgb(float h, float s, float v) {
    // h in [0, 1], s in [0, 1], v in [0, 1]
    float c = v * s;
    float x = c * (1.0 - abs(mod(h * 6.0, 2.0) - 1.0));
    float m = v - c;

    vec3 rgb;
    if (h < 1.0/6.0) {
        rgb = vec3(v, m + x, m);
    } else if (h < 2.0/6.0) {
        rgb = vec3(m + x, v, m);
    } else if (h < 3.0/6.0) {
        rgb = vec3(m, v, m + x);
    } else if (h < 4.0/6.0) {
        rgb = vec3(m, m + x, v);
    } else if (h < 5.0/6.0) {
        rgb = vec3(m + x, m, v);
    } else {
        rgb = vec3(v, m, m + x);
    }

    return rgb;
}

layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float time;

vec4 render(vec2 pos) {
    // Center of texture
    vec2 center = outputSize * 0.5;

    // Direction from center to fragment
    vec2 dir = pos - center;

    // Calculate angle (atan2 gives angle in [-PI, PI])
    float angle = atan(dir.y, dir.x);

    // Rotate angle with time (full rotation every 2 seconds)
    angle = (angle + time * 3.14159);

    // Normalize angle to [0, 1] for hue
    // atan returns [-PI, PI], map to [0, 1] by: (angle + PI) / (2 * PI)
    // Wrap hue to [0, 1] using mod to handle large time values
    float hue = mod((angle + 3.14159) / (2.0 * 3.14159), 1.0);

    // Distance from center (normalized to [0, 1])
    float maxDist = length(outputSize * 0.5);
    float dist = length(dir) / maxDist;

    // Clamp distance to prevent issues
    dist = min(dist, 1.0);

    // Value (brightness): highest at center, darker at edges
    float value = 1.0 - dist * 0.5;

    // Convert HSV to RGB
    vec3 rgb = hsv_to_rgb(hue, 1.0, value);

    // Clamp to [0, 1] and return
    return vec4(max(vec3(0.0), min(vec3(1.0), rgb)), 1.0);
}"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MappingConfig, PROJECT_FORMAT_VERSION};
    use alloc::string::ToString;

    #[test]
    fn effect_starter_files_compose_a_working_folder() {
        let registry = SlotShapeRegistry::default();
        let files = effect_starter_files("glow", &registry).expect("compose");
        let names: Vec<&str> = files.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["project.json", "shader.json", "main.glsl"]);

        for (name, bytes) in &files {
            if !name.ends_with(".json") {
                continue;
            }
            let text = core::str::from_utf8(bytes).expect("utf-8");
            let def = NodeDef::read_json(&registry, text).expect("artifact parses");
            let rewritten = def.write_json(&registry).expect("re-write");
            assert_eq!(
                rewritten, text,
                "{name} must be canonical write_json output"
            );
        }

        let (_, project) = files
            .iter()
            .find(|(name, _)| name == "project.json")
            .unwrap();
        let project =
            NodeDef::read_json(&registry, core::str::from_utf8(project).unwrap()).unwrap();
        let project = project.as_project().expect("project def");
        assert_eq!(project.format(), Some(PROJECT_FORMAT_VERSION));
        let control = project
            .controls
            .entries
            .get("speed")
            .expect("speed control");
        assert_eq!(control.target.value().to_string(), "node:shader#speed");

        let (_, shader) = files
            .iter()
            .find(|(name, _)| name == "shader.json")
            .unwrap();
        let shader = NodeDef::read_json(&registry, core::str::from_utf8(shader).unwrap()).unwrap();
        let NodeDef::Shader(shader) = shader else {
            panic!("expected shader");
        };
        let speed = shader
            .consumed_slots
            .entries
            .get("speed")
            .expect("speed slot");
        assert_eq!(
            speed.panel.data.as_ref().map(|slot| *slot.value()),
            Some(true)
        );
        assert!(shader.consumed_slots.entries.get("time").is_some());
    }

    #[test]
    fn starter_project_files_are_complete_and_parse() {
        let registry = SlotShapeRegistry::default();
        let files = starter_project_files("demo", &registry).expect("compose");
        let names: Vec<&str> = files.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            names,
            [
                "project.json",
                "clock.json",
                "texture.json",
                "shader.json",
                "shader.glsl",
                "output.json",
                "fixture.json",
            ]
        );

        for (name, bytes) in &files {
            if !name.ends_with(".json") {
                continue;
            }
            let text = core::str::from_utf8(bytes).expect("utf-8");
            let def = NodeDef::read_json(&registry, text).expect("artifact parses");
            let rewritten = def.write_json(&registry).expect("re-write");
            assert_eq!(
                rewritten, text,
                "{name} must be canonical write_json output"
            );
        }
    }

    #[test]
    fn starter_project_root_carries_format_and_name() {
        let registry = SlotShapeRegistry::default();
        let files = starter_project_files("porch sign", &registry).expect("compose");
        let (_, bytes) = files
            .iter()
            .find(|(name, _)| name == "project.json")
            .unwrap();
        let def = NodeDef::read_json(&registry, core::str::from_utf8(bytes).unwrap()).unwrap();
        let NodeDef::Project(project) = def else {
            panic!("expected project root");
        };
        assert_eq!(
            project.format.data.as_ref().map(|slot| *slot.value()),
            Some(PROJECT_FORMAT_VERSION)
        );
        assert_eq!(project.name(), Some("porch sign"));
        assert_eq!(project.nodes.entries.len(), 5);
    }

    #[test]
    fn starter_project_shader_references_shader_glsl() {
        let registry = SlotShapeRegistry::default();
        let files = starter_project_files("demo", &registry).expect("compose");
        let (_, bytes) = files
            .iter()
            .find(|(name, _)| name == "shader.json")
            .unwrap();
        let def = NodeDef::read_json(&registry, core::str::from_utf8(bytes).unwrap()).unwrap();
        let NodeDef::Shader(shader) = def else {
            panic!("expected shader");
        };
        assert_eq!(
            shader.shader_source().artifact_value().unwrap().to_string(),
            "shader.glsl"
        );
        assert!(shader.consumed_slots.entries.get("time").is_some());
    }

    #[test]
    fn starter_project_fixture_has_ring_mapping() {
        let registry = SlotShapeRegistry::default();
        let files = starter_project_files("demo", &registry).expect("compose");
        let (_, bytes) = files
            .iter()
            .find(|(name, _)| name == "fixture.json")
            .unwrap();
        let def = NodeDef::read_json(&registry, core::str::from_utf8(bytes).unwrap()).unwrap();
        let NodeDef::Fixture(fixture) = def else {
            panic!("expected fixture");
        };
        assert!(matches!(
            fixture.mapping.value(),
            MappingConfig::PathPoints { .. }
        ));
    }
}
