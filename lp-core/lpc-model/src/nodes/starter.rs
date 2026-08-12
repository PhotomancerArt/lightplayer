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
use crate::nodes::fixture::{FixtureDef, MappingConfig, PatchConfig};
use crate::nodes::shader::{ComputeShaderDef, ShaderDef, ShaderSlotDef};
use crate::nodes::texture::TextureDef;
use crate::{AssetSlot, EnumSlot, MapSlot, NodeDef, PhasorConfig, Waveform};

/// Placeholder in starter asset names and asset references. Callers substitute
/// the artifact file stem (e.g. `pulse.json` ⇒ stem `pulse`) via
/// [`NodeStarter::for_stem`] before writing files.
pub const STARTER_STEM_PLACEHOLDER: &str = "{stem}";

/// Canonical scaffold shader: the smallest animating body in the repo (red
/// pulse). It proves compile plus clock binding on first render, and it
/// teaches the phasor idiom — `phase` already wraps in `[0,1)`, so no shader
/// ever has to fold raw seconds itself.
pub const STARTER_SHADER_GLSL: &str = "layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float phase;

vec4 render_2d(vec2 pos) {
    return vec4(phase, 0.0, 0.0, 1.0);
}
";

/// Canonical scaffold compute body: one produced value tracking the clock.
/// A compute shader's bare default references a dangling `main.glsl` and
/// fails the runtime spine load outright, so the starter must scaffold a
/// real, trivially-compiling source.
pub const STARTER_COMPUTE_GLSL: &str = "void tick() { pulse = phase; }
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
        rewrite_node_def_asset_refs(&mut self.def, |current| {
            Some(current.replace(STARTER_STEM_PLACEHOLDER, stem))
        });
        self
    }
}

/// Every sibling asset reference a node def carries, in a stable order.
///
/// Plural because a fixture carries two — its mapping document and its patch
/// — and a copy that followed only the first would paste a node whose second
/// document is not there. Shares [`NodeStarter::for_stem`]'s knowledge of
/// which kinds reference assets, so a new asset-bearing kind is added in one
/// place.
pub fn node_def_asset_refs(def: &NodeDef) -> Vec<String> {
    let mut refs = Vec::new();
    match def {
        NodeDef::Shader(shader) => {
            refs.extend(shader.source.artifact_value().map(|spec| spec.to_string()));
        }
        NodeDef::ComputeShader(compute) => {
            refs.extend(compute.source.artifact_value().map(|spec| spec.to_string()));
        }
        NodeDef::Fixture(fixture) => {
            if let MappingConfig::Map2d { source } = fixture.mapping.value() {
                refs.extend(source.artifact_value().map(|spec| spec.to_string()));
            }
            if let Some(source) = fixture.patch.value().source() {
                refs.extend(source.artifact_value().map(|spec| spec.to_string()));
            }
        }
        _ => {}
    }
    refs
}

/// Re-point a node def's sibling asset references.
///
/// Used when a copied node is pasted into a project where its original
/// filenames are taken: each asset is written under a free name, and the def
/// must follow or the pasted node references files that are not there.
/// `rename` maps a current reference to its new path; `None` leaves that
/// reference alone. No-op for kinds that reference no asset.
pub fn rewrite_node_def_asset_refs(def: &mut NodeDef, rename: impl Fn(&str) -> Option<String>) {
    fn rewrite(slot: &mut AssetSlot, rename: &impl Fn(&str) -> Option<String>) {
        let Some(current) = slot.artifact_value().map(|spec| spec.to_string()) else {
            return;
        };
        if let Some(path) = rename(&current) {
            *slot = AssetSlot::path(path);
        }
    }

    match def {
        NodeDef::Shader(shader) => rewrite(&mut shader.source, &rename),
        NodeDef::ComputeShader(compute) => rewrite(&mut compute.source, &rename),
        NodeDef::Fixture(fixture) => {
            if let MappingConfig::Map2d { source } = fixture.mapping.value() {
                let mut source = source.clone();
                rewrite(&mut source, &rename);
                fixture.mapping = EnumSlot::new(MappingConfig::Map2d { source });
            }
            if let Some(source) = fixture.patch.value().source() {
                let mut source = source.clone();
                rewrite(&mut source, &rename);
                fixture.patch = EnumSlot::new(PatchConfig::File { source });
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

/// The period, in seconds, of the `phase` uniform every starter shader
/// declares. Four seconds is slow enough to read as motion rather than
/// flicker on a first render.
pub const STARTER_PHASE_PERIOD_SECONDS: f32 = 4.0;

/// The `phase` consumed slot every starter shader declares: a phasor uniform
/// riding the scope's time product, so the scaffold animates without manual
/// wiring AND teaches the idiom (wrapped cycle position, never raw seconds —
/// raw seconds overflow Q16.16 after about nine hours on device).
pub fn starter_phase_consumed_slots() -> MapSlot<String, ShaderSlotDef> {
    let mut slots = lp_collection::VecMap::new();
    slots.insert(
        String::from("phase"),
        ShaderSlotDef::phasor(
            "Phase",
            "Cycle position (0-1) over the phasor period",
            PhasorConfig {
                period_seconds: STARTER_PHASE_PERIOD_SECONDS,
                waveform: Waveform::Ramp,
                phase_offset: 0.0,
            },
        ),
    );
    MapSlot::new(slots)
}

fn starter_shader_def() -> ShaderDef {
    ShaderDef {
        source: AssetSlot::path(alloc::format!("{STARTER_STEM_PLACEHOLDER}.glsl")),
        consumed_slots: starter_phase_consumed_slots(),
        ..ShaderDef::default()
    }
}

fn starter_compute_shader_def() -> ComputeShaderDef {
    let mut produced = lp_collection::VecMap::new();
    produced.insert(
        String::from("pulse"),
        ShaderSlotDef::value_f32("Pulse", "Computed pulse level (0..1)", 0.0, None),
    );
    ComputeShaderDef {
        source: AssetSlot::path(alloc::format!("{STARTER_STEM_PLACEHOLDER}.glsl")),
        consumed_slots: starter_phase_consumed_slots(),
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
    fn compute_shader_starter_scaffolds_a_real_source_with_phase_and_pulse() {
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
        assert!(compute.consumed_slots.entries.get("phase").is_some());
        assert!(compute.produced_slots.entries.get("pulse").is_some());
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
    fn shader_starter_references_stem_glsl_with_phase_slot_and_asset() {
        let starter = starter_for_kind(NodeKind::Shader).expect("shader starter");
        let NodeDef::Shader(shader) = &starter.def else {
            panic!("expected shader");
        };
        assert_eq!(
            shader.shader_source().artifact_value().unwrap().to_string(),
            "{stem}.glsl"
        );
        let phase = shader
            .consumed_slots
            .entries
            .get("phase")
            .expect("phase consumed slot");
        // A phasor slot rides the scope's time product — it takes no
        // `bus:time` binding of its own (that channel now carries a product,
        // and an f32 uniform bound to it would warn, not animate).
        assert_eq!(*phase.kind.value(), crate::ShaderSlotKind::Phasor);
        assert!(phase.default_bind.data.is_none());
        assert_eq!(
            phase.phasor_config(),
            crate::PhasorConfig {
                period_seconds: STARTER_PHASE_PERIOD_SECONDS,
                waveform: crate::Waveform::Ramp,
                phase_offset: 0.0,
            }
        );
        // The starter authors no pin: Auto is the target's native
        // representation, which is what a new shader should get.
        assert!(shader.float_mode.is_none());
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

    /// The starter fixture is UNPATCHED: a patch is opt-in, and a scaffolded
    /// one would be a file every new fixture carries and nobody asked for.
    #[test]
    fn the_fixture_starter_ships_no_patch_document() {
        let starter = starter_for_kind(NodeKind::Fixture)
            .expect("fixture starter")
            .for_stem("sign");
        let NodeDef::Fixture(fixture) = &starter.def else {
            panic!("expected fixture");
        };
        assert_eq!(*fixture.patch.value(), PatchConfig::Unset);
        assert_eq!(starter.assets.len(), 1, "the mapping document, and only it");
    }

    /// The copy/paste pair is plural because a fixture carries two documents;
    /// following only the first pastes a node missing its patch.
    #[test]
    fn a_patched_fixtures_asset_refs_list_both_documents_and_both_rewrite() {
        let mut def = NodeDef::Fixture(FixtureDef {
            mapping: EnumSlot::new(MappingConfig::map2d("sign.map2d.json")),
            patch: EnumSlot::new(PatchConfig::file("sign.patch.json")),
            ..FixtureDef::default()
        });

        assert_eq!(
            node_def_asset_refs(&def),
            vec![
                String::from("sign.map2d.json"),
                String::from("sign.patch.json")
            ]
        );

        rewrite_node_def_asset_refs(&mut def, |current| Some(current.replace("sign", "copy_2")));
        assert_eq!(
            node_def_asset_refs(&def),
            vec![
                String::from("copy_2.map2d.json"),
                String::from("copy_2.patch.json")
            ]
        );
    }

    /// A `None` from the renamer leaves that reference standing — the caller
    /// re-homes only the assets it actually wrote under new names.
    #[test]
    fn an_unmatched_asset_reference_is_left_alone() {
        let mut def = NodeDef::Fixture(FixtureDef {
            mapping: EnumSlot::new(MappingConfig::map2d("sign.map2d.json")),
            patch: EnumSlot::new(PatchConfig::file("sign.patch.json")),
            ..FixtureDef::default()
        });

        rewrite_node_def_asset_refs(&mut def, |current| {
            (current == "sign.patch.json").then(|| String::from("other.patch.json"))
        });

        assert_eq!(
            node_def_asset_refs(&def),
            vec![
                String::from("sign.map2d.json"),
                String::from("other.patch.json")
            ]
        );
    }

    #[test]
    fn an_unpatched_fixture_lists_only_its_mapping() {
        let def = NodeDef::Fixture(FixtureDef {
            mapping: EnumSlot::new(MappingConfig::map2d("sign.map2d.json")),
            ..FixtureDef::default()
        });
        assert_eq!(
            node_def_asset_refs(&def),
            vec![String::from("sign.map2d.json")]
        );
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
