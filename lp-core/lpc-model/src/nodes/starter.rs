//! Data-driven starter templates for newly created node artifacts.
//!
//! [`NodeDef::default_for_kind`] doubles as the parse-time fill-in for absent
//! fields, so its `Default` impls must stay untouched. This table layers
//! authored starter content on top for the kinds whose bare default is not a
//! usable authoring target (0×0 textures, dangling shader sources, unset
//! fixture mappings). Kinds without an entry are created from the bare
//! default with no assets.
//!
//! The table is pure data: serialization stays the caller's job via
//! [`NodeDef::write_json`], and writing files is an edge concern.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::node::kind::NodeKind;
use crate::nodes::fixture::{FixtureDef, MappingConfig};
use crate::nodes::shader::{ComputeShaderDef, ShaderDef, ShaderSlotDef};
use crate::nodes::texture::TextureDef;
use crate::{AssetSlot, BindingRef, EnumSlot, MapSlot, NodeDef};

/// Placeholder in starter asset names and asset references. Callers substitute
/// the artifact file stem (e.g. `pulse.json` ⇒ stem `pulse`) via
/// [`NodeStarter::for_stem`] before writing files.
pub const STARTER_STEM_PLACEHOLDER: &str = "{stem}";

/// Canonical scaffold shader: the smallest animating body in the repo (red
/// pulse). It proves compile plus clock binding on first render.
pub const STARTER_SHADER_GLSL: &str = "layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float time;

vec4 render(vec2 pos) {
    return vec4(mod(time, 1.0), 0.0, 0.0, 1.0);
}
";

/// Canonical scaffold compute body: one produced value tracking the clock.
/// A compute shader's bare default references a dangling `main.glsl` and
/// fails the runtime spine load outright, so the starter must scaffold a
/// real, trivially-compiling source.
pub const STARTER_COMPUTE_GLSL: &str = "void tick() { phase = mod(time, 1.0); }
";

/// Canonical scaffold mapping document: a small grid, immediately visible
/// and immediately editable in the in-place mapping editor (resize it,
/// replace it, or delete it and draw the real fixture).
pub const STARTER_FIXTURE_MAP2D: &str = r#"{
  "format": 1,
  "objects": [
    { "name": "grid", "shape": { "grid": { "origin": [0, 0], "cols": 8, "rows": 8, "pitch": 10 } } }
  ]
}
"#;

/// A starter node artifact: the authored definition plus any sibling assets
/// it references (paths relative to the def file's directory, named with
/// [`STARTER_STEM_PLACEHOLDER`]).
#[derive(Clone, Debug, PartialEq)]
pub struct NodeStarter {
    pub def: NodeDef,
    /// Sibling assets as `(name, bytes)`, e.g. `("{stem}.glsl", …)`.
    pub assets: Vec<(String, Vec<u8>)>,
}

impl NodeStarter {
    fn def_only(def: NodeDef) -> Self {
        Self {
            def,
            assets: Vec::new(),
        }
    }

    /// Substitute the artifact file stem into asset names and the def's
    /// asset references, yielding a starter ready to serialize and write.
    pub fn for_stem(mut self, stem: &str) -> Self {
        for (name, _) in &mut self.assets {
            *name = name.replace(STARTER_STEM_PLACEHOLDER, stem);
        }
        match &mut self.def {
            NodeDef::Shader(shader) => {
                if let Some(spec) = shader.source.artifact_value() {
                    let substituted = spec.to_string().replace(STARTER_STEM_PLACEHOLDER, stem);
                    shader.source = AssetSlot::path(substituted);
                }
            }
            NodeDef::ComputeShader(compute) => {
                if let Some(spec) = compute.source.artifact_value() {
                    let substituted = spec.to_string().replace(STARTER_STEM_PLACEHOLDER, stem);
                    compute.source = AssetSlot::path(substituted);
                }
            }
            NodeDef::Fixture(fixture) => {
                if let MappingConfig::Map2d { source } = fixture.mapping.value()
                    && let Some(spec) = source.artifact_value()
                {
                    let substituted = spec.to_string().replace(STARTER_STEM_PLACEHOLDER, stem);
                    fixture.mapping = EnumSlot::new(MappingConfig::Map2d {
                        source: AssetSlot::path(substituted),
                    });
                }
            }
            _ => {}
        }
        self
    }
}

/// A node def's sibling asset reference, when its kind carries one.
///
/// Shares [`NodeStarter::for_stem`]'s knowledge of which kinds reference
/// assets, so a new asset-bearing kind is added in one place.
pub fn node_def_asset_ref(def: &NodeDef) -> Option<String> {
    match def {
        NodeDef::Shader(shader) => shader.source.artifact_value().map(|spec| spec.to_string()),
        NodeDef::ComputeShader(compute) => {
            compute.source.artifact_value().map(|spec| spec.to_string())
        }
        _ => None,
    }
}

/// Point a node def's sibling asset reference at `path`.
///
/// Used when a copied node is pasted into a project where its original
/// filename is taken: the asset is written under a free name, and the def
/// must follow it or the pasted node references a file that is not there.
/// No-op for kinds that reference no asset.
pub fn set_node_def_asset_ref(def: &mut NodeDef, path: &str) {
    match def {
        NodeDef::Shader(shader) => {
            if shader.source.artifact_value().is_some() {
                shader.source = AssetSlot::path(path);
            }
        }
        NodeDef::ComputeShader(compute) => {
            if compute.source.artifact_value().is_some() {
                compute.source = AssetSlot::path(path);
            }
        }
        _ => {}
    }
}

/// Starter template for a node kind: `Some` when the kind carries starter
/// overrides, `None` when the bare [`NodeDef::default_for_kind`] is already a
/// usable authoring target (callers then use it directly, with no assets).
pub fn starter_for_kind(kind: NodeKind) -> Option<NodeStarter> {
    match kind {
        NodeKind::Texture => Some(NodeStarter::def_only(NodeDef::Texture(TextureDef::new(
            64, 64,
        )))),
        NodeKind::Shader => Some(NodeStarter {
            def: NodeDef::Shader(starter_shader_def()),
            assets: vec![(
                alloc::format!("{STARTER_STEM_PLACEHOLDER}.glsl"),
                STARTER_SHADER_GLSL.as_bytes().to_vec(),
            )],
        }),
        NodeKind::ComputeShader => Some(NodeStarter {
            def: NodeDef::ComputeShader(starter_compute_shader_def()),
            assets: vec![(
                alloc::format!("{STARTER_STEM_PLACEHOLDER}.glsl"),
                STARTER_COMPUTE_GLSL.as_bytes().to_vec(),
            )],
        }),
        NodeKind::Fixture => Some(NodeStarter {
            def: NodeDef::Fixture(starter_fixture_def()),
            assets: vec![(
                alloc::format!("{STARTER_STEM_PLACEHOLDER}.map2d.json"),
                STARTER_FIXTURE_MAP2D.as_bytes().to_vec(),
            )],
        }),
        _ => None,
    }
}

/// Starter definition for a kind: the table entry's def when present, the
/// bare default otherwise. Assets (shader scaffolds) are only reachable via
/// [`starter_for_kind`].
pub fn starter_def_for_kind(kind: NodeKind) -> NodeDef {
    starter_for_kind(kind).map_or_else(|| NodeDef::default_for_kind(kind), |starter| starter.def)
}

/// The `time` consumed slot every starter shader declares, default-bound to
/// the project clock bus so the scaffold animates without manual wiring.
pub fn starter_time_consumed_slots() -> MapSlot<String, ShaderSlotDef> {
    let mut slots = lp_collection::VecMap::new();
    slots.insert(
        String::from("time"),
        ShaderSlotDef::value_f32("Time", "Project clock time in seconds", 0.0, None)
            .with_default_bind(BindingRef::parse("bus:time").expect("bus:time endpoint")),
    );
    MapSlot::new(slots)
}

fn starter_shader_def() -> ShaderDef {
    ShaderDef {
        source: AssetSlot::path(alloc::format!("{STARTER_STEM_PLACEHOLDER}.glsl")),
        consumed_slots: starter_time_consumed_slots(),
        ..ShaderDef::default()
    }
}

fn starter_compute_shader_def() -> ComputeShaderDef {
    let mut produced = lp_collection::VecMap::new();
    produced.insert(
        String::from("phase"),
        ShaderSlotDef::value_f32("Phase", "Computed clock phase (0..1)", 0.0, None),
    );
    ComputeShaderDef {
        source: AssetSlot::path(alloc::format!("{STARTER_STEM_PLACEHOLDER}.glsl")),
        consumed_slots: starter_time_consumed_slots(),
        produced_slots: MapSlot::new(produced),
        ..ComputeShaderDef::default()
    }
}

fn starter_fixture_def() -> FixtureDef {
    FixtureDef {
        mapping: EnumSlot::new(MappingConfig::Map2d {
            source: AssetSlot::path(alloc::format!("{STARTER_STEM_PLACEHOLDER}.map2d.json")),
        }),
        ..FixtureDef::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SlotShapeRegistry;
    use alloc::string::ToString;

    const ALL_KINDS: &[NodeKind] = &[
        NodeKind::Module,
        NodeKind::Button,
        NodeKind::Clock,
        NodeKind::Texture,
        NodeKind::Shader,
        NodeKind::ComputeShader,
        NodeKind::Fluid,
        NodeKind::Playlist,
        NodeKind::ControlRadio,
        NodeKind::Output,
        NodeKind::Fixture,
    ];

    #[test]
    fn every_kind_starter_round_trips_byte_stable() {
        let registry = SlotShapeRegistry::default();
        for kind in ALL_KINDS {
            let def = starter_def_for_kind(*kind);
            assert_eq!(def.kind(), *kind);
            let first = def.write_json(&registry).expect("write starter");
            let read = NodeDef::read_json(&registry, &first).expect("read starter");
            let second = read.write_json(&registry).expect("re-write starter");
            assert_eq!(first, second, "{kind:?} starter must round-trip");
        }
    }

    #[test]
    fn starter_table_covers_exactly_the_gap_kinds() {
        for kind in ALL_KINDS {
            let expected = matches!(
                kind,
                NodeKind::Texture | NodeKind::Shader | NodeKind::ComputeShader | NodeKind::Fixture
            );
            assert_eq!(
                starter_for_kind(*kind).is_some(),
                expected,
                "{kind:?} starter presence"
            );
        }
    }

    #[test]
    fn compute_shader_starter_scaffolds_a_real_source_with_time_and_phase() {
        // The bare ComputeShader default references a dangling `main.glsl`
        // and fails the runtime spine load — the starter must scaffold a
        // real source plus the slots its body uses.
        let starter = starter_for_kind(NodeKind::ComputeShader)
            .expect("compute starter")
            .for_stem("pulse");
        let NodeDef::ComputeShader(compute) = &starter.def else {
            panic!("expected compute shader");
        };
        assert_eq!(
            compute
                .shader_source()
                .artifact_value()
                .unwrap()
                .to_string(),
            "pulse.glsl"
        );
        assert!(compute.consumed_slots.entries.get("time").is_some());
        assert!(compute.produced_slots.entries.get("phase").is_some());
        assert_eq!(
            starter.assets,
            vec![(
                String::from("pulse.glsl"),
                STARTER_COMPUTE_GLSL.as_bytes().to_vec()
            )]
        );
        let registry = SlotShapeRegistry::default();
        let text = starter.def.write_json(&registry).expect("write");
        let read = NodeDef::read_json(&registry, &text).expect("read");
        assert_eq!(read.write_json(&registry).expect("re-write"), text);
    }

    #[test]
    fn texture_starter_is_64_by_64() {
        let NodeDef::Texture(texture) = starter_def_for_kind(NodeKind::Texture) else {
            panic!("expected texture");
        };
        assert_eq!(texture.width(), 64);
        assert_eq!(texture.height(), 64);
    }

    #[test]
    fn shader_starter_references_stem_glsl_with_time_slot_and_asset() {
        let starter = starter_for_kind(NodeKind::Shader).expect("shader starter");
        let NodeDef::Shader(shader) = &starter.def else {
            panic!("expected shader");
        };
        assert_eq!(
            shader.shader_source().artifact_value().unwrap().to_string(),
            "{stem}.glsl"
        );
        let time = shader
            .consumed_slots
            .entries
            .get("time")
            .expect("time consumed slot");
        assert_eq!(*time.kind.value(), crate::ShaderSlotKind::Value);
        assert_eq!(
            time.default_bind
                .data
                .as_ref()
                .expect("default bind")
                .value()
                .to_string(),
            "bus:time"
        );
        assert_eq!(
            *shader.float_mode.value(),
            crate::nodes::shader::FloatMode::Fixed
        );
        assert_eq!(
            starter.assets,
            vec![(
                String::from("{stem}.glsl"),
                STARTER_SHADER_GLSL.as_bytes().to_vec()
            )]
        );
    }

    #[test]
    fn shader_starter_for_stem_substitutes_source_and_asset_names() {
        let starter = starter_for_kind(NodeKind::Shader)
            .expect("shader starter")
            .for_stem("pulse");
        let NodeDef::Shader(shader) = &starter.def else {
            panic!("expected shader");
        };
        assert_eq!(
            shader.shader_source().artifact_value().unwrap().to_string(),
            "pulse.glsl"
        );
        assert_eq!(starter.assets[0].0, "pulse.glsl");
        // Stem-substituted defs are what callers actually serialize.
        let registry = SlotShapeRegistry::default();
        let text = starter.def.write_json(&registry).expect("write");
        let read = NodeDef::read_json(&registry, &text).expect("read");
        assert_eq!(read.write_json(&registry).expect("re-write"), text);
    }

    #[test]
    fn fixture_starter_maps_via_a_stem_named_map2d_doc() {
        let registry = SlotShapeRegistry::default();
        let starter = starter_for_kind(NodeKind::Fixture)
            .expect("fixture starter")
            .for_stem("sign");
        let NodeDef::Fixture(fixture) = &starter.def else {
            panic!("expected fixture");
        };
        let MappingConfig::Map2d { source } = fixture.mapping.value() else {
            panic!("starter fixture mapping must be Map2d, not Unset");
        };
        assert_eq!(
            source.artifact_value().unwrap().to_string(),
            "sign.map2d.json"
        );
        assert_eq!(starter.assets[0].0, "sign.map2d.json");

        // The scaffold document itself parses and the def round-trips.
        let doc = core::str::from_utf8(&starter.assets[0].1).expect("utf8 doc");
        assert!(doc.contains("\"format\": 1"));
        let text = starter.def.write_json(&registry).expect("write fixture");
        let read = NodeDef::read_json(&registry, &text).expect("read fixture");
        assert_eq!(read.write_json(&registry).expect("re-write"), text);
    }

    #[test]
    fn bare_defaults_are_untouched_by_the_starter_table() {
        // Guard: the table must never leak into parse-time defaults.
        assert_eq!(
            NodeDef::default_for_kind(NodeKind::Shader),
            NodeDef::Shader(ShaderDef::default())
        );
        assert_eq!(
            NodeDef::default_for_kind(NodeKind::Texture),
            NodeDef::Texture(TextureDef::default())
        );
        assert_eq!(
            NodeDef::default_for_kind(NodeKind::Fixture),
            NodeDef::Fixture(FixtureDef::default())
        );
    }
}
