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
    ArtifactSpec, BindingDef, BindingDefs, BindingRef, BusSlotRef, ChannelName, MapSlot, ModuleDef,
    NodeDef, NodeInvocation, NodeInvocationSlot, ProjectManifest, SlotShapeRegistry, ValueSlot,
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
        ProjectManifest::new_current(name).write_json().into_bytes(),
    ));

    files.push((
        String::from("module.json"),
        node_json(&starter_module_def(), registry)?,
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

    // The fixture starter carries a sibling mapping document; substitute
    // the real stem and ship both files.
    let fixture_starter = crate::nodes::starter::starter_for_kind(NodeKind::Fixture)
        .expect("fixture kind has a starter")
        .for_stem("fixture");
    let mut fixture = fixture_starter.def;
    if let NodeDef::Fixture(fixture) = &mut fixture {
        fixture.bindings = fixture_binding_defs();
        fixture.sampling = ValueSlot::new(FixtureSamplingConfig::TextureArea);
        fixture.color_order = ValueSlot::new(ColorOrder::Rgb);
    }
    files.push((String::from("fixture.json"), node_json(&fixture, registry)?));
    files.extend(fixture_starter.assets);

    Ok(files)
}

fn starter_module_def() -> NodeDef {
    let mut nodes = VecMap::new();
    for node in ["output", "clock", "texture", "shader", "fixture"] {
        nodes.insert(
            String::from(node),
            NodeInvocationSlot::new(NodeInvocation::path(ArtifactSpec::path(format!(
                "./{node}.json"
            )))),
        );
    }
    NodeDef::Module(ModuleDef {
        nodes: MapSlot::new(nodes),
        ..ModuleDef::default()
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
layout(binding = 1) uniform float phase;

vec4 render_2d(vec2 pos) {
    // Center of texture
    vec2 center = outputSize * 0.5;

    // Direction from center to fragment
    vec2 dir = pos - center;

    // Calculate angle (atan2 gives angle in [-PI, PI])
    float angle = atan(dir.y, dir.x);

    // Rotate angle with the phasor (one full rotation per phasor period)
    angle = (angle + phase * 6.28318);

    // Normalize angle to [0, 1] for hue
    // atan returns [-PI, PI], map to [0, 1] by: (angle + PI) / (2 * PI)
    // Wrap hue to [0, 1] using mod
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
    fn starter_project_files_are_complete_and_parse() {
        let registry = SlotShapeRegistry::default();
        let files = starter_project_files("demo", &registry).expect("compose");
        let names: Vec<&str> = files.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            names,
            [
                "project.json",
                "module.json",
                "clock.json",
                "texture.json",
                "shader.json",
                "shader.glsl",
                "output.json",
                "fixture.json",
                "fixture.map2d.json",
            ]
        );

        for (name, bytes) in &files {
            // Only node-definition artifacts parse as NodeDef; the mapping
            // document is an opaque asset (D1) and `project.json` is the
            // non-node container manifest (its canonical form is
            // `ProjectManifest::write_json`, asserted below).
            if !name.ends_with(".json") || name.ends_with(".map2d.json") || name == "project.json" {
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
    fn starter_project_manifest_carries_format_and_name_root_module_the_nodes() {
        let registry = SlotShapeRegistry::default();
        let files = starter_project_files("porch sign", &registry).expect("compose");

        let (_, bytes) = files
            .iter()
            .find(|(name, _)| name == "project.json")
            .unwrap();
        let text = core::str::from_utf8(bytes).unwrap();
        let manifest = ProjectManifest::read_json(text).expect("container manifest");
        assert_eq!(manifest.format, Some(PROJECT_FORMAT_VERSION));
        assert_eq!(manifest.name.as_deref(), Some("porch sign"));
        assert_eq!(manifest.write_json(), text, "manifest must be canonical");

        let (_, bytes) = files
            .iter()
            .find(|(name, _)| name == "module.json")
            .unwrap();
        let def = NodeDef::read_json(&registry, core::str::from_utf8(bytes).unwrap()).unwrap();
        let NodeDef::Module(module) = def else {
            panic!("expected module root");
        };
        assert_eq!(module.nodes.entries.len(), 5);
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
        assert!(shader.consumed_slots.entries.get("phase").is_some());
    }

    #[test]
    fn starter_project_fixture_has_map2d_mapping_and_ships_the_doc() {
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
        let MappingConfig::Map2d { source } = fixture.mapping.value() else {
            panic!("expected Map2d mapping");
        };
        assert_eq!(
            source.artifact_value().unwrap().to_string(),
            "fixture.map2d.json"
        );
        assert!(
            files.iter().any(|(name, _)| name == "fixture.map2d.json"),
            "the mapping document ships with the project"
        );
    }
}
