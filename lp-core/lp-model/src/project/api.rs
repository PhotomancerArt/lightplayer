use crate::nodes::{NodeConfig, NodeHandle, NodeKind};
use crate::nodes::{
    fixture::FixtureConfig, output::OutputConfig, shader::ShaderConfig, texture::TextureConfig,
};
use crate::path::LpPathBuf;
use crate::project::FrameId;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

/// Node specifier for API requests
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ApiNodeSpecifier {
    /// No nodes
    None,
    /// All nodes
    All,
    /// Specific handles
    ByHandles(Vec<NodeHandle>),
}

/// Project request from client
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ProjectRequest {
    /// Get changes since a frame
    GetChanges {
        /// Last frame client synced
        since_frame: FrameId,
        /// Which nodes need full state
        detail_specifier: ApiNodeSpecifier,
    },
}

/// Project response from server
///
/// Note: Cannot implement Clone because NodeDetail contains trait object.
///
/// TODO: Serialization is disabled in ServerResponse because ProjectResponse contains
/// `NodeDetail` which includes `Box<dyn NodeConfig>` (a trait object) that cannot be
/// serialized directly with serde. See `lp-model/src/server/api.rs::ServerResponse`
/// for the disabled variant.
#[derive(Debug)]
pub enum ProjectResponse {
    /// Changes response
    GetChanges {
        /// Current frame ID
        current_frame: FrameId,
        /// All current node handles (for pruning removed nodes)
        node_handles: Vec<NodeHandle>,
        /// Changed nodes since since_frame
        node_changes: Vec<NodeChange>,
        /// Full detail for requested nodes
        node_details: BTreeMap<NodeHandle, NodeDetail>,
        /// Theoretical FPS based on frame processing time (None if not available)
        theoretical_fps: Option<f32>,
    },
}

/// Node change notification
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeChange {
    /// New node created
    Created {
        handle: NodeHandle,
        path: LpPathBuf,
        kind: NodeKind,
    },
    /// Config updated
    ConfigUpdated {
        handle: NodeHandle,
        config_ver: FrameId,
    },
    /// State updated
    StateUpdated {
        handle: NodeHandle,
        state_ver: FrameId,
    },
    /// Status changed
    StatusChanged {
        handle: NodeHandle,
        status: NodeStatus,
    },
    /// Node removed
    Removed { handle: NodeHandle },
}

/// Node status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeStatus {
    /// Created but not yet initialized
    Created,
    /// Error initializing the node
    InitError(String),
    /// Node is running normally
    Ok,
    /// Node is running, but something is wrong
    Warn(String),
    /// Node cannot run
    Error(String),
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
/// See: `lp-model/src/server/api.rs::ServerResponse` for where this blocks serialization
#[derive(Debug)]
pub struct NodeDetail {
    pub path: LpPathBuf,
    pub config: Box<dyn NodeConfig>, // TODO: Needs serialization support (see struct docs)
    pub state: NodeState,            // External state only
}

/// Node state - external state (shared with clients)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NodeState {
    Texture(crate::nodes::texture::TextureState),
    Shader(crate::nodes::shader::ShaderState),
    Output(crate::nodes::output::OutputState),
    Fixture(crate::nodes::fixture::FixtureState),
}

/// Serializable wrapper for NodeDetail
///
/// This enum allows NodeDetail (which contains Box<dyn NodeConfig>) to be serialized
/// by matching on NodeKind and serializing concrete config types.
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SerializableProjectResponse {
    /// Changes response
    GetChanges {
        /// Current frame ID
        current_frame: FrameId,
        /// All current node handles (for pruning removed nodes)
        node_handles: Vec<NodeHandle>,
        /// Changed nodes since since_frame
        node_changes: Vec<NodeChange>,
        /// Full detail for requested nodes (serializable)
        /// Uses Vec instead of BTreeMap for JSON compatibility
        node_details: Vec<(NodeHandle, SerializableNodeDetail)>,
        /// Theoretical FPS based on frame processing time (None if not available)
        theoretical_fps: Option<f32>,
    },
}

impl NodeDetail {
    /// Convert NodeDetail to SerializableNodeDetail
    ///
    /// Downcasts the Box<dyn NodeConfig> to the concrete config type based on NodeKind.
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
    /// Converts all NodeDetail entries to SerializableNodeDetail.
    pub fn to_serializable(&self) -> Result<SerializableProjectResponse, String> {
        match self {
            ProjectResponse::GetChanges {
                current_frame,
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
    use alloc::{string::ToString, vec};

    #[test]
    fn test_api_node_specifier() {
        let spec = ApiNodeSpecifier::None;
        assert_eq!(spec, ApiNodeSpecifier::None);

        let spec = ApiNodeSpecifier::All;
        assert_eq!(spec, ApiNodeSpecifier::All);

        let spec = ApiNodeSpecifier::ByHandles(vec![NodeHandle::new(1), NodeHandle::new(2)]);
        match spec {
            ApiNodeSpecifier::ByHandles(handles) => {
                assert_eq!(handles.len(), 2);
            }
            _ => panic!("Expected ByHandles"),
        }
    }

    #[test]
    fn test_project_request() {
        let request = ProjectRequest::GetChanges {
            since_frame: FrameId::default(),
            detail_specifier: ApiNodeSpecifier::All,
        };
        match request {
            ProjectRequest::GetChanges {
                since_frame,
                detail_specifier,
            } => {
                assert_eq!(since_frame, FrameId::default());
                assert_eq!(detail_specifier, ApiNodeSpecifier::All);
            }
        }
    }

    #[test]
    fn test_node_status() {
        let status = NodeStatus::Created;
        assert_eq!(status, NodeStatus::Created);

        let status = NodeStatus::InitError("test error".to_string());
        match status {
            NodeStatus::InitError(msg) => assert_eq!(msg, "test error"),
            _ => panic!("Expected InitError"),
        }
    }

    #[test]
    fn test_node_state() {
        let state = NodeState::Texture(crate::nodes::texture::TextureState {
            texture_data: vec![0, 1, 2, 3],
            width: 2,
            height: 2,
            format: "RGBA8".to_string(),
        });
        match state {
            NodeState::Texture(tex_state) => {
                assert_eq!(tex_state.texture_data.len(), 4);
            }
            _ => panic!("Expected Texture state"),
        }
    }

    #[test]
    fn test_node_detail_to_serializable_texture() {
        use crate::nodes::texture::TextureConfig;
        let detail = NodeDetail {
            path: LpPathBuf::from("/src/texture.texture"),
            config: Box::new(TextureConfig {
                width: 100,
                height: 200,
            }),
            state: NodeState::Texture(crate::nodes::texture::TextureState {
                texture_data: vec![0, 1, 2, 3],
                width: 2,
                height: 2,
                format: "RGBA8".to_string(),
            }),
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
        use crate::nodes::shader::ShaderConfig;
        let detail = NodeDetail {
            path: LpPathBuf::from("/src/shader.shader"),
            config: Box::new(ShaderConfig::default()),
            state: NodeState::Shader(crate::nodes::shader::ShaderState {
                glsl_code: String::new(),
                error: None,
            }),
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
        use crate::nodes::texture::TextureConfig;
        let mut node_details = BTreeMap::new();
        node_details.insert(
            NodeHandle::new(1),
            NodeDetail {
                path: LpPathBuf::from("/src/texture.texture"),
                config: Box::new(TextureConfig {
                    width: 100,
                    height: 200,
                }),
                state: NodeState::Texture(crate::nodes::texture::TextureState {
                    texture_data: vec![0, 1, 2, 3],
                    width: 2,
                    height: 2,
                    format: "RGBA8".to_string(),
                }),
            },
        );

        let response = ProjectResponse::GetChanges {
            current_frame: FrameId::default(),
            node_handles: vec![NodeHandle::new(1)],
            node_changes: vec![],
            node_details,
            theoretical_fps: None,
        };

        let serializable = response.to_serializable().unwrap();
        match serializable {
            SerializableProjectResponse::GetChanges {
                current_frame,
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
                        .any(|(handle, _)| *handle == NodeHandle::new(1))
                );
            }
        }
    }

    #[test]
    fn test_serializable_node_detail_serialization() {
        use crate::nodes::texture::TextureConfig;
        let detail = SerializableNodeDetail::Texture {
            path: LpPathBuf::from("/src/texture.texture"),
            config: TextureConfig {
                width: 100,
                height: 200,
            },
            state: NodeState::Texture(crate::nodes::texture::TextureState {
                texture_data: vec![0, 1, 2, 3],
                width: 2,
                height: 2,
                format: "RGBA8".to_string(),
            }),
        };
        let json = serde_json::to_string(&detail).unwrap();
        let deserialized: SerializableNodeDetail = serde_json::from_str(&json).unwrap();
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
    fn test_serializable_project_response_serialization() {
        use crate::nodes::texture::TextureConfig;
        let mut node_details = Vec::new();
        node_details.push((
            NodeHandle::new(1),
            SerializableNodeDetail::Texture {
                path: LpPathBuf::from("/src/texture.texture"),
                config: TextureConfig {
                    width: 100,
                    height: 200,
                },
                state: NodeState::Texture(crate::nodes::texture::TextureState {
                    texture_data: vec![0, 1, 2, 3],
                    width: 2,
                    height: 2,
                    format: "RGBA8".to_string(),
                }),
            },
        ));

        let response = SerializableProjectResponse::GetChanges {
            current_frame: FrameId::default(),
            node_handles: vec![NodeHandle::new(1)],
            node_changes: vec![],
            node_details,
            theoretical_fps: None,
        };

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: SerializableProjectResponse = serde_json::from_str(&json).unwrap();
        match deserialized {
            SerializableProjectResponse::GetChanges {
                current_frame,
                node_handles,
                node_changes,
                node_details,
                theoretical_fps: _,
            } => {
                assert_eq!(current_frame, FrameId::default());
                assert_eq!(node_handles.len(), 1);
                assert_eq!(node_changes.len(), 0);
                assert_eq!(node_details.len(), 1);
            }
        }
    }
}
