use alloc::boxed::Box;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::String;
use alloc::{vec, vec::Vec};
use lp_model::{
    FrameId, LpPathBuf, NodeConfig, NodeHandle, NodeKind,
    project::api::{ApiNodeSpecifier, NodeChange, NodeState, NodeStatus},
};

/// Status change information
#[derive(Debug, Clone)]
pub struct StatusChange {
    /// Node path
    pub path: LpPathBuf,
    /// Previous status
    pub old_status: NodeStatus,
    /// New status
    pub new_status: NodeStatus,
}

/// Client view of project
pub struct ClientProjectView {
    /// Current frame ID (last synced)
    pub frame_id: FrameId,
    /// Node entries
    pub nodes: BTreeMap<NodeHandle, ClientNodeEntry>,
    /// Which nodes we're tracking detail for
    pub detail_tracking: BTreeSet<NodeHandle>,
    /// Previous status for each node (for detecting status changes)
    previous_status: BTreeMap<NodeHandle, NodeStatus>,
}

/// Client node entry
pub struct ClientNodeEntry {
    pub path: LpPathBuf,
    pub kind: NodeKind,
    pub config: Box<dyn NodeConfig>, // todo!("Proper config storage/cloning")
    pub config_ver: FrameId,
    pub state: Option<NodeState>, // Only present if in detail_tracking
    pub state_ver: FrameId,
    pub status: NodeStatus,
    pub status_ver: FrameId,
}

impl ClientProjectView {
    /// Create new client view
    pub fn new() -> Self {
        Self {
            frame_id: FrameId::default(),
            nodes: BTreeMap::new(),
            detail_tracking: BTreeSet::new(),
            previous_status: BTreeMap::new(),
        }
    }

    /// Start tracking detail for a node
    pub fn watch_detail(&mut self, handle: NodeHandle) {
        self.detail_tracking.insert(handle);
    }

    /// Stop tracking detail for a node
    pub fn unwatch_detail(&mut self, handle: NodeHandle) {
        self.detail_tracking.remove(&handle);
        // Clear state when stopping detail
        if let Some(entry) = self.nodes.get_mut(&handle) {
            entry.state = None;
        }
    }

    /// Generate detail specifier for sync
    pub fn detail_specifier(&self) -> ApiNodeSpecifier {
        if self.detail_tracking.is_empty() {
            ApiNodeSpecifier::None
        } else {
            ApiNodeSpecifier::ByHandles(self.detail_tracking.iter().copied().collect())
        }
    }

    /// Sync with server (update view from response)
    ///
    /// Returns a list of all status changes that the caller can use for logging or other purposes.
    pub fn apply_changes(
        &mut self,
        response: &lp_model::project::api::ProjectResponse,
    ) -> Result<Vec<StatusChange>, String> {
        let mut status_changes = Vec::new();
        match response {
            lp_model::project::api::ProjectResponse::GetChanges {
                current_frame,
                node_handles,
                node_changes,
                node_details,
                theoretical_fps: _,
            } => {
                // Update frame ID
                self.frame_id = *current_frame;

                // Prune removed nodes
                let handles_set: BTreeSet<NodeHandle> = node_handles.iter().copied().collect();
                self.nodes.retain(|handle, _| handles_set.contains(handle));

                // Apply changes
                for change in node_changes {
                    match change {
                        NodeChange::Created { handle, path, kind } => {
                            // Create new entry with placeholder config
                            let config: Box<dyn NodeConfig> = match kind {
                                NodeKind::Texture => {
                                    Box::new(lp_model::nodes::texture::TextureConfig {
                                        width: 0,
                                        height: 0,
                                    })
                                }
                                NodeKind::Shader => {
                                    Box::new(lp_model::nodes::shader::ShaderConfig::default())
                                }
                                NodeKind::Output => {
                                    Box::new(lp_model::nodes::output::OutputConfig::GpioStrip {
                                        pin: 0,
                                    })
                                }
                                NodeKind::Fixture => {
                                    Box::new(lp_model::nodes::fixture::FixtureConfig {
                                        output_spec: lp_model::NodeSpecifier::from(""),
                                        texture_spec: lp_model::NodeSpecifier::from(""),
                                        mapping:
                                            lp_model::nodes::fixture::MappingConfig::PathPoints {
                                                paths: vec![],
                                                sample_diameter: 2.0,
                                            },
                                        color_order: lp_model::nodes::fixture::ColorOrder::Rgb,
                                        transform: [[0.0; 4]; 4],
                                        brightness: None,
                                        gamma_correction: None,
                                    })
                                }
                            };

                            let initial_status = NodeStatus::Created;
                            self.nodes.insert(
                                *handle,
                                ClientNodeEntry {
                                    path: path.clone(),
                                    kind: *kind,
                                    config,
                                    config_ver: FrameId::default(),
                                    state: None,
                                    state_ver: FrameId::default(),
                                    status: initial_status.clone(),
                                    status_ver: FrameId::default(),
                                },
                            );
                            // Track initial status
                            self.previous_status.insert(*handle, initial_status);
                        }
                        NodeChange::ConfigUpdated { handle, config_ver } => {
                            if let Some(entry) = self.nodes.get_mut(handle) {
                                entry.config_ver = *config_ver;
                                // todo!("Update config from details if available")
                            }
                        }
                        NodeChange::StateUpdated { handle, state_ver } => {
                            if let Some(entry) = self.nodes.get_mut(handle) {
                                entry.state_ver = *state_ver;
                                // todo!("Update state from details if tracking")
                            }
                        }
                        NodeChange::StatusChanged { handle, status } => {
                            if let Some(entry) = self.nodes.get_mut(handle) {
                                let old_status = entry.status.clone();
                                let new_status = status.clone();

                                // Track all status changes - StatusChanged event indicates a change occurred
                                status_changes.push(StatusChange {
                                    path: entry.path.clone(),
                                    old_status: old_status.clone(),
                                    new_status: new_status.clone(),
                                });

                                entry.status = new_status.clone();
                                // Update status_ver - we use frame_id as proxy since StatusChanged
                                // doesn't include status_ver. The actual status_ver from server
                                // triggered this event, so we use current frame_id.
                                entry.status_ver = self.frame_id;
                                // Update previous status for next comparison
                                self.previous_status.insert(*handle, new_status);
                            } else {
                                // Node doesn't exist yet - just track the status
                                self.previous_status.insert(*handle, status.clone());
                            }
                        }
                        NodeChange::Removed { handle } => {
                            self.nodes.remove(handle);
                            self.detail_tracking.remove(handle);
                            self.previous_status.remove(handle);
                        }
                    }
                }

                // Update details (create entries if they don't exist)
                for (handle, detail) in node_details {
                    if let Some(entry) = self.nodes.get_mut(handle) {
                        // Update existing entry
                        // Use config from detail if available, otherwise keep existing config
                        let config: Box<dyn NodeConfig> = match entry.kind {
                            NodeKind::Texture => {
                                Box::new(lp_model::nodes::texture::TextureConfig {
                                    width: 0,
                                    height: 0,
                                })
                            }
                            NodeKind::Shader => {
                                Box::new(lp_model::nodes::shader::ShaderConfig::default())
                            }
                            NodeKind::Output => {
                                Box::new(lp_model::nodes::output::OutputConfig::GpioStrip {
                                    pin: 0,
                                })
                            }
                            NodeKind::Fixture => {
                                Box::new(lp_model::nodes::fixture::FixtureConfig {
                                    output_spec: lp_model::NodeSpecifier::from(""),
                                    texture_spec: lp_model::NodeSpecifier::from(""),
                                    mapping: lp_model::nodes::fixture::MappingConfig::PathPoints {
                                        paths: vec![],
                                        sample_diameter: 2.0,
                                    },
                                    color_order: lp_model::nodes::fixture::ColorOrder::Rgb,
                                    transform: [[0.0; 4]; 4],
                                    brightness: None,
                                    gamma_correction: None,
                                })
                            }
                        };

                        entry.config = config;
                        entry.state = Some(detail.state.clone());
                        // Status is no longer in node_details, it comes via StatusChanged events
                    } else {
                        // Create new entry from detail (node exists but wasn't in Created changes)
                        // Note: NodeDetail doesn't have kind field, so we infer from state
                        let kind = match &detail.state {
                            NodeState::Texture(_) => NodeKind::Texture,
                            NodeState::Shader(_) => NodeKind::Shader,
                            NodeState::Output(_) => NodeKind::Output,
                            NodeState::Fixture(_) => NodeKind::Fixture,
                        };

                        let config: Box<dyn NodeConfig> = match kind {
                            NodeKind::Texture => {
                                Box::new(lp_model::nodes::texture::TextureConfig {
                                    width: 0,
                                    height: 0,
                                })
                            }
                            NodeKind::Shader => {
                                Box::new(lp_model::nodes::shader::ShaderConfig::default())
                            }
                            NodeKind::Output => {
                                Box::new(lp_model::nodes::output::OutputConfig::GpioStrip {
                                    pin: 0,
                                })
                            }
                            NodeKind::Fixture => {
                                Box::new(lp_model::nodes::fixture::FixtureConfig {
                                    output_spec: lp_model::NodeSpecifier::from(""),
                                    texture_spec: lp_model::NodeSpecifier::from(""),
                                    mapping: lp_model::nodes::fixture::MappingConfig::PathPoints {
                                        paths: vec![],
                                        sample_diameter: 2.0,
                                    },
                                    color_order: lp_model::nodes::fixture::ColorOrder::Rgb,
                                    transform: [[0.0; 4]; 4],
                                    brightness: None,
                                    gamma_correction: None,
                                })
                            }
                        };

                        self.nodes.insert(
                            *handle,
                            ClientNodeEntry {
                                path: detail.path.clone(),
                                kind,
                                config,
                                config_ver: FrameId::default(),
                                state: Some(detail.state.clone()),
                                state_ver: FrameId::default(),
                                status: NodeStatus::Created,
                                status_ver: FrameId::default(),
                            },
                        );
                        // Status will come via StatusChanged events, initialize to Created
                        self.previous_status.insert(*handle, NodeStatus::Created);
                    }
                }

                Ok(status_changes)
            }
        }
    }

    /// Get texture data for a node handle
    ///
    /// Returns the texture data bytes, or an error if:
    /// - The node doesn't exist
    /// - The node is not a texture node
    /// - The node doesn't have state (not being tracked for detail)
    pub fn get_texture_data(&self, handle: NodeHandle) -> Result<Vec<u8>, String> {
        let entry = self
            .nodes
            .get(&handle)
            .ok_or_else(|| format!("Node handle {} not found in client view", handle.as_i32()))?;

        if entry.kind != NodeKind::Texture {
            return Err(format!(
                "Node {} is not a texture node (kind: {:?})",
                entry.path.as_str(),
                entry.kind
            ));
        }

        match &entry.state {
            Some(NodeState::Texture(tex_state)) => Ok(tex_state.texture_data.clone()),
            Some(_) => Err(format!(
                "Node {} has wrong state type (expected Texture)",
                entry.path.as_str()
            )),
            None => Err(format!(
                "Node {} does not have state (not being tracked for detail)",
                entry.path.as_str()
            )),
        }
    }

    /// Get output channel data for a node handle
    ///
    /// Returns the output channel data bytes, or an error if:
    /// - The node doesn't exist
    /// - The node is not an output node
    /// - The node doesn't have state (not being tracked for detail)
    pub fn get_output_data(&self, handle: NodeHandle) -> Result<Vec<u8>, String> {
        let entry = self
            .nodes
            .get(&handle)
            .ok_or_else(|| format!("Node handle {} not found in client view", handle.as_i32()))?;

        if entry.kind != NodeKind::Output {
            return Err(format!(
                "Node {} is not an output node (kind: {:?})",
                entry.path.as_str(),
                entry.kind
            ));
        }

        match &entry.state {
            Some(NodeState::Output(output_state)) => Ok(output_state.channel_data.clone()),
            Some(_) => Err(format!(
                "Node {} has wrong state type (expected Output)",
                entry.path.as_str()
            )),
            None => Err(format!(
                "Node {} does not have state (not being tracked for detail)",
                entry.path.as_str()
            )),
        }
    }
}
