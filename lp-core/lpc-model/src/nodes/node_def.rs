//! Canonical authored node definition enum.
//!
//! This is the closed set of core node definitions understood by the current
//! LightPlayer model. Adding a core node kind should start here, then add the
//! concrete definition type and loader/runtime handling that variant requires.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::artifact::artifact_spec::ArtifactSpec;
use crate::node::kind::NodeKind;
use crate::nodes::button::ButtonDef;
use crate::nodes::clock::ClockDef;
use crate::nodes::fixture::{FixtureDef, MappingConfig};
use crate::nodes::fluid::FluidDef;
use crate::nodes::output::OutputDef;
use crate::nodes::playlist::PlaylistDef;
use crate::nodes::project::ProjectDef;
use crate::nodes::radio::ControlRadioDef;
use crate::nodes::shader::{ComputeShaderDef, ShaderDef};
use crate::nodes::texture::TextureDef;
use crate::{
    ArtifactLocation, AssetContentType, AssetLocation, AssetSlot, AssetSlotValue, EnumSlot, LpPath,
    LpPathBuf, NodeInvocation, ProjectNodePlacement, ReferencedAsset, SlotAccess, SlotDataAccess,
    SlotDataMutAccess, SlotMapKey, SlotMutAccess, SlotName, SlotPath, SlotShapeId,
    SlotShapeRegistry, Slotted, StaticSlotShape,
};

const PROJECT_VARIANT: &str = "Project";
const BUTTON_VARIANT: &str = "Button";
const CLOCK_VARIANT: &str = "Clock";
const TEXTURE_VARIANT: &str = "Texture";
const SHADER_VARIANT: &str = "Shader";
const COMPUTE_SHADER_VARIANT: &str = "ComputeShader";
const FLUID_VARIANT: &str = "Fluid";
const PLAYLIST_VARIANT: &str = "Playlist";
const CONTROL_RADIO_VARIANT: &str = "ControlRadio";
const OUTPUT_VARIANT: &str = "Output";
const FIXTURE_VARIANT: &str = "Fixture";
const NODE_DEF_VARIANT_NAMES: &[&str] = &[
    PROJECT_VARIANT,
    BUTTON_VARIANT,
    CLOCK_VARIANT,
    TEXTURE_VARIANT,
    SHADER_VARIANT,
    COMPUTE_SHADER_VARIANT,
    FLUID_VARIANT,
    PLAYLIST_VARIANT,
    CONTROL_RADIO_VARIANT,
    OUTPUT_VARIANT,
    FIXTURE_VARIANT,
];

/// Authored body of a node artifact.
///
/// A `NodeDef` is source data: it is what a JSON artifact defines before the
/// engine instantiates a runtime node. Project artifacts are included because
/// a project defines the root project node and its child node invocations.
#[derive(Clone, Debug, PartialEq, Slotted)]
pub enum NodeDef {
    #[default]
    Project(ProjectDef),
    Button(ButtonDef),
    Clock(ClockDef),
    Texture(TextureDef),
    Shader(ShaderDef),
    ComputeShader(ComputeShaderDef),
    Fluid(FluidDef),
    Playlist(PlaylistDef),
    ControlRadio(ControlRadioDef),
    Output(OutputDef),
    Fixture(FixtureDef),
}

/// Slot-owned authored node artifact root.
///
/// The wrapper gives an artifact its own shape id and factory while exposing
/// the active [`NodeDef`] shape directly. Paths start at the node definition
/// payload; there is no synthetic wrapper field.
#[derive(Clone, Debug, Default, PartialEq, Slotted)]
pub struct NodeArtifact(pub EnumSlot<NodeDef>);

/// One child node invocation and its path within the owning artifact.
#[derive(Clone, Debug, PartialEq)]
pub struct InvocationSite {
    pub path: SlotPath,
    pub invocation: NodeInvocation,
    pub role: ProjectNodePlacement,
}

/// Failure resolving model-authored artifact path references.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArtifactPathResolutionError {
    LibUnsupported { specifier: String },
    RelativePath { path: String, base_dir: String },
}

impl core::fmt::Display for ArtifactPathResolutionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::LibUnsupported { specifier } => {
                write!(
                    f,
                    "library artifact specifiers are not supported: {specifier}"
                )
            }
            Self::RelativePath { path, base_dir } => {
                write!(
                    f,
                    "path `{path}` cannot be resolved relative to `{base_dir}`"
                )
            }
        }
    }
}

impl NodeArtifact {
    pub fn new(def: NodeDef) -> Self {
        Self(EnumSlot::new(def))
    }

    pub fn node_def(&self) -> &NodeDef {
        self.0.value()
    }

    pub fn into_node_def(self) -> NodeDef {
        self.0.into_inner()
    }

    /// Read an authored JSON node artifact through the slot registry.
    ///
    /// The codec streams, so the top-level `"kind"` field must precede the
    /// variant's other fields — canonical [`Self::write_json`] output always
    /// satisfies this.
    pub fn read_json(registry: &SlotShapeRegistry, text: &str) -> Result<Self, NodeDefParseError> {
        reject_unknown_kind_json(text)?;
        let object = registry
            .read_slot_json(NodeArtifact::SHAPE_ID, text)
            .map_err(|error| NodeDefParseError::Syntax {
                error: error.to_string(),
            })?;
        downcast_node_artifact(object)
    }

    /// Write this artifact as authored JSON: pretty-printed, slot-shape
    /// declaration order, trailing newline. Output is deterministic so
    /// identical models produce byte-identical files.
    pub fn write_json(&self, registry: &SlotShapeRegistry) -> Result<String, NodeDefWriteError> {
        let mut out = registry
            .write_slot_json_pretty(self, alloc::vec::Vec::new())
            .map_err(|error| NodeDefWriteError {
                error: error.to_string(),
            })?;
        out.push(b'\n');
        String::from_utf8(out).map_err(|_| NodeDefWriteError {
            error: String::from("slot JSON writer produced invalid UTF-8"),
        })
    }
}

impl NodeDef {
    /// Default-authored node definition for a kind.
    pub fn default_for_kind(kind: NodeKind) -> Self {
        match kind {
            NodeKind::Project => Self::Project(ProjectDef::default()),
            NodeKind::Button => Self::Button(ButtonDef::default()),
            NodeKind::Clock => Self::Clock(ClockDef::default()),
            NodeKind::Texture => Self::Texture(TextureDef::default()),
            NodeKind::Shader => Self::Shader(ShaderDef::default()),
            NodeKind::ComputeShader => Self::ComputeShader(ComputeShaderDef::default()),
            NodeKind::Fluid => Self::Fluid(FluidDef::default()),
            NodeKind::Playlist => Self::Playlist(PlaylistDef::default()),
            NodeKind::ControlRadio => Self::ControlRadio(ControlRadioDef::default()),
            NodeKind::Output => Self::Output(OutputDef::default()),
            NodeKind::Fixture => Self::Fixture(FixtureDef::default()),
        }
    }

    /// Core node kind for this definition.
    pub fn kind(&self) -> NodeKind {
        match self {
            Self::Project(_) => NodeKind::Project,
            Self::Button(_) => NodeKind::Button,
            Self::Clock(_) => NodeKind::Clock,
            Self::Texture(_) => NodeKind::Texture,
            Self::Shader(_) => NodeKind::Shader,
            Self::ComputeShader(_) => NodeKind::ComputeShader,
            Self::Fluid(_) => NodeKind::Fluid,
            Self::Playlist(_) => NodeKind::Playlist,
            Self::ControlRadio(_) => NodeKind::ControlRadio,
            Self::Output(_) => NodeKind::Output,
            Self::Fixture(_) => NodeKind::Fixture,
        }
    }

    /// Stable authored `kind` string used in TOML and diagnostics.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Project(_) => ProjectDef::KIND,
            Self::Button(_) => ButtonDef::KIND,
            Self::Clock(_) => ClockDef::KIND,
            Self::Texture(_) => TextureDef::KIND,
            Self::Shader(_) => ShaderDef::KIND,
            Self::ComputeShader(_) => ComputeShaderDef::KIND,
            Self::Fluid(_) => FluidDef::KIND,
            Self::Playlist(_) => PlaylistDef::KIND,
            Self::ControlRadio(_) => ControlRadioDef::KIND,
            Self::Output(_) => OutputDef::KIND,
            Self::Fixture(_) => FixtureDef::KIND,
        }
    }

    /// Slot enum discriminator used by authored TOML.
    pub fn variant_name(&self) -> &'static str {
        match self {
            Self::Project(_) => PROJECT_VARIANT,
            Self::Button(_) => BUTTON_VARIANT,
            Self::Clock(_) => CLOCK_VARIANT,
            Self::Texture(_) => TEXTURE_VARIANT,
            Self::Shader(_) => SHADER_VARIANT,
            Self::ComputeShader(_) => COMPUTE_SHADER_VARIANT,
            Self::Fluid(_) => FLUID_VARIANT,
            Self::Playlist(_) => PLAYLIST_VARIANT,
            Self::ControlRadio(_) => CONTROL_RADIO_VARIANT,
            Self::Output(_) => OUTPUT_VARIANT,
            Self::Fixture(_) => FIXTURE_VARIANT,
        }
    }

    pub fn is_variant_name(name: &str) -> bool {
        NODE_DEF_VARIANT_NAMES.contains(&name)
    }

    /// Child invocation slots reachable directly from this definition.
    ///
    /// Definitions live one-per-artifact, so site paths are rooted at the
    /// artifact root.
    pub fn invocation_sites(&self) -> Vec<InvocationSite> {
        let base = SlotPath::root();
        let base = &base;
        match self {
            Self::Project(project) => project
                .nodes
                .entries
                .iter()
                .filter_map(|(name, invocation)| {
                    Some(InvocationSite {
                        path: project_node_path(base, name)?,
                        invocation: invocation.value().clone(),
                        role: ProjectNodePlacement::ProjectChild { name: name.clone() },
                    })
                })
                .collect(),
            Self::Playlist(playlist) => playlist
                .entries
                .entries
                .iter()
                .filter_map(|(key, entry)| {
                    Some(InvocationSite {
                        path: playlist_entry_node_path(base, *key)?,
                        invocation: entry.node.value().clone(),
                        role: ProjectNodePlacement::PlaylistEntry {
                            entry: *key,
                            name: entry.name.data.as_ref().map(|name| name.value().clone()),
                        },
                    })
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// File-backed asset paths referenced by this definition.
    pub fn referenced_asset_paths(
        &self,
        containing_file: &LpPath,
    ) -> Result<Vec<LpPathBuf>, ArtifactPathResolutionError> {
        let mut paths = Vec::new();
        for asset in self.referenced_assets(containing_file)? {
            let AssetLocation::Artifact { location } = asset.location;
            paths.push(location.file_path().clone());
        }
        Ok(paths)
    }

    /// Assets referenced by this definition.
    pub fn referenced_assets(
        &self,
        containing_file: &LpPath,
    ) -> Result<Vec<ReferencedAsset>, ArtifactPathResolutionError> {
        match self {
            Self::Shader(shader) => assets_for_slot(
                shader.shader_source(),
                containing_file,
                AssetContentType::ShaderSource,
            ),
            Self::ComputeShader(shader) => assets_for_slot(
                shader.shader_source(),
                containing_file,
                AssetContentType::ComputeShaderSource,
            ),
            Self::Fixture(fixture) => assets_for_fixture(fixture, containing_file),
            _ => Ok(Vec::new()),
        }
    }

    /// True when full authored bodies differ.
    pub fn body_changed(before: &Self, after: &Self) -> bool {
        before != after
    }

    /// True when parent-facing shell views differ.
    ///
    /// With strictly one node definition per artifact (no inline child
    /// bodies), the parent-facing shell is the full authored body, so this is
    /// equivalent to [`Self::body_changed`].
    pub fn shell_changed(before: &Self, after: &Self) -> bool {
        Self::body_changed(before, after)
    }

    pub fn as_project(&self) -> Option<&ProjectDef> {
        match self {
            Self::Project(def) => Some(def),
            _ => None,
        }
    }

    pub fn as_texture(&self) -> Option<&TextureDef> {
        match self {
            Self::Texture(def) => Some(def),
            _ => None,
        }
    }

    pub fn as_button(&self) -> Option<&ButtonDef> {
        match self {
            Self::Button(def) => Some(def),
            _ => None,
        }
    }

    pub fn as_clock(&self) -> Option<&ClockDef> {
        match self {
            Self::Clock(def) => Some(def),
            _ => None,
        }
    }

    pub fn as_shader(&self) -> Option<&ShaderDef> {
        match self {
            Self::Shader(def) => Some(def),
            _ => None,
        }
    }

    pub fn as_compute_shader(&self) -> Option<&ComputeShaderDef> {
        match self {
            Self::ComputeShader(def) => Some(def),
            _ => None,
        }
    }

    pub fn as_fluid(&self) -> Option<&FluidDef> {
        match self {
            Self::Fluid(def) => Some(def),
            _ => None,
        }
    }

    pub fn as_playlist(&self) -> Option<&PlaylistDef> {
        match self {
            Self::Playlist(def) => Some(def),
            _ => None,
        }
    }

    pub fn as_control_radio(&self) -> Option<&ControlRadioDef> {
        match self {
            Self::ControlRadio(def) => Some(def),
            _ => None,
        }
    }

    pub fn as_output(&self) -> Option<&OutputDef> {
        match self {
            Self::Output(def) => Some(def),
            _ => None,
        }
    }

    pub fn as_fixture(&self) -> Option<&FixtureDef> {
        match self {
            Self::Fixture(def) => Some(def),
            _ => None,
        }
    }

    /// Read an authored JSON node artifact through the slot registry.
    pub fn read_json(registry: &SlotShapeRegistry, text: &str) -> Result<Self, NodeDefParseError> {
        NodeArtifact::read_json(registry, text).map(NodeArtifact::into_node_def)
    }

    /// Read authored JSON using the model crate's generated static shape registry.
    pub fn from_json_str(text: &str) -> Result<Self, NodeDefParseError> {
        let registry = SlotShapeRegistry::default();
        Self::read_json(&registry, text)
    }

    /// Write this node definition as authored JSON through the slot registry.
    pub fn write_json(&self, registry: &SlotShapeRegistry) -> Result<String, NodeDefWriteError> {
        NodeArtifact::new(self.clone()).write_json(registry)
    }
}

fn project_node_path(base: &SlotPath, name: &str) -> Option<SlotPath> {
    let nodes = SlotName::parse("nodes").ok()?;
    let key = SlotMapKey::String(String::from(name));
    Some(base.child(nodes).child_key(key))
}

fn playlist_entry_node_path(base: &SlotPath, key: u32) -> Option<SlotPath> {
    let entries = SlotName::parse("entries").ok()?;
    let node = SlotName::parse("node").ok()?;
    Some(
        base.child(entries)
            .child_key(SlotMapKey::U32(key))
            .child(node),
    )
}

fn assets_for_fixture(
    fixture: &FixtureDef,
    containing_file: &LpPath,
) -> Result<Vec<ReferencedAsset>, ArtifactPathResolutionError> {
    let MappingConfig::SvgPath { source, .. } = fixture.mapping.value() else {
        return Ok(Vec::new());
    };
    assets_for_slot(source, containing_file, AssetContentType::FixtureSvg)
}

fn assets_for_slot(
    slot: &AssetSlot,
    containing_file: &LpPath,
    content_type: AssetContentType,
) -> Result<Vec<ReferencedAsset>, ArtifactPathResolutionError> {
    match slot.value() {
        AssetSlotValue::Artifact(specifier) => {
            let location =
                ArtifactLocation::file(resolve_artifact_specifier(containing_file, specifier)?);
            Ok(vec![ReferencedAsset::new(
                AssetLocation::artifact(location),
                content_type,
            )])
        }
    }
}

pub fn resolve_artifact_specifier(
    containing_file: &LpPath,
    specifier: &ArtifactSpec,
) -> Result<LpPathBuf, ArtifactPathResolutionError> {
    let base_dir = containing_file.parent().unwrap_or_else(|| LpPath::new("/"));
    match specifier {
        ArtifactSpec::Path(path) => {
            if path.is_absolute() {
                Ok(path.clone())
            } else {
                base_dir
                    .to_path_buf()
                    .join_relative(path.as_str())
                    .ok_or_else(|| ArtifactPathResolutionError::RelativePath {
                        path: String::from(path.as_str()),
                        base_dir: String::from(base_dir.as_str()),
                    })
            }
        }
        ArtifactSpec::Lib(lib) => Err(ArtifactPathResolutionError::LibUnsupported {
            specifier: lib.to_string(),
        }),
    }
}

impl SlotAccess for NodeDef {
    fn shape_id(&self) -> SlotShapeId {
        match self {
            Self::Project(def) => def.shape_id(),
            Self::Button(def) => def.shape_id(),
            Self::Clock(def) => def.shape_id(),
            Self::Texture(def) => def.shape_id(),
            Self::Shader(def) => def.shape_id(),
            Self::ComputeShader(def) => def.shape_id(),
            Self::Fluid(def) => def.shape_id(),
            Self::Playlist(def) => def.shape_id(),
            Self::ControlRadio(def) => def.shape_id(),
            Self::Output(def) => def.shape_id(),
            Self::Fixture(def) => def.shape_id(),
        }
    }

    fn data(&self) -> SlotDataAccess<'_> {
        match self {
            Self::Project(def) => def.data(),
            Self::Button(def) => def.data(),
            Self::Clock(def) => def.data(),
            Self::Texture(def) => def.data(),
            Self::Shader(def) => def.data(),
            Self::ComputeShader(def) => def.data(),
            Self::Fluid(def) => def.data(),
            Self::Playlist(def) => def.data(),
            Self::ControlRadio(def) => def.data(),
            Self::Output(def) => def.data(),
            Self::Fixture(def) => def.data(),
        }
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn core::any::Any> {
        self
    }
}

impl SlotMutAccess for NodeDef {
    fn data_mut(&mut self) -> SlotDataMutAccess<'_> {
        match self {
            Self::Project(def) => def.data_mut(),
            Self::Button(def) => def.data_mut(),
            Self::Clock(def) => def.data_mut(),
            Self::Texture(def) => def.data_mut(),
            Self::Shader(def) => def.data_mut(),
            Self::ComputeShader(def) => def.data_mut(),
            Self::Fluid(def) => def.data_mut(),
            Self::Playlist(def) => def.data_mut(),
            Self::ControlRadio(def) => def.data_mut(),
            Self::Output(def) => def.data_mut(),
            Self::Fixture(def) => def.data_mut(),
        }
    }
}

/// Failure parsing an authored node definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeDefParseError {
    UnknownKind { kind: String },
    Syntax { error: String },
}

impl core::fmt::Display for NodeDefParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownKind { kind } => write!(f, "unknown node kind `{kind}`"),
            Self::Syntax { error } => f.write_str(error),
        }
    }
}

/// Failure writing an authored node definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeDefWriteError {
    error: String,
}

impl core::fmt::Display for NodeDefWriteError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.error)
    }
}

impl core::error::Error for NodeDefWriteError {}

fn downcast_node_artifact(
    object: alloc::boxed::Box<dyn crate::SlotMutAccess>,
) -> Result<NodeArtifact, NodeDefParseError> {
    object
        .into_any()
        .downcast::<NodeArtifact>()
        .map(|artifact| *artifact)
        .map_err(|_| NodeDefParseError::Syntax {
            error: format!(
                "slot reader returned unexpected type for shape {}",
                NodeArtifact::SHAPE_ID
            ),
        })
}

fn reject_unknown_kind_json(text: &str) -> Result<(), NodeDefParseError> {
    let kind = read_kind_json(text)?;
    if NODE_DEF_VARIANT_NAMES.contains(&kind.as_str()) {
        Ok(())
    } else {
        Err(NodeDefParseError::UnknownKind { kind })
    }
}

/// Streaming probe for the top-level `"kind"` string in an authored JSON
/// artifact. Uses syntax events so device loads never materialize a value
/// tree just to pre-check the kind.
fn read_kind_json(text: &str) -> Result<String, NodeDefParseError> {
    use crate::slot_codec::{JsonSyntaxSource, SyntaxEvent, SyntaxEventSource};

    let syntax_error = |error: crate::slot_codec::SyntaxError| NodeDefParseError::Syntax {
        error: error.to_string(),
    };

    let mut source = JsonSyntaxSource::new(text).map_err(syntax_error)?;
    match source.next_event().map_err(syntax_error)? {
        Some(SyntaxEvent::StartObject { .. }) => {}
        _ => {
            return Err(NodeDefParseError::Syntax {
                error: String::from("node definition JSON root must be an object"),
            });
        }
    }

    // Scan top-level props, skipping nested values by depth.
    let mut depth = 0usize;
    loop {
        let Some(event) = source.next_event().map_err(syntax_error)? else {
            return Err(NodeDefParseError::Syntax {
                error: String::from("missing required field `kind`"),
            });
        };
        match event {
            SyntaxEvent::Prop { name, .. } if depth == 0 && name == "kind" => {
                let mut kind = String::new();
                loop {
                    match source.next_event().map_err(syntax_error)? {
                        Some(SyntaxEvent::StringChunk { text, is_last, .. }) => {
                            kind.push_str(&text);
                            if is_last {
                                return Ok(kind);
                            }
                        }
                        _ => {
                            return Err(NodeDefParseError::Syntax {
                                error: String::from("field `kind` must be a string"),
                            });
                        }
                    }
                }
            }
            SyntaxEvent::StartObject { .. } | SyntaxEvent::StartArray { .. } => depth += 1,
            SyntaxEvent::EndArray { .. } => depth = depth.saturating_sub(1),
            SyntaxEvent::EndObject { .. } => {
                if depth == 0 {
                    return Err(NodeDefParseError::Syntax {
                        error: String::from("missing required field `kind`"),
                    });
                }
                depth -= 1;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    use crate::{BindingRef, LpValue, MappingConfig, PathSpec, SlotShapeRegistry, TextureDef};

    #[test]
    fn node_def_delegates_kind_and_slots() {
        let def = NodeDef::Texture(TextureDef::new(64, 48));

        assert_eq!(def.kind(), NodeKind::Texture);
        assert_eq!(def.kind_name(), "texture");
        assert_eq!(def.variant_name(), "Texture");
        assert_eq!(def.shape_id(), TextureDef::SHAPE_ID);
    }

    #[test]
    fn node_def_parses_output_and_fixture_json() {
        let registry = registry();

        let output = NodeDef::read_json(
            &registry,
            r#"{
  "kind": "Output",
  "endpoint": "ws281x:rmt:D10",
  "options": { "brightness": 0.5 }
}"#,
        )
        .expect("output");
        assert!(matches!(output, NodeDef::Output(_)));

        let fixture = NodeDef::read_json(
            &registry,
            r#"{
  "kind": "Fixture",
  "render_size": { "width": 8, "height": 8 },
  "mapping": { "kind": "PathPoints" }
}"#,
        )
        .expect("fixture");
        let NodeDef::Fixture(fixture) = fixture else {
            panic!("expected fixture");
        };
        assert!(matches!(
            fixture.mapping.value(),
            MappingConfig::PathPoints { .. }
        ));

        let fixture = NodeDef::read_json(
            &registry,
            r#"{
  "kind": "Fixture",
  "render_size": { "width": 64, "height": 16 },
  "mapping": {
    "kind": "SvgPath",
    "source": "./fyeah-mapping.svg",
    "sample_diameter": 2.0
  }
}"#,
        )
        .expect("svg path fixture");
        let NodeDef::Fixture(fixture) = fixture else {
            panic!("expected fixture");
        };
        let MappingConfig::SvgPath {
            source,
            sample_diameter,
        } = fixture.mapping.value()
        else {
            panic!("expected SvgPath mapping");
        };
        assert_eq!(
            source.artifact_value().unwrap().to_string(),
            "fyeah-mapping.svg"
        );
        assert_eq!(sample_diameter.value().0, 2.0);
    }

    #[test]
    fn node_def_round_trips_point_list_fixture_json() {
        let registry = registry();
        let fixture = crate::FixtureDef {
            mapping: EnumSlot::new(MappingConfig::path_points_vec(
                alloc::vec![PathSpec::point_list(
                    3,
                    alloc::vec![[0.0, 0.25], [1.0, 0.75]],
                )],
                2.0,
            )),
            ..crate::FixtureDef::default()
        };
        let text = NodeDef::Fixture(fixture)
            .write_json(&registry)
            .expect("write fixture");
        let read = NodeDef::read_json(&registry, &text).expect("read fixture");
        let NodeDef::Fixture(read) = read else {
            panic!("expected fixture");
        };
        let MappingConfig::PathPoints { paths, .. } = read.mapping.value() else {
            panic!("expected PathPoints");
        };
        let PathSpec::PointList {
            first_channel,
            points,
        } = paths.entries.get(&0).expect("path").value()
        else {
            panic!("expected PointList");
        };
        assert_eq!(*first_channel.value(), 3);
        assert_eq!(
            points.entries.get(&0).expect("point").value().0,
            [0.0, 0.25]
        );
        assert_eq!(
            points.entries.get(&1).expect("point").value().0,
            [1.0, 0.75]
        );
    }

    #[test]
    fn node_artifact_root_loads_through_wrapper_shape() {
        let registry = registry();

        let artifact = registry
            .read_slot_json(
                NodeArtifact::SHAPE_ID,
                r#"{ "kind": "Texture", "size": { "width": 1, "height": 2 } }"#,
            )
            .expect("artifact slot load")
            .into_any()
            .downcast::<NodeArtifact>()
            .expect("node artifact");

        assert_eq!(artifact.shape_id(), NodeArtifact::SHAPE_ID);
        let SlotDataAccess::Enum(en) = artifact.data() else {
            panic!("artifact wrapper should expose node enum data");
        };
        assert_eq!(en.variant(), "Texture");
        let NodeDef::Texture(def) = artifact.node_def() else {
            panic!("expected texture");
        };
        assert_eq!(def.size.value().width, 1);
        assert_eq!(def.size.value().height, 2);
    }

    #[test]
    fn node_def_parses_project_and_texture_json() {
        let registry = registry();
        let project = NodeDef::read_json(
            &registry,
            r#"{
  "kind": "Project",
  "nodes": {
    "texture": { "ref": "./texture.json" }
  }
}"#,
        )
        .expect("project");
        assert!(matches!(project, NodeDef::Project(_)));

        let texture = NodeDef::read_json(
            &registry,
            r#"{ "kind": "Texture", "size": { "width": 64, "height": 48 } }"#,
        )
        .expect("texture");
        let NodeDef::Texture(def) = texture else {
            panic!("expected texture");
        };
        assert_eq!(def.size.value().width, 64);
        assert_eq!(def.size.value().height, 48);
    }

    #[test]
    fn node_def_parses_shader_json_with_bindings() {
        let registry = registry();
        let shader = NodeDef::read_json(
            &registry,
            r#"{
  "kind": "Shader",
  "render_order": 2,
  "source": { "path": "shader.glsl" },
  "bindings": { "visual": { "target": "bus#visual.out" } }
}"#,
        )
        .expect("shader");
        assert!(matches!(shader, NodeDef::Shader(_)));
    }

    #[test]
    fn node_def_json_rejects_missing_invalid_and_unknown_kind() {
        let registry = registry();

        let missing =
            NodeDef::read_json(&registry, r#"{ "name": "missing" }"#).expect_err("missing kind");
        assert!(missing.to_string().contains("kind"));

        let invalid = NodeDef::read_json(&registry, r#"{ "kind": 7 }"#).expect_err("invalid kind");
        assert!(invalid.to_string().contains("string"));

        let not_object = NodeDef::read_json(&registry, r#"[1, 2]"#).expect_err("array root");
        assert!(not_object.to_string().contains("object"));

        let unknown =
            NodeDef::read_json(&registry, r#"{ "kind": "bogus" }"#).expect_err("unknown kind");
        assert_eq!(
            unknown,
            NodeDefParseError::UnknownKind {
                kind: String::from("bogus")
            }
        );
    }

    #[test]
    fn node_def_json_kind_probe_skips_nested_objects() {
        let registry = registry();

        // A nested "kind" key must not satisfy the top-level probe: this
        // should report the missing top-level kind, not UnknownKind(Bogus).
        let err = NodeDef::read_json(&registry, r#"{ "mapping": { "kind": "Bogus" } }"#)
            .expect_err("missing top-level kind");
        assert!(err.to_string().contains("missing required field"), "{err}");

        // Nested kinds after the top-level one are fine.
        let fixture = NodeDef::read_json(
            &registry,
            r#"{
  "kind": "Fixture",
  "render_size": { "width": 8, "height": 8 },
  "mapping": { "kind": "PathPoints" }
}"#,
        )
        .expect("fixture");
        assert!(matches!(fixture, NodeDef::Fixture(_)));
    }

    #[test]
    fn node_def_writes_pretty_authored_json() {
        let registry = registry();
        let text = NodeDef::Texture(TextureDef::new(3, 4))
            .write_json(&registry)
            .expect("write texture");

        assert!(text.starts_with("{\n  \"kind\": \"Texture\""), "{text}");
        assert!(text.ends_with("}\n"), "{text}");
        assert!(text.contains("\"width\": 3"), "{text}");
        assert!(text.contains("\"height\": 4"), "{text}");

        let read = NodeDef::read_json(&registry, &text).expect("read texture");
        let NodeDef::Texture(def) = read else {
            panic!("expected texture");
        };
        assert_eq!(def.size.value().width, 3);
        assert_eq!(def.size.value().height, 4);
    }

    #[test]
    fn node_def_json_round_trip_is_byte_stable() {
        let registry = registry();
        let fixture = crate::FixtureDef {
            mapping: EnumSlot::new(MappingConfig::path_points_vec(
                alloc::vec![PathSpec::point_list(
                    3,
                    alloc::vec![[0.0, 0.25], [1.0, 0.75]],
                )],
                2.0,
            )),
            ..crate::FixtureDef::default()
        };
        let first = NodeDef::Fixture(fixture)
            .write_json(&registry)
            .expect("write fixture");
        let read = NodeDef::read_json(&registry, &first).expect("read fixture");
        let second = read.write_json(&registry).expect("re-write fixture");
        assert_eq!(first, second);
    }

    #[test]
    fn node_def_reads_binding_values_and_refs() {
        let registry = registry();

        let def = NodeDef::read_json(
            &registry,
            r##"{
  "kind": "Output",
  "endpoint": "ws281x:rmt:D10",
  "bindings": { "main": { "value": 0.25 } }
}"##,
        )
        .expect("output");
        let NodeDef::Output(def) = def else {
            panic!("expected output");
        };
        let binding = def.bindings.0.entries.get("main").expect("binding");
        assert_eq!(binding.value_literal(), Some(&LpValue::F32(0.25)));

        let def = NodeDef::read_json(
            &registry,
            r##"{
  "kind": "Output",
  "endpoint": "ws281x:rmt:D10",
  "bindings": { "main": { "target": "bus#control.out" } }
}"##,
        )
        .expect("output target");
        let NodeDef::Output(def) = def else {
            panic!("expected output");
        };
        let binding = def.bindings.0.entries.get("main").expect("binding");
        assert!(matches!(binding.target_ref(), Some(BindingRef::Bus(_))));
    }

    #[test]
    fn node_def_invocation_sites_cover_project_and_playlist() {
        let project = NodeDef::from_json_str(
            r#"{
  "kind": "Project",
  "nodes": {
    "clock": { "ref": "./clock.json" }
  }
}"#,
        )
        .expect("project");
        let sites = project.invocation_sites();
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].path.to_string(), "nodes[clock]");
        assert!(matches!(sites[0].invocation, NodeInvocation::Ref(_)));

        let playlist = NodeDef::from_json_str(
            r#"{
  "kind": "Playlist",
  "entries": {
    "2": {
      "name": "active",
      "node": { "ref": "./active.json" }
    }
  }
}"#,
        )
        .expect("playlist");
        let sites = playlist.invocation_sites();
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].path.to_string(), "entries[2].node");
        assert!(matches!(sites[0].invocation, NodeInvocation::Ref(_)));
    }

    #[test]
    fn node_def_referenced_asset_paths_resolve_relative_shader_compute_and_fixture_paths() {
        let shader =
            NodeDef::from_json_str(r#"{ "kind": "Shader", "source": { "path": "shader.glsl" } }"#)
                .expect("shader");
        assert_eq!(
            shader
                .referenced_asset_paths(LpPath::new("/nodes/shader.json"))
                .unwrap(),
            vec![LpPathBuf::from("/nodes/shader.glsl")]
        );

        let compute = NodeDef::from_json_str(
            r#"{ "kind": "ComputeShader", "source": { "path": "../compute.glsl" } }"#,
        )
        .expect("compute");
        assert_eq!(
            compute
                .referenced_asset_paths(LpPath::new("/nodes/compute.json"))
                .unwrap(),
            vec![LpPathBuf::from("/compute.glsl")]
        );

        let fixture = NodeDef::from_json_str(
            r#"{
  "kind": "Fixture",
  "render_size": { "width": 64, "height": 16 },
  "mapping": {
    "kind": "SvgPath",
    "source": "fixture.svg",
    "sample_diameter": 2.0
  }
}"#,
        )
        .expect("fixture");
        assert_eq!(
            fixture
                .referenced_asset_paths(LpPath::new("/fixtures/fixture.json"))
                .unwrap(),
            vec![LpPathBuf::from("/fixtures/fixture.svg")]
        );
    }

    #[test]
    fn node_def_rejects_inline_asset_bodies() {
        let err = NodeDef::from_json_str(
            r#"{ "kind": "Shader", "source": { "glsl": "void main() {}" } }"#,
        )
        .expect_err("inline asset body must be rejected");
        assert!(err.to_string().contains("inline asset"), "{err}");
    }

    #[test]
    fn node_def_referenced_assets_include_source_identity_and_kind() {
        let fixture = NodeDef::from_json_str(
            r#"{
  "kind": "Fixture",
  "render_size": { "width": 64, "height": 16 },
  "mapping": {
    "kind": "SvgPath",
    "source": "fixture.svg",
    "sample_diameter": 2.0
  }
}"#,
        )
        .expect("fixture");

        assert_eq!(
            fixture
                .referenced_assets(LpPath::new("/fixtures/f.json"))
                .unwrap(),
            vec![ReferencedAsset::new(
                AssetLocation::artifact(ArtifactLocation::file("/fixtures/fixture.svg")),
                AssetContentType::FixtureSvg,
            )]
        );
    }

    #[test]
    fn node_def_rejects_inline_child_definitions() {
        let err = NodeDef::from_json_str(
            r#"{
  "kind": "Playlist",
  "entries": {
    "2": { "node": { "def": { "kind": "Clock" } } }
  }
}"#,
        )
        .expect_err("inline child definition must be rejected");
        assert!(err.to_string().contains("def"), "{err}");
    }

    #[test]
    fn node_def_shell_change_tracks_child_ref_changes() {
        let before = NodeDef::from_json_str(
            r#"{ "kind": "Project", "nodes": { "a": { "ref": "./a.json" } } }"#,
        )
        .expect("before");
        let ref_changed = NodeDef::from_json_str(
            r#"{ "kind": "Project", "nodes": { "a": { "ref": "./b.json" } } }"#,
        )
        .expect("ref changed");

        assert!(NodeDef::body_changed(&before, &ref_changed));
        assert!(NodeDef::shell_changed(&before, &ref_changed));
        assert!(!NodeDef::shell_changed(&before, &before.clone()));
    }

    #[test]
    fn node_def_default_for_kind_covers_every_kind() {
        for kind in [
            NodeKind::Project,
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
        ] {
            assert_eq!(NodeDef::default_for_kind(kind).kind(), kind);
        }
    }

    fn registry() -> SlotShapeRegistry {
        SlotShapeRegistry::default()
    }
}
