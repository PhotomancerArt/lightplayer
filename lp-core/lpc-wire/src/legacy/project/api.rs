use crate::WireNodeStatus;
use crate::legacy::nodes::fixture::state::SerializableFixtureState;
use crate::legacy::nodes::output::state::SerializableOutputState;
use crate::legacy::nodes::shader::state::SerializableShaderState;
use crate::legacy::nodes::texture::state::SerializableTextureState;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use lpc_model::NodeId;
use lpc_model::lp_path::LpPathBuf;
use lpc_model::project::FrameId;
use lpc_source::legacy::nodes::{
    NodeConfig, NodeKind, fixture::FixtureConfig, output::OutputConfig, shader::ShaderConfig,
    texture::TextureConfig,
};
use serde::{Deserialize, Serialize, Serializer, ser::SerializeStructVariant};

/// Project response from server
///
/// Note: Cannot implement Clone because NodeDetail contains trait object.
///
/// TODO: Serialization is disabled in ServerResponse because ProjectResponse contains
/// `NodeDetail` which includes `Box<dyn NodeConfig>` (a trait object) that cannot be
/// serialized directly with serde. See `lpc-model/src/server/api.rs::ServerResponse`
/// for the disabled variant.
#[derive(Debug)]
pub enum ProjectResponse {
    /// Changes response
    GetChanges {
        /// Current frame ID
        current_frame: FrameId,
        /// Frame ID to compare against (since_frame from request)
        since_frame: FrameId,
        /// All current node handles (for pruning removed nodes)
        node_handles: Vec<NodeId>,
        /// Changed nodes since since_frame
        node_changes: Vec<NodeChange>,
        /// Full detail for requested nodes
        node_details: BTreeMap<NodeId, NodeDetail>,
        /// Theoretical FPS based on frame processing time (None if not available)
        theoretical_fps: Option<f32>,
    },
}

/// Node change notification
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeChange {
    /// New node created
    Created {
        handle: NodeId,
        path: LpPathBuf,
        kind: NodeKind,
    },
    /// Config updated
    ConfigUpdated { handle: NodeId, config_ver: FrameId },
    /// State updated
    StateUpdated { handle: NodeId, state_ver: FrameId },
    /// Status changed
    StatusChanged {
        handle: NodeId,
        status: WireNodeStatus,
    },
    /// Node removed
    Removed { handle: NodeId },
}
/// Node detail - full config + state
///
/// Note: Cannot implement Clone/PartialEq/Eq because config is a trait object.
///
/// TODO: Serialization is blocked because `Box<dyn NodeConfig>` cannot be serialized
/// directly with serde. This prevents ProjectResponse (which contains NodeDetail) from
/// being serialized in ServerResponse.
///
/// Options for future implementation:
/// 1. Create a serializable wrapper enum that matches on NodeKind and serializes concrete types
/// 2. Implement custom Serialize/Deserialize that dispatches based on NodeKind
/// 3. Refactor to use an enum instead of trait objects (breaking change)
///
/// See: `lpc-model/src/server/api.rs::ServerResponse` for where this blocks serialization
#[derive(Debug)]
pub struct NodeDetail {
    pub path: LpPathBuf,
    pub config: Box<dyn NodeConfig>, // TODO: Needs serialization support (see struct docs)
    pub state: NodeState,            // External state only
}

/// Node state - external state (shared with clients)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NodeState {
    Texture(crate::legacy::nodes::texture::TextureState),
    Shader(crate::legacy::nodes::shader::ShaderState),
    Output(crate::legacy::nodes::output::OutputState),
    Fixture(crate::legacy::nodes::fixture::FixtureState),
}

impl NodeState {
    /// Merge fields from a partial update into this state
    ///
    /// Only fields that are present in `other` (non-default values) are merged.
    /// Fields not present in the partial update are preserved from `self`.
    ///
    /// # Panics
    ///
    /// Panics if `self` and `other` are different variants (should not happen in practice).
    pub fn merge_from(&mut self, other: &Self, frame_id: FrameId) {
        match self {
            NodeState::Texture(self_state) => {
                if let NodeState::Texture(other_state) = other {
                    self_state.merge_from(other_state, frame_id);
                } else {
                    // Mismatched variants shouldn't happen, but handle gracefully by replacing
                    *self = other.clone();
                }
            }
            NodeState::Shader(self_state) => {
                if let NodeState::Shader(other_state) = other {
                    self_state.merge_from(other_state, frame_id);
                } else {
                    *self = other.clone();
                }
            }
            NodeState::Output(self_state) => {
                if let NodeState::Output(other_state) = other {
                    self_state.merge_from(other_state, frame_id);
                } else {
                    *self = other.clone();
                }
            }
            NodeState::Fixture(self_state) => {
                if let NodeState::Fixture(other_state) = other {
                    self_state.merge_from(other_state, frame_id);
                } else {
                    *self = other.clone();
                }
            }
        }
    }
}

/// Serializable wrapper for NodeDetail
///
/// This enum allows NodeDetail (which contains Box<dyn NodeConfig>) to be serialized
/// by matching on NodeKind and serializing concrete config types.
///
/// The state field is serialized with context-aware serialization that only includes
/// fields changed since since_frame (handled by SerializableProjectResponse).
///
/// Note: When serialized standalone (e.g., in tests), all fields are serialized.
/// When serialized as part of SerializableProjectResponse, partial serialization
/// is used based on since_frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SerializableNodeDetail {
    /// Texture node detail
    Texture {
        path: LpPathBuf,
        config: TextureConfig,
        state: NodeState,
    },
    /// Shader node detail
    Shader {
        path: LpPathBuf,
        config: ShaderConfig,
        state: NodeState,
    },
    /// Output node detail
    Output {
        path: LpPathBuf,
        config: OutputConfig,
        state: NodeState,
    },
    /// Fixture node detail
    Fixture {
        path: LpPathBuf,
        config: FixtureConfig,
        state: NodeState,
    },
}

/// Serializable wrapper for ProjectResponse
///
/// This enum allows ProjectResponse (which contains NodeDetail) to be serialized
/// by using SerializableNodeDetail instead of NodeDetail.
///
/// Note: node_details uses Vec instead of BTreeMap because JSON map keys must be strings,
/// and tuple structs don't deserialize correctly from string keys.
///
/// Uses custom [`Serialize`] so nested [`NodeState`] uses [`SerializableTextureState`] /
/// shader/output/fixture wrappers and omits unchanged fields when `since_frame` is not initial.
///
/// JSON round-trip preserves scalar snapshot fields on initial sync (`since_frame`
/// [`FrameId::default()`]). [`Versioned`] metadata uses [`FrameId::default()`] on deserialize
/// because the wire omits provenance; compare `.value()` when testing payloads.
///
/// [`Deserialize`] matches this wire shape (externally tagged [`SerializableNodeDetail`] and
/// [`NodeState`] variants).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub enum SerializableProjectResponse {
    /// Changes response
    GetChanges {
        /// Current frame ID
        current_frame: FrameId,
        /// Frame ID to compare against (since_frame from request)
        since_frame: FrameId,
        /// All current node handles (for pruning removed nodes)
        node_handles: Vec<NodeId>,
        /// Changed nodes since since_frame
        node_changes: Vec<NodeChange>,
        /// Full detail for requested nodes (serializable)
        /// Uses Vec instead of BTreeMap for JSON compatibility
        node_details: Vec<(NodeId, SerializableNodeDetail)>,
        /// Theoretical FPS based on frame processing time (None if not available)
        theoretical_fps: Option<f32>,
    },
}

/// Wraps state serialization so it produces NodeState variant format (e.g. {"Texture": {...}})
/// instead of a bare struct. Required because NodeState is an externally tagged enum.
enum NodeStateSerializer<'a> {
    Texture(&'a SerializableTextureState<'a>),
    Shader(&'a SerializableShaderState<'a>),
    Output(&'a SerializableOutputState<'a>),
    Fixture(&'a SerializableFixtureState<'a>),
}

impl Serialize for NodeStateSerializer<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Texture(s) => serializer.serialize_newtype_variant("NodeState", 0, "Texture", s),
            Self::Shader(s) => serializer.serialize_newtype_variant("NodeState", 1, "Shader", s),
            Self::Output(s) => serializer.serialize_newtype_variant("NodeState", 2, "Output", s),
            Self::Fixture(s) => serializer.serialize_newtype_variant("NodeState", 3, "Fixture", s),
        }
    }
}

// Helper struct to serialize SerializableNodeDetail with since_frame context
struct SerializableNodeDetailWithFrame<'a> {
    detail: &'a SerializableNodeDetail,
    since_frame: FrameId,
}

impl<'a> Serialize for SerializableNodeDetailWithFrame<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.detail {
            SerializableNodeDetail::Texture {
                path,
                config,
                state,
            } => {
                let mut s = serializer.serialize_struct_variant(
                    "SerializableNodeDetail",
                    0,
                    "Texture",
                    3,
                )?;
                s.serialize_field("path", path)?;
                s.serialize_field("config", config)?;
                match state {
                    NodeState::Texture(texture_state) => {
                        let serializable_state =
                            SerializableTextureState::new(texture_state, self.since_frame);
                        s.serialize_field(
                            "state",
                            &NodeStateSerializer::Texture(&serializable_state),
                        )?;
                    }
                    _ => return Err(serde::ser::Error::custom("State kind mismatch")),
                }
                s.end()
            }
            SerializableNodeDetail::Shader {
                path,
                config,
                state,
            } => {
                let mut s = serializer.serialize_struct_variant(
                    "SerializableNodeDetail",
                    1,
                    "Shader",
                    3,
                )?;
                s.serialize_field("path", path)?;
                s.serialize_field("config", config)?;
                match state {
                    NodeState::Shader(shader_state) => {
                        let serializable_state =
                            SerializableShaderState::new(shader_state, self.since_frame);
                        s.serialize_field(
                            "state",
                            &NodeStateSerializer::Shader(&serializable_state),
                        )?;
                    }
                    _ => return Err(serde::ser::Error::custom("State kind mismatch")),
                }
                s.end()
            }
            SerializableNodeDetail::Output {
                path,
                config,
                state,
            } => {
                let mut s = serializer.serialize_struct_variant(
                    "SerializableNodeDetail",
                    2,
                    "Output",
                    3,
                )?;
                s.serialize_field("path", path)?;
                s.serialize_field("config", config)?;
                match state {
                    NodeState::Output(output_state) => {
                        let serializable_state =
                            SerializableOutputState::new(output_state, self.since_frame);
                        s.serialize_field(
                            "state",
                            &NodeStateSerializer::Output(&serializable_state),
                        )?;
                    }
                    _ => return Err(serde::ser::Error::custom("State kind mismatch")),
                }
                s.end()
            }
            SerializableNodeDetail::Fixture {
                path,
                config,
                state,
            } => {
                let mut s = serializer.serialize_struct_variant(
                    "SerializableNodeDetail",
                    3,
                    "Fixture",
                    3,
                )?;
                s.serialize_field("path", path)?;
                s.serialize_field("config", config)?;
                match state {
                    NodeState::Fixture(fixture_state) => {
                        let serializable_state =
                            SerializableFixtureState::new(fixture_state, self.since_frame);
                        s.serialize_field(
                            "state",
                            &NodeStateSerializer::Fixture(&serializable_state),
                        )?;
                    }
                    _ => return Err(serde::ser::Error::custom("State kind mismatch")),
                }
                s.end()
            }
        }
    }
}

// Helper struct for serializing GetChanges variant with context-aware state serialization
struct GetChangesSerializer<'a> {
    current_frame: &'a FrameId,
    since_frame: &'a FrameId,
    node_handles: &'a Vec<NodeId>,
    node_changes: &'a Vec<NodeChange>,
    node_details: &'a Vec<(NodeId, SerializableNodeDetail)>,
    theoretical_fps: &'a Option<f32>,
}

impl<'a> Serialize for GetChangesSerializer<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("GetChanges", 6)?;
        state.serialize_field("current_frame", self.current_frame)?;
        state.serialize_field("since_frame", self.since_frame)?;
        state.serialize_field("node_handles", self.node_handles)?;
        state.serialize_field("node_changes", self.node_changes)?;

        // Serialize node_details with context-aware state serialization
        struct TupleSerializer<'b, T1: Serialize, T2: Serialize> {
            item1: &'b T1,
            item2: &'b T2,
        }
        impl<'b, T1: Serialize, T2: Serialize> Serialize for TupleSerializer<'b, T1, T2> {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                use serde::ser::SerializeTuple;
                let mut tuple = serializer.serialize_tuple(2)?;
                tuple.serialize_element(self.item1)?;
                tuple.serialize_element(self.item2)?;
                tuple.end()
            }
        }
        // Collect SerializableNodeDetailWithFrame instances first (to avoid lifetime issues)
        let detail_wrappers: Vec<_> = self
            .node_details
            .iter()
            .map(|(_, detail)| SerializableNodeDetailWithFrame {
                detail,
                since_frame: *self.since_frame,
            })
            .collect();
        // Then create tuples referencing the wrappers
        let serializable_tuples: Vec<_> = self
            .node_details
            .iter()
            .zip(detail_wrappers.iter())
            .map(|((handle, _), wrapper)| TupleSerializer {
                item1: handle,
                item2: wrapper,
            })
            .collect();
        state.serialize_field("node_details", &serializable_tuples)?;
        state.serialize_field("theoretical_fps", self.theoretical_fps)?;
        state.end()
    }
}

impl Serialize for SerializableProjectResponse {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            SerializableProjectResponse::GetChanges {
                current_frame,
                since_frame,
                node_handles,
                node_changes,
                node_details,
                theoretical_fps,
            } => {
                // Serialize as externally tagged enum: {"GetChanges": {...}}
                // Use a helper struct that will be serialized as the enum variant content
                let variant_serializer = GetChangesSerializer {
                    current_frame,
                    since_frame,
                    node_handles,
                    node_changes,
                    node_details,
                    theoretical_fps,
                };
                // Serialize as map with single entry for enum variant
                use serde::ser::SerializeMap;
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("GetChanges", &variant_serializer)?;
                map.end()
            }
        }
    }
}

impl NodeDetail {
    /// Convert NodeDetail to SerializableNodeDetail
    ///
    /// Downcasts the Box<dyn NodeConfig> to the concrete config type based on NodeKind.
    /// The state is stored as NodeState; context-aware serialization happens during
    /// SerializableProjectResponse serialization.
    pub fn to_serializable(&self) -> Result<SerializableNodeDetail, String> {
        let kind = self.config.kind();
        match kind {
            NodeKind::Texture => {
                let config = self
                    .config
                    .as_any()
                    .downcast_ref::<TextureConfig>()
                    .ok_or_else(|| format!("Failed to downcast to TextureConfig"))?;
                Ok(SerializableNodeDetail::Texture {
                    path: self.path.clone(),
                    config: config.clone(),
                    state: self.state.clone(),
                })
            }
            NodeKind::Shader => {
                let config = self
                    .config
                    .as_any()
                    .downcast_ref::<ShaderConfig>()
                    .ok_or_else(|| format!("Failed to downcast to ShaderConfig"))?;
                Ok(SerializableNodeDetail::Shader {
                    path: self.path.clone(),
                    config: config.clone(),
                    state: self.state.clone(),
                })
            }
            NodeKind::Output => {
                let config = self
                    .config
                    .as_any()
                    .downcast_ref::<OutputConfig>()
                    .ok_or_else(|| format!("Failed to downcast to OutputConfig"))?;
                Ok(SerializableNodeDetail::Output {
                    path: self.path.clone(),
                    config: config.clone(),
                    state: self.state.clone(),
                })
            }
            NodeKind::Fixture => {
                let config = self
                    .config
                    .as_any()
                    .downcast_ref::<FixtureConfig>()
                    .ok_or_else(|| format!("Failed to downcast to FixtureConfig"))?;
                Ok(SerializableNodeDetail::Fixture {
                    path: self.path.clone(),
                    config: config.clone(),
                    state: self.state.clone(),
                })
            }
        }
    }
}

impl ProjectResponse {
    /// Convert ProjectResponse to SerializableProjectResponse
    ///
    /// Converts all NodeDetail entries to SerializableNodeDetail with since_frame context.
    pub fn to_serializable(&self) -> Result<SerializableProjectResponse, String> {
        match self {
            ProjectResponse::GetChanges {
                current_frame,
                since_frame,
                node_handles,
                node_changes,
                node_details,
                theoretical_fps,
            } => {
                let mut serializable_details = Vec::new();
                for (handle, detail) in node_details {
                    let serializable_detail = detail.to_serializable()?;
                    serializable_details.push((*handle, serializable_detail));
                }
                Ok(SerializableProjectResponse::GetChanges {
                    current_frame: *current_frame,
                    since_frame: *since_frame,
                    node_handles: node_handles.clone(),
                    node_changes: node_changes.clone(),
                    node_details: serializable_details,
                    theoretical_fps: *theoretical_fps,
                })
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use lpc_source::legacy::nodes::shader::ShaderConfig;
    use lpc_source::legacy::nodes::texture::{TextureConfig, TextureFormat};
    use serde_json::Value;

    #[test]
    fn test_node_state() {
        use lpc_model::project::FrameId;
        let mut tex_state = crate::legacy::nodes::texture::TextureState::new(FrameId::default());
        tex_state
            .texture_data
            .set(FrameId::default(), vec![0, 1, 2, 3]);
        tex_state.width.set(FrameId::default(), 2);
        tex_state.height.set(FrameId::default(), 2);
        tex_state
            .format
            .set(FrameId::default(), TextureFormat::Rgba16);
        let state = NodeState::Texture(tex_state);
        match state {
            NodeState::Texture(tex_state) => {
                assert_eq!(tex_state.texture_data.value().len(), 4);
            }
            _ => panic!("Expected Texture state"),
        }
    }

    #[test]
    fn test_node_detail_to_serializable_texture() {
        use lpc_model::project::FrameId;
        let mut tex_state = crate::legacy::nodes::texture::TextureState::new(FrameId::default());
        tex_state
            .texture_data
            .set(FrameId::default(), vec![0, 1, 2, 3]);
        tex_state.width.set(FrameId::default(), 2);
        tex_state.height.set(FrameId::default(), 2);
        tex_state
            .format
            .set(FrameId::default(), TextureFormat::Rgba16);
        let detail = NodeDetail {
            path: LpPathBuf::from("/src/texture.texture"),
            config: Box::new(TextureConfig {
                width: 100,
                height: 200,
            }),
            state: NodeState::Texture(tex_state),
        };
        let serializable = detail.to_serializable().unwrap();
        match serializable {
            SerializableNodeDetail::Texture {
                path,
                config,
                state,
            } => {
                assert_eq!(path.as_str(), "/src/texture.texture");
                assert_eq!(config.width, 100);
                assert_eq!(config.height, 200);
                assert!(matches!(state, NodeState::Texture(_)));
            }
            _ => panic!("Expected Texture variant"),
        }
    }

    #[test]
    fn test_node_detail_to_serializable_shader() {
        use lpc_model::project::FrameId;
        let shader_state = crate::legacy::nodes::shader::ShaderState::new(FrameId::default());
        let detail = NodeDetail {
            path: LpPathBuf::from("/src/shader.shader"),
            config: Box::new(ShaderConfig::default()),
            state: NodeState::Shader(shader_state),
        };
        let serializable = detail.to_serializable().unwrap();
        match serializable {
            SerializableNodeDetail::Shader {
                path,
                config: _,
                state,
            } => {
                assert_eq!(path.as_str(), "/src/shader.shader");
                assert!(matches!(state, NodeState::Shader(_)));
            }
            _ => panic!("Expected Shader variant"),
        }
    }

    #[test]
    fn test_project_response_to_serializable() {
        let mut node_details = BTreeMap::new();
        node_details.insert(
            NodeId::new(1),
            NodeDetail {
                path: LpPathBuf::from("/src/texture.texture"),
                config: Box::new(TextureConfig {
                    width: 100,
                    height: 200,
                }),
                state: {
                    use lpc_model::project::FrameId;
                    let mut tex_state =
                        crate::legacy::nodes::texture::TextureState::new(FrameId::default());
                    tex_state
                        .texture_data
                        .set(FrameId::default(), vec![0, 1, 2, 3]);
                    tex_state.width.set(FrameId::default(), 2);
                    tex_state.height.set(FrameId::default(), 2);
                    tex_state
                        .format
                        .set(FrameId::default(), TextureFormat::Rgba16);
                    NodeState::Texture(tex_state)
                },
            },
        );

        let response = ProjectResponse::GetChanges {
            current_frame: FrameId::default(),
            since_frame: FrameId::default(),
            node_handles: vec![NodeId::new(1)],
            node_changes: vec![],
            node_details,
            theoretical_fps: None,
        };

        let serializable = response.to_serializable().unwrap();
        match serializable {
            SerializableProjectResponse::GetChanges {
                current_frame,
                since_frame: _,
                node_handles,
                node_changes,
                node_details,
                theoretical_fps: _,
            } => {
                assert_eq!(current_frame, FrameId::default());
                assert_eq!(node_handles.len(), 1);
                assert_eq!(node_changes.len(), 0);
                assert_eq!(node_details.len(), 1);
                assert!(
                    node_details
                        .iter()
                        .any(|(handle, _)| *handle == NodeId::new(1))
                );
            }
        }
    }

    #[test]
    fn test_serializable_node_detail_serialization() {
        use lpc_model::project::FrameId;
        let mut tex_state = crate::legacy::nodes::texture::TextureState::new(FrameId::default());
        tex_state
            .texture_data
            .set(FrameId::default(), vec![0, 1, 2, 3]);
        tex_state.width.set(FrameId::default(), 2);
        tex_state.height.set(FrameId::default(), 2);
        tex_state
            .format
            .set(FrameId::default(), TextureFormat::Rgba16);
        let detail = SerializableNodeDetail::Texture {
            path: LpPathBuf::from("/src/texture.texture"),
            config: TextureConfig {
                width: 100,
                height: 200,
            },
            state: NodeState::Texture(tex_state),
        };
        let json = crate::json::to_string(&detail).unwrap();
        let deserialized: SerializableNodeDetail = crate::json::from_str(&json).unwrap();
        match deserialized {
            SerializableNodeDetail::Texture {
                path,
                config,
                state: _,
            } => {
                assert_eq!(path.as_str(), "/src/texture.texture");
                assert_eq!(config.width, 100);
                assert_eq!(config.height, 200);
            }
            _ => panic!("Expected Texture variant"),
        }
    }

    #[test]
    fn serializable_project_response_initial_sync_round_trip_and_wire_shape() {
        let response = sample_get_changes_texture_response(FrameId::default(), FrameId::default());
        let json = crate::json::to_string(&response).unwrap();
        assert_serializable_project_response_wire_shape(&json, WireShapeExpect::InitialSyncTexture);
        let deserialized: SerializableProjectResponse = crate::json::from_str(&json).unwrap();
        assert_serializable_project_response_semantically_equal(&response, &deserialized);
    }

    #[test]
    fn serializable_project_response_partial_texture_omits_stale_fields() {
        let since_frame = FrameId::new(2);
        let response = sample_get_changes_texture_response(FrameId::new(10), since_frame);
        let json = crate::json::to_string(&response).unwrap();
        assert_serializable_project_response_wire_shape(&json, WireShapeExpect::PartialTexture);

        let deserialized: SerializableProjectResponse = crate::json::from_str(&json).unwrap();
        match deserialized {
            SerializableProjectResponse::GetChanges {
                since_frame: sf,
                node_details,
                ..
            } => {
                assert_eq!(sf, since_frame);
                assert_eq!(node_details.len(), 1);
                let (
                    _,
                    SerializableNodeDetail::Texture {
                        state: NodeState::Texture(tex),
                        ..
                    },
                ) = &node_details[0]
                else {
                    panic!("expected Texture detail");
                };
                // Deserialized omitted fields use defaults at frame 0; merge_from repairs client view.
                assert_eq!(tex.texture_data.value(), &Vec::<u8>::new());
                assert_eq!(tex.format.value(), &TextureFormat::Rgba16);
                assert_eq!(tex.width.value(), &150);
                assert_eq!(tex.height.value(), &250);
            }
        }
    }

    fn assert_serializable_project_response_semantically_equal(
        original: &SerializableProjectResponse,
        decoded: &SerializableProjectResponse,
    ) {
        let (
            SerializableProjectResponse::GetChanges {
                current_frame: cf_a,
                since_frame: sf_a,
                node_handles: nh_a,
                node_changes: nc_a,
                node_details: nd_a,
                theoretical_fps: fps_a,
            },
            SerializableProjectResponse::GetChanges {
                current_frame: cf_b,
                since_frame: sf_b,
                node_handles: nh_b,
                node_changes: nc_b,
                node_details: nd_b,
                theoretical_fps: fps_b,
            },
        ) = (original, decoded);
        assert_eq!(cf_a, cf_b);
        assert_eq!(sf_a, sf_b);
        assert_eq!(nh_a, nh_b);
        assert_eq!(nc_a, nc_b);
        assert_eq!(fps_a, fps_b);
        assert_eq!(nd_a.len(), nd_b.len());
        for ((ha, da), (hb, db)) in nd_a.iter().zip(nd_b.iter()) {
            assert_eq!(ha, hb);
            assert_serializable_node_detail_semantically_equal(da, db);
        }
    }

    fn assert_serializable_node_detail_semantically_equal(
        a: &SerializableNodeDetail,
        b: &SerializableNodeDetail,
    ) {
        match (a, b) {
            (
                SerializableNodeDetail::Texture {
                    path: pa,
                    config: ca,
                    state: NodeState::Texture(sa),
                },
                SerializableNodeDetail::Texture {
                    path: pb,
                    config: cb,
                    state: NodeState::Texture(sb),
                },
            ) => {
                assert_eq!(pa, pb);
                assert_eq!(ca, cb);
                assert_texture_snapshot_equal(sa, sb);
            }
            _ => panic!("fixture uses Texture detail only"),
        }
    }

    fn assert_texture_snapshot_equal(
        a: &crate::legacy::nodes::texture::TextureState,
        b: &crate::legacy::nodes::texture::TextureState,
    ) {
        assert_eq!(a.texture_data.value(), b.texture_data.value());
        assert_eq!(a.width.value(), b.width.value());
        assert_eq!(a.height.value(), b.height.value());
        assert_eq!(a.format.value(), b.format.value());
    }

    enum WireShapeExpect {
        InitialSyncTexture,
        PartialTexture,
    }

    fn get_changes_object<'a>(v: &'a Value) -> &'a serde_json::Map<String, Value> {
        v.as_object()
            .expect("root JSON object")
            .get("GetChanges")
            .expect("externally tagged GetChanges")
            .as_object()
            .expect("GetChanges payload object")
    }

    fn texture_state_json<'a>(gc: &'a serde_json::Map<String, Value>) -> &'a Value {
        let details = gc
            .get("node_details")
            .expect("node_details")
            .as_array()
            .expect("node_details array");
        let pair = details.first().expect("one detail tuple");
        pair.get(1)
            .expect("detail entry")
            .get("Texture")
            .expect("SerializableNodeDetail.Texture")
            .get("state")
            .expect("detail.state")
            .get("Texture")
            .expect("NodeState.Texture externally tagged")
    }

    fn assert_serializable_project_response_wire_shape(json: &str, expect: WireShapeExpect) {
        let v: Value = serde_json::from_str(json).expect("valid JSON");
        let gc = get_changes_object(&v);
        assert!(
            gc.contains_key("current_frame") && gc.contains_key("since_frame"),
            "GetChanges must carry frame ids"
        );
        assert!(gc.contains_key("node_handles"));
        assert!(gc.contains_key("node_changes"));
        assert!(gc.contains_key("node_details"));
        assert!(gc.contains_key("theoretical_fps"));

        let inner = texture_state_json(gc);
        match expect {
            WireShapeExpect::InitialSyncTexture => {
                for key in ["texture_data", "width", "height", "format"] {
                    assert!(
                        inner.get(key).is_some(),
                        "initial sync must include `{key}`, got {inner:?}"
                    );
                }
            }
            WireShapeExpect::PartialTexture => {
                assert!(
                    inner.get("width").is_some() && inner.get("height").is_some(),
                    "partial payload should include updated fields: {inner:?}"
                );
                assert!(
                    inner.get("texture_data").is_none() && inner.get("format").is_none(),
                    "fields unchanged since since_frame must be omitted: {inner:?}"
                );
            }
        }
    }

    fn sample_get_changes_texture_response(
        current_frame: FrameId,
        since_frame: FrameId,
    ) -> SerializableProjectResponse {
        let mut tex_state = crate::legacy::nodes::texture::TextureState::new(FrameId::new(1));
        tex_state
            .texture_data
            .set(FrameId::new(1), vec![1, 2, 3, 4]);
        tex_state.width.set(FrameId::new(1), 100);
        tex_state.height.set(FrameId::new(1), 200);
        tex_state.format.set(FrameId::new(1), TextureFormat::Rgb8);

        if since_frame != FrameId::default() {
            tex_state.width.set(FrameId::new(5), 150);
            tex_state.height.set(FrameId::new(5), 250);
        }

        SerializableProjectResponse::GetChanges {
            current_frame,
            since_frame,
            node_handles: vec![NodeId::new(1)],
            node_changes: vec![],
            node_details: vec![(
                NodeId::new(1),
                SerializableNodeDetail::Texture {
                    path: LpPathBuf::from("/src/texture.texture"),
                    config: TextureConfig {
                        width: 100,
                        height: 200,
                    },
                    state: NodeState::Texture(tex_state),
                },
            )],
            theoretical_fps: Some(60.0),
        }
    }
}
