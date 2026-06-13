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
    LpPathBuf, NodeDefLocation, NodeInvocation, ProjectNodePlacement, ReferencedAsset, SlotAccess,
    SlotDataAccess, SlotDataMutAccess, SlotMapKey, SlotMutAccess, SlotName, SlotPath, SlotShapeId,
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
/// A `NodeDef` is source data: it is what a TOML artifact defines before the
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

/// Borrowed inline text asset body owned by a node definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InlineAssetText<'a> {
    pub extension: &'a str,
    pub text: &'a str,
}

/// Borrowed inline byte asset body owned by a node definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InlineAssetBytes<'a> {
    pub extension: Option<&'a str>,
    pub bytes: &'a [u8],
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

    /// Read an authored TOML node artifact through the slot registry.
    pub fn read_toml(registry: &SlotShapeRegistry, text: &str) -> Result<Self, NodeDefParseError> {
        let payload = toml::from_str::<toml::Value>(text).map_err(toml_parse_error)?;
        reject_unknown_kind(&payload)?;
        read_node_artifact(registry, payload)
    }

    /// Write an authored TOML node artifact through the slot registry.
    pub fn write_toml(&self, registry: &SlotShapeRegistry) -> Result<String, NodeDefWriteError> {
        write_node_artifact(registry, self)
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

    /// Child invocation slots reachable directly from this definition under `base`.
    pub fn invocation_sites(&self, base: &SlotPath) -> Vec<InvocationSite> {
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
        let owner =
            NodeDefLocation::artifact_root(ArtifactLocation::location_for_path(containing_file));
        let mut paths = Vec::new();
        for asset in self.referenced_assets(containing_file, &owner, &SlotPath::root())? {
            if let AssetLocation::Artifact { location } = asset.location {
                paths.push(location.file_path().clone());
            }
        }
        Ok(paths)
    }

    /// Assets referenced by this definition.
    pub fn referenced_assets(
        &self,
        containing_file: &LpPath,
        owner: &NodeDefLocation,
        base: &SlotPath,
    ) -> Result<Vec<ReferencedAsset>, ArtifactPathResolutionError> {
        match self {
            Self::Shader(shader) => assets_for_shader(
                shader.shader_source(),
                containing_file,
                owner,
                base,
                AssetContentType::ShaderSource,
            ),
            Self::ComputeShader(shader) => assets_for_shader(
                shader.shader_source(),
                containing_file,
                owner,
                base,
                AssetContentType::ComputeShaderSource,
            ),
            Self::Fixture(fixture) => assets_for_fixture(fixture, containing_file, owner, base),
            _ => Ok(Vec::new()),
        }
    }

    /// Inline UTF-8 asset text at `asset_path`, when this definition owns one.
    pub fn inline_asset_text(
        &self,
        owner_path: &SlotPath,
        asset_path: &SlotPath,
    ) -> Option<InlineAssetText<'_>> {
        match self {
            Self::Shader(shader) if asset_path == &source_slot_path(owner_path) => {
                inline_text_from_slot(shader.shader_source(), "glsl")
            }
            Self::ComputeShader(shader) if asset_path == &source_slot_path(owner_path) => {
                inline_text_from_slot(shader.shader_source(), "glsl")
            }
            Self::Fixture(fixture)
                if asset_path == &fixture_mapping_source_slot_path(owner_path) =>
            {
                let MappingConfig::SvgPath { source, .. } = fixture.mapping.value() else {
                    return None;
                };
                inline_text_from_slot(source, "svg")
            }
            _ => None,
        }
    }

    /// Inline binary asset bytes at `asset_path`, when this definition owns one.
    pub fn inline_asset_bytes(
        &self,
        owner_path: &SlotPath,
        asset_path: &SlotPath,
    ) -> Option<InlineAssetBytes<'_>> {
        match self {
            Self::Shader(shader) if asset_path == &source_slot_path(owner_path) => {
                inline_bytes_from_slot(shader.shader_source())
            }
            Self::ComputeShader(shader) if asset_path == &source_slot_path(owner_path) => {
                inline_bytes_from_slot(shader.shader_source())
            }
            Self::Fixture(fixture)
                if asset_path == &fixture_mapping_source_slot_path(owner_path) =>
            {
                let MappingConfig::SvgPath { source, .. } = fixture.mapping.value() else {
                    return None;
                };
                inline_bytes_from_slot(source)
            }
            _ => None,
        }
    }

    /// True when full authored bodies differ.
    pub fn body_changed(before: &Self, after: &Self) -> bool {
        before != after
    }

    /// True when parent-facing shell views differ.
    ///
    /// Inline child definition bodies are reduced to kind-only stubs so parent
    /// containers only report a shell change when child presence, references,
    /// ordering, or child kind changes.
    pub fn shell_changed(before: &Self, after: &Self) -> bool {
        def_shell(before) != def_shell(after)
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

    /// Read an authored TOML node artifact through the slot registry.
    pub fn read_toml(registry: &SlotShapeRegistry, text: &str) -> Result<Self, NodeDefParseError> {
        NodeArtifact::read_toml(registry, text).map(NodeArtifact::into_node_def)
    }

    /// Read authored TOML using the model crate's generated static shape registry.
    pub fn from_toml_str(text: &str) -> Result<Self, NodeDefParseError> {
        let registry = SlotShapeRegistry::default();
        Self::read_toml(&registry, text)
    }

    /// Write this node definition as authored TOML through the slot registry.
    pub fn write_toml(&self, registry: &SlotShapeRegistry) -> Result<String, NodeDefWriteError> {
        NodeArtifact::new(self.clone()).write_toml(registry)
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

fn assets_for_shader(
    source: &AssetSlot,
    containing_file: &LpPath,
    owner: &NodeDefLocation,
    base: &SlotPath,
    content_type: AssetContentType,
) -> Result<Vec<ReferencedAsset>, ArtifactPathResolutionError> {
    assets_for_slot(
        source,
        containing_file,
        owner,
        source_slot_path(base),
        content_type,
    )
}

fn assets_for_fixture(
    fixture: &FixtureDef,
    containing_file: &LpPath,
    owner: &NodeDefLocation,
    base: &SlotPath,
) -> Result<Vec<ReferencedAsset>, ArtifactPathResolutionError> {
    let MappingConfig::SvgPath { source, .. } = fixture.mapping.value() else {
        return Ok(Vec::new());
    };
    assets_for_slot(
        source,
        containing_file,
        owner,
        fixture_mapping_source_slot_path(base),
        AssetContentType::FixtureSvg,
    )
}

fn assets_for_slot(
    slot: &AssetSlot,
    containing_file: &LpPath,
    owner: &NodeDefLocation,
    asset_path: SlotPath,
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
        AssetSlotValue::InlineText { .. } | AssetSlotValue::InlineBytes { .. } => {
            Ok(vec![ReferencedAsset::new(
                AssetLocation::inline(owner.clone(), asset_path),
                content_type,
            )])
        }
    }
}

fn source_slot_path(base: &SlotPath) -> SlotPath {
    base.child(SlotName::parse("source").expect("source is a valid slot name"))
}

fn fixture_mapping_source_slot_path(base: &SlotPath) -> SlotPath {
    base.child(SlotName::parse("mapping").expect("mapping is a valid slot name"))
        .child(SlotName::parse("source").expect("source is a valid slot name"))
}

fn inline_text_from_slot<'a>(
    slot: &'a AssetSlot,
    default_extension: &'static str,
) -> Option<InlineAssetText<'a>> {
    let (extension, text) = slot.inline_text_value()?;
    Some(InlineAssetText {
        extension: extension.unwrap_or(default_extension),
        text,
    })
}

fn inline_bytes_from_slot(slot: &AssetSlot) -> Option<InlineAssetBytes<'_>> {
    let (extension, bytes) = slot.inline_bytes_value()?;
    Some(InlineAssetBytes { extension, bytes })
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

fn def_shell(def: &NodeDef) -> NodeDef {
    match def {
        NodeDef::Project(project) => {
            let mut shell = project.clone();
            for invocation in shell.nodes.entries.values_mut() {
                *invocation = EnumSlot::new(invocation_shell(invocation.value()));
            }
            NodeDef::Project(shell)
        }
        NodeDef::Playlist(playlist) => {
            let mut shell = playlist.clone();
            for entry in shell.entries.entries.values_mut() {
                entry.node = EnumSlot::new(invocation_shell(entry.node.value()));
            }
            NodeDef::Playlist(shell)
        }
        other => other.clone(),
    }
}

fn invocation_shell(invocation: &NodeInvocation) -> NodeInvocation {
    match invocation {
        NodeInvocation::Unset | NodeInvocation::Ref(_) => invocation.clone(),
        NodeInvocation::Def(body) => {
            NodeInvocation::inline(NodeDef::default_for_kind(body.value().kind()))
        }
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
    Toml { error: String },
}

impl core::fmt::Display for NodeDefParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownKind { kind } => write!(f, "unknown node kind `{kind}`"),
            Self::Toml { error } => f.write_str(error),
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

fn reject_unknown_kind(payload: &toml::Value) -> Result<(), NodeDefParseError> {
    let kind = read_kind(payload)?;
    if NODE_DEF_VARIANT_NAMES.contains(&kind.as_str()) {
        Ok(())
    } else {
        Err(NodeDefParseError::UnknownKind { kind })
    }
}

fn read_kind(payload: &toml::Value) -> Result<String, NodeDefParseError> {
    let Some(table) = payload.as_table() else {
        return Err(NodeDefParseError::Toml {
            error: String::from("node definition TOML root must be a table"),
        });
    };
    let Some(kind) = table.get("kind") else {
        return Err(NodeDefParseError::Toml {
            error: String::from("missing required field `kind`"),
        });
    };
    kind.as_str()
        .map(String::from)
        .ok_or_else(|| NodeDefParseError::Toml {
            error: String::from("field `kind` must be a string"),
        })
}

fn read_node_artifact(
    registry: &SlotShapeRegistry,
    payload: toml::Value,
) -> Result<NodeArtifact, NodeDefParseError> {
    let object = registry
        .read_slot_toml(NodeArtifact::SHAPE_ID, &payload)
        .map_err(|error| NodeDefParseError::Toml {
            error: error.to_string(),
        })?;
    object
        .into_any()
        .downcast::<NodeArtifact>()
        .map(|artifact| *artifact)
        .map_err(|_| NodeDefParseError::Toml {
            error: format!(
                "slot reader returned unexpected type for shape {}",
                NodeArtifact::SHAPE_ID
            ),
        })
}

fn write_node_artifact(
    registry: &SlotShapeRegistry,
    artifact: &NodeArtifact,
) -> Result<String, NodeDefWriteError> {
    let value = registry
        .write_slot_toml(artifact)
        .map_err(|error| NodeDefWriteError {
            error: error.to_string(),
        })?;
    toml::to_string(&value).map_err(|error| NodeDefWriteError {
        error: error.to_string(),
    })
}

fn toml_parse_error(error: toml::de::Error) -> NodeDefParseError {
    NodeDefParseError::Toml {
        error: format!("{error}"),
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
    fn node_def_parses_project_and_texture_toml() {
        let registry = registry();
        let project = NodeDef::read_toml(
            &registry,
            r#"
kind = "Project"

[nodes.texture]
ref = "./texture.toml"
"#,
        )
        .expect("project");
        assert!(matches!(project, NodeDef::Project(_)));

        let texture = NodeDef::read_toml(
            &registry,
            r#"
kind = "Texture"
size = { width = 64, height = 48 }
"#,
        )
        .expect("texture");
        assert!(matches!(texture, NodeDef::Texture(_)));
    }

    #[test]
    fn node_def_parses_shader_output_and_fixture_toml() {
        let registry = registry();

        let shader = NodeDef::read_toml(
            &registry,
            r#"
kind = "Shader"
render_order = 2

source = { path = "shader.glsl" }

[bindings.visual]
target = "bus#visual.out"
"#,
        )
        .expect("shader");
        assert!(matches!(shader, NodeDef::Shader(_)));

        let output = NodeDef::read_toml(
            &registry,
            r#"
kind = "Output"
endpoint = "ws281x:rmt:D10"

[options]
brightness = 0.5
"#,
        )
        .expect("output");
        assert!(matches!(output, NodeDef::Output(_)));

        let fixture = NodeDef::read_toml(
            &registry,
            r#"
kind = "Fixture"
render_size = { width = 8, height = 8 }
mapping = { kind = "PathPoints" }
"#,
        )
        .expect("fixture");
        let NodeDef::Fixture(fixture) = fixture else {
            panic!("expected fixture");
        };
        assert!(matches!(
            fixture.mapping.value(),
            MappingConfig::PathPoints { .. }
        ));

        let fixture = NodeDef::read_toml(
            &registry,
            r#"
kind = "Fixture"
render_size = { width = 64, height = 16 }

[mapping]
kind = "SvgPath"
source = "./fyeah-mapping.svg"
sample_diameter = 2.0
"#,
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
    fn node_def_round_trips_point_list_fixture_toml() {
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
            .write_toml(&registry)
            .expect("write fixture");
        let read = NodeDef::read_toml(&registry, &text).expect("read fixture");
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
    fn node_def_rejects_missing_invalid_and_unknown_kind() {
        let registry = registry();

        let missing =
            NodeDef::read_toml(&registry, "name = \"missing\"").expect_err("missing kind");
        assert!(missing.to_string().contains("kind"));

        let invalid = NodeDef::read_toml(&registry, "kind = 7").expect_err("invalid kind");
        assert!(invalid.to_string().contains("string"));

        let unknown = NodeDef::read_toml(&registry, "kind = \"bogus\"").expect_err("unknown kind");
        assert_eq!(
            unknown,
            NodeDefParseError::UnknownKind {
                kind: String::from("bogus")
            }
        );
    }

    #[test]
    fn node_artifact_root_loads_through_wrapper_shape() {
        let registry = registry();
        let payload = toml::from_str::<toml::Value>(
            r#"
kind = "Texture"
size = { width = 1, height = 2 }
"#,
        )
        .unwrap();

        let artifact = registry
            .read_slot_toml(NodeArtifact::SHAPE_ID, &payload)
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
    fn node_def_from_toml_uses_artifact_wrapper_loader() {
        let registry = registry();

        let def = NodeDef::read_toml(
            &registry,
            r#"
kind = "Texture"
size = { width = 1, height = 2 }
"#,
        )
        .expect("texture");

        let NodeDef::Texture(def) = def else {
            panic!("expected texture");
        };
        assert_eq!(def.size.value().width, 1);
        assert_eq!(def.size.value().height, 2);
    }

    #[test]
    fn node_def_writes_authored_toml_through_artifact_wrapper() {
        let write_registry = registry();
        let text = NodeDef::Texture(TextureDef::new(3, 4))
            .write_toml(&write_registry)
            .expect("write texture");

        assert!(text.contains("kind = \"Texture\""));
        assert!(text.contains("width = 3"));
        assert!(text.contains("height = 4"));

        let read = NodeDef::read_toml(&registry(), &text).expect("read texture");
        let NodeDef::Texture(def) = read else {
            panic!("expected texture");
        };
        assert_eq!(def.size.value().width, 3);
        assert_eq!(def.size.value().height, 4);
    }

    #[test]
    fn node_def_reads_binding_values_and_refs() {
        let registry = registry();

        let def = NodeDef::read_toml(
            &registry,
            r##"
kind = "Output"
endpoint = "ws281x:rmt:D10"

[bindings.main]
value = 0.25
"##,
        )
        .expect("output");
        let NodeDef::Output(def) = def else {
            panic!("expected output");
        };
        let binding = def.bindings.0.entries.get("main").expect("binding");
        assert_eq!(binding.value_literal(), Some(&LpValue::F32(0.25)));

        let def = NodeDef::read_toml(
            &registry,
            r##"
kind = "Output"
endpoint = "ws281x:rmt:D10"

[bindings.main]
target = "bus#control.out"
"##,
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
        let project = NodeDef::from_toml_str(
            r#"
kind = "Project"

[nodes.clock]
ref = "./clock.toml"
"#,
        )
        .expect("project");
        let sites = project.invocation_sites(&SlotPath::root());
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].path.to_string(), "nodes[clock]");
        assert!(matches!(sites[0].invocation, NodeInvocation::Ref(_)));

        let playlist = NodeDef::from_toml_str(
            r#"
kind = "Playlist"

[entries.2]
name = "active"

[entries.2.node.def]
kind = "Shader"
source = { path = "active.glsl" }
"#,
        )
        .expect("playlist");
        let sites = playlist.invocation_sites(&SlotPath::root());
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].path.to_string(), "entries[2].node");
        assert!(matches!(sites[0].invocation, NodeInvocation::Def(_)));
    }

    #[test]
    fn node_def_referenced_asset_paths_resolve_relative_shader_compute_and_fixture_paths() {
        let shader = NodeDef::from_toml_str(
            r#"
kind = "Shader"
source = { path = "shader.glsl" }
"#,
        )
        .expect("shader");
        assert_eq!(
            shader
                .referenced_asset_paths(LpPath::new("/nodes/shader.toml"))
                .unwrap(),
            vec![LpPathBuf::from("/nodes/shader.glsl")]
        );

        let compute = NodeDef::from_toml_str(
            r#"
kind = "ComputeShader"
source = { path = "../compute.glsl" }
"#,
        )
        .expect("compute");
        assert_eq!(
            compute
                .referenced_asset_paths(LpPath::new("/nodes/compute.toml"))
                .unwrap(),
            vec![LpPathBuf::from("/compute.glsl")]
        );

        let fixture = NodeDef::from_toml_str(
            r#"
kind = "Fixture"
render_size = { width = 64, height = 16 }

[mapping]
kind = "SvgPath"
source = "fixture.svg"
sample_diameter = 2.0
"#,
        )
        .expect("fixture");
        assert_eq!(
            fixture
                .referenced_asset_paths(LpPath::new("/fixtures/fixture.toml"))
                .unwrap(),
            vec![LpPathBuf::from("/fixtures/fixture.svg")]
        );
    }

    #[test]
    fn node_def_referenced_assets_include_source_identity_and_kind() {
        let owner = NodeDefLocation {
            artifact: ArtifactLocation::file("/project.toml"),
            path: SlotPath::parse("nodes[shader]").unwrap(),
        };
        let shader = NodeDef::from_toml_str(
            r#"
kind = "Shader"
source = { glsl = "void main() {}" }
"#,
        )
        .expect("shader");

        assert_eq!(
            shader
                .referenced_assets(LpPath::new("/project.toml"), &owner, &owner.path)
                .unwrap(),
            vec![ReferencedAsset::new(
                AssetLocation::inline(owner, SlotPath::parse("nodes[shader].source").unwrap()),
                AssetContentType::ShaderSource,
            )]
        );

        let fixture = NodeDef::from_toml_str(
            r#"
kind = "Fixture"
render_size = { width = 64, height = 16 }

[mapping]
kind = "SvgPath"
source = "fixture.svg"
sample_diameter = 2.0
"#,
        )
        .expect("fixture");
        let owner = NodeDefLocation::artifact_root(ArtifactLocation::file("/fixtures/f.toml"));

        assert_eq!(
            fixture
                .referenced_assets(LpPath::new("/fixtures/f.toml"), &owner, &owner.path)
                .unwrap(),
            vec![ReferencedAsset::new(
                AssetLocation::artifact(ArtifactLocation::file("/fixtures/fixture.svg")),
                AssetContentType::FixtureSvg,
            )]
        );
    }

    #[test]
    fn node_def_shell_change_ignores_inline_body_but_tracks_inline_kind() {
        let before = NodeDef::from_toml_str(
            r#"
kind = "Playlist"

[entries.2.node.def]
kind = "Shader"
source = { path = "a.glsl" }
"#,
        )
        .expect("before");
        let body_changed = NodeDef::from_toml_str(
            r#"
kind = "Playlist"

[entries.2.node.def]
kind = "Shader"
source = { path = "b.glsl" }
"#,
        )
        .expect("body changed");
        let kind_changed = NodeDef::from_toml_str(
            r#"
kind = "Playlist"

[entries.2.node.def]
kind = "Clock"
"#,
        )
        .expect("kind changed");

        assert!(NodeDef::body_changed(&before, &body_changed));
        assert!(!NodeDef::shell_changed(&before, &body_changed));
        assert!(NodeDef::shell_changed(&before, &kind_changed));
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
